//! Execution engine: LLM client, MCP server pool, tool routing, and tracing.
//!
//! The engine bridges the user's LLM and their MCP servers. It spawns servers,
//! routes model tool calls to the correct server, records full execution traces
//! (prompt → model output → tool call → tool result → timing), and supports
//! pass^k reliability testing.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::ModelConfig;
use crate::exercises::Exercise;
use crate::prompts::{self, AssembledPrompt, PromptConfig};

/// Errors from the execution engine.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// LLM API call failed.
    #[error("LLM call failed: {0}")]
    LlmError(String),
    /// Tool call routing failed.
    #[error("tool routing failed for '{tool}': {reason}")]
    RoutingError {
        /// Tool name.
        tool: String,
        /// Failure reason.
        reason: String,
    },
    /// Timeout during execution.
    #[error("execution timed out after {0:?}")]
    Timeout(Duration),
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    /// JSON parse error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// A complete execution trace for one exercise run.
///
/// Captures the full decision chain from prompt to final result, with timing
/// at each step. This is the input to the scoring system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    /// Exercise ID this trace corresponds to.
    pub exercise_id: String,
    /// The prompt sent to the model.
    pub prompt: AssembledPrompt,
    /// Raw model output text.
    pub model_output: String,
    /// Reasoning content (if thinking model).
    #[serde(default)]
    pub reasoning_content: Option<String>,
    /// Tool calls extracted from the model output.
    pub tool_calls: Vec<ToolCallRecord>,
    /// Total execution time.
    pub duration: Duration,
    /// Whether this was a successful execution (no errors).
    pub success: bool,
    /// Error message if execution failed.
    #[serde(default)]
    pub error: Option<String>,
}

/// A single tool call and its result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    /// Tool name as specified by the model.
    pub tool_name: String,
    /// Parameters passed to the tool.
    pub params: Value,
    /// Result returned by the tool (or error).
    pub result: ToolCallResult,
    /// Time taken for this tool call.
    pub duration: Duration,
}

/// Result of a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ToolCallResult {
    /// Tool call succeeded.
    #[serde(rename = "success")]
    Success {
        /// Tool output.
        output: Value,
    },
    /// Tool call failed.
    #[serde(rename = "error")]
    Error {
        /// Error message.
        message: String,
    },
}

// ── LLM Client ──────────────────────────────────────────────────────────────

/// OpenAI-compatible chat completion response.
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

/// A single choice from the chat completion response.
#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

/// Message from the chat completion response.
#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

/// Call the LLM with a prompt and return the model's response.
///
/// # Errors
///
/// Returns [`EngineError`] if the HTTP call fails or the response cannot
/// be parsed.
pub async fn call_llm(
    client: &reqwest::Client,
    config: &ModelConfig,
    messages: &[Value],
) -> Result<(String, Option<String>), EngineError> {
    let api_key = config
        .api_key_env
        .as_deref()
        .and_then(|env_name| std::env::var(env_name).ok())
        .unwrap_or_default();

    let body = serde_json::json!({
        "model": config.name,
        "messages": messages,
        "max_tokens": config.max_tokens,
        "temperature": config.temperature,
    });

    let mut req = client
        .post(format!("{}/chat/completions", config.endpoint))
        .json(&body);

    if !api_key.is_empty() {
        req = req.bearer_auth(&api_key);
    }

    let resp = req.send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(EngineError::LlmError(format!("HTTP {status}: {body}")));
    }

    let completion: ChatCompletionResponse = resp.json().await?;

    let choice = completion
        .choices
        .first()
        .ok_or_else(|| EngineError::LlmError("no choices in response".into()))?;

    let content = choice.message.content.clone().unwrap_or_default();
    let reasoning = choice.message.reasoning_content.clone();

    Ok((content, reasoning))
}

// ── Tool Call Parsing ───────────────────────────────────────────────────────

/// Maximum allowed length for a parsed tool name.
const MAX_TOOL_NAME_LEN: usize = 256;

/// Parse a tool call from the model's text output.
///
/// Looks for JSON containing `"tool"` and `"params"` keys. Handles nested
/// JSON with balanced brace matching.
#[must_use]
pub fn parse_tool_call(output: &str) -> Option<(String, Value)> {
    // Find the first `{` and match braces.
    let start = output.find('{')?;
    let json_str = extract_balanced_json(&output[start..])?;

    let parsed: Value = serde_json::from_str(&json_str).ok()?;

    let tool_name = parsed
        .get("tool")
        .and_then(Value::as_str)
        .map(String::from)?;

    // RT-08: Reject malformed tool names before they can poison routing or logs.
    // Names must be non-empty, <= 256 chars, and contain only [A-Za-z0-9_].
    if tool_name.is_empty()
        || tool_name.len() > MAX_TOOL_NAME_LEN
        || !tool_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }

    let params = parsed
        .get("params")
        .cloned()
        .unwrap_or(Value::Object(serde_json::Map::new()));

    Some((tool_name, params))
}

/// Extract a balanced JSON object from a string starting with `{`.
fn extract_balanced_json(s: &str) -> Option<String> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in s.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[..=i].to_owned());
                }
            }
            _ => {}
        }
    }
    None
}

// ── Exercise Execution ──────────────────────────────────────────────────────

/// Execute a single exercise against the model (without MCP server interaction).
///
/// This is a "dry" execution — it sends the prompt to the LLM and records
/// the response. Tool calls are parsed but NOT routed to real servers.
/// For real execution, use the full pipeline in Phase 9.
///
/// # Errors
///
/// Returns [`EngineError`] on LLM communication failure.
pub async fn execute_exercise(
    client: &reqwest::Client,
    model_config: &ModelConfig,
    exercise: &Exercise,
    prompt_config: &PromptConfig,
) -> Result<Trace, EngineError> {
    let start = Instant::now();

    let assembled = prompts::assemble(exercise, prompt_config);
    let messages = prompts::to_chat_messages(&assembled);

    let (output, reasoning) = call_llm(client, model_config, &messages).await?;

    // Parse tool calls from output.
    let mut tool_calls = Vec::new();
    if let Some((tool_name, params)) = parse_tool_call(&output) {
        tool_calls.push(ToolCallRecord {
            tool_name,
            params,
            result: ToolCallResult::Success {
                output: Value::String("(dry run — not executed)".into()),
            },
            duration: Duration::ZERO,
        });
    }

    let duration = start.elapsed();

    Ok(Trace {
        exercise_id: exercise.id.clone(),
        prompt: assembled,
        model_output: output,
        reasoning_content: reasoning,
        tool_calls,
        duration,
        success: true,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_tool_call() {
        let output = r#"I'll use the weather tool: {"tool": "get_weather", "params": {"location": "London"}}"#;
        let (name, params) = parse_tool_call(output).expect("should parse");
        assert_eq!(name, "get_weather");
        assert_eq!(params["location"], "London");
    }

    #[test]
    fn parse_nested_json() {
        let output = r#"{"tool": "query", "params": {"filter": {"age": {"$gt": 18}}}}"#;
        let (name, params) = parse_tool_call(output).expect("should parse");
        assert_eq!(name, "query");
        assert_eq!(params["filter"]["age"]["$gt"], 18);
    }

    #[test]
    fn parse_no_params() {
        let output = r#"{"tool": "list_tables"}"#;
        let (name, params) = parse_tool_call(output).expect("should parse");
        assert_eq!(name, "list_tables");
        assert!(params.is_object());
    }

    #[test]
    fn parse_no_json_returns_none() {
        let output = "I don't think any tool is needed here.";
        assert!(parse_tool_call(output).is_none());
    }

    #[test]
    fn parse_malformed_json_returns_none() {
        let output = r#"{"tool": "bad", "params": {"unclosed": true"#;
        assert!(parse_tool_call(output).is_none());
    }

    #[test]
    fn balanced_json_extraction() {
        let s = r#"{"a": {"b": 1}, "c": 2} trailing text"#;
        let result = extract_balanced_json(s).expect("should extract");
        assert_eq!(result, r#"{"a": {"b": 1}, "c": 2}"#);
    }

    #[test]
    fn balanced_json_with_strings() {
        let s = r#"{"key": "value with { braces }"}"#;
        let result = extract_balanced_json(s).expect("should extract");
        assert_eq!(result, s);
    }

    #[test]
    fn trace_serializes() {
        let trace = Trace {
            exercise_id: "test-1".into(),
            prompt: AssembledPrompt {
                system: "sys".into(),
                user: "usr".into(),
                objective: crate::config::OutputFormat::Sft,
            },
            model_output: "output".into(),
            reasoning_content: None,
            tool_calls: vec![],
            duration: Duration::from_millis(100),
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&trace).expect("serialize");
        assert!(json.contains("test-1"));
    }

    #[test]
    fn tool_call_result_variants() {
        let success = ToolCallResult::Success {
            output: Value::String("ok".into()),
        };
        let json = serde_json::to_string(&success).expect("ser");
        assert!(json.contains("success"));

        let error = ToolCallResult::Error {
            message: "timeout".into(),
        };
        let json = serde_json::to_string(&error).expect("ser");
        assert!(json.contains("error"));
    }
}
