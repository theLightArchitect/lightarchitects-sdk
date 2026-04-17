//! Ollama `/api/chat` client for Arena tool-calling workflows.
//!
//! Speaks Ollama's native tool-calling protocol:
//! - Request: `POST /api/chat` with `tools` array (JSON Schema functions)
//! - Response: NDJSON stream — one `StreamChunk` per line until `done: true`
//! - Tool results: `role: "tool"` messages fed back into the conversation
//!
//! Streaming is always enabled (`stream: true`). This means:
//! - Tokens arrive immediately as they are generated
//! - If Ollama crashes mid-generation, the stream closes and the error is
//!   surfaced instantly rather than after the full `CHAT_TIMEOUT` elapses
//! - Tool calls accumulate across chunks and are returned when `done: true`
//!
//! The client is stateless — each call is a fresh HTTP request. Conversation
//! history is managed by the caller (Arena's spar loop).

use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::GatewayError;

/// Default Ollama endpoint (local).
const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Timeout applied to the HTTP client (covers connection + total generation).
const CHAT_TIMEOUT: Duration = Duration::from_secs(120);

// ── Types ─────────────────────────────────────────────────────────────────────

/// A message in the Ollama chat format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Message role: "system", "user", "assistant", or "tool".
    pub role: String,
    /// Text content (absent when the model returns only `tool_calls`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool calls the model wants to execute (absent for non-assistant messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// A tool call returned by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// The function call details (name + arguments).
    pub function: FunctionCall,
}

/// The function name + arguments within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Tool name the model wants to call.
    pub name: String,
    /// Structured arguments for the tool.
    pub arguments: Value,
}

/// Ollama `/api/chat` request body.
#[derive(Debug, Serialize)]
struct ChatRequest {
    /// Model tag (e.g., "qwen3-14b").
    model: String,
    /// Conversation messages.
    messages: Vec<ChatMessage>,
    /// Tool definitions for the model to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
    /// Always `true` — streaming provides faster crash detection.
    stream: bool,
}

/// Assembled result returned to callers after the stream completes.
#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    /// The assembled assistant response.
    pub message: ChatMessage,
    /// Always `true` for a completed stream.
    #[serde(default)]
    pub done: bool,
}

/// A single line from an Ollama streaming response (NDJSON).
#[derive(Debug, Deserialize)]
struct StreamChunk {
    message: ChatMessage,
    #[serde(default)]
    done: bool,
}

// ── Tool definitions ──────────────────────────────────────────────────────────

/// Build Ollama-format tool definitions from the gateway's core tools.
///
/// These are the tools the model can call during a spar exercise.
/// Only read-only tools are exposed — write, edit, and bash are blocked
/// in spar mode to prevent training exercises from modifying files or
/// executing arbitrary commands.
pub fn gateway_tool_defs() -> Vec<Value> {
    vec![
        tool_def(
            "read",
            "Read file contents",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to read"}
                },
                "required": ["path"]
            }),
        ),
        tool_def(
            "search",
            "Search file contents using regex",
            json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Regex pattern"},
                    "path": {"type": "string", "description": "Directory to search"}
                },
                "required": ["pattern"]
            }),
        ),
        tool_def(
            "glob",
            "Find files matching a glob pattern",
            json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Glob pattern, e.g. **/*.rs"}
                },
                "required": ["pattern"]
            }),
        ),
    ]
}

/// Build a single Ollama tool definition.
fn tool_def(name: &str, description: &str, parameters: Value) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters,
        }
    })
}

// ── Client ────────────────────────────────────────────────────────────────────

/// Send a streaming chat completion to Ollama and return the assembled response.
///
/// Uses `stream: true` so the connection drops immediately if Ollama crashes,
/// rather than waiting for `CHAT_TIMEOUT` before surfacing the error.
///
/// # Arguments
///
/// * `model` — Ollama model tag (e.g., "qwen3-14b", "nemotron-super:cloud")
/// * `messages` — Conversation history
/// * `tools` — Tool definitions (pass `None` for tool-free completion)
/// * `ollama_url` — Base URL (default: `http://localhost:11434`)
///
/// # Errors
///
/// Returns [`GatewayError::Internal`] on HTTP, stream, or JSON errors.
pub async fn chat(
    model: &str,
    messages: Vec<ChatMessage>,
    tools: Option<Vec<Value>>,
    ollama_url: Option<&str>,
) -> Result<ChatResponse, GatewayError> {
    let base = ollama_url.unwrap_or(DEFAULT_OLLAMA_URL);

    // Security: validate that the Ollama endpoint is localhost-only.
    super::security::validate_local_url(base)?;

    let url = format!("{base}/api/chat");

    let body = ChatRequest {
        model: model.to_owned(),
        messages,
        tools,
        stream: true,
    };

    let client = Client::builder()
        .timeout(CHAT_TIMEOUT)
        .build()
        .map_err(|e| GatewayError::Internal(format!("HTTP client build error: {e}")))?;

    let response = client.post(&url).json(&body).send().await.map_err(|e| {
        if e.is_timeout() {
            GatewayError::Internal(format!(
                "Ollama timeout after {}s — is the model loaded? (model: {model})",
                CHAT_TIMEOUT.as_secs()
            ))
        } else if e.is_connect() {
            GatewayError::Internal(format!(
                "Cannot connect to Ollama at {url} — is Ollama running?"
            ))
        } else {
            GatewayError::Internal(format!("Ollama request failed: {e}"))
        }
    })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(GatewayError::Internal(format!(
            "Ollama returned {status}: {body}"
        )));
    }

    assemble_stream(response).await
}

/// Read an Ollama NDJSON stream and assemble it into a single [`ChatResponse`].
///
/// Each line is a [`StreamChunk`]. Content fragments are concatenated; tool
/// calls are captured from whichever chunk contains them. Returns when the
/// chunk with `done: true` arrives or when the stream closes.
async fn assemble_stream(mut response: reqwest::Response) -> Result<ChatResponse, GatewayError> {
    let mut buf: Vec<u8> = Vec::new();
    let mut content = String::new();
    let mut tool_calls: Option<Vec<ToolCall>> = None;

    while let Some(bytes) = response
        .chunk()
        .await
        .map_err(|e| GatewayError::Internal(format!("Ollama stream read error: {e}")))?
    {
        buf.extend_from_slice(&bytes);

        // Process all complete newline-delimited lines in the buffer.
        loop {
            let Some(pos) = buf.iter().position(|&b| b == b'\n') else {
                break;
            };
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let text = String::from_utf8_lossy(&line);
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            match serde_json::from_str::<StreamChunk>(text) {
                Ok(chunk) => {
                    if let Some(c) = chunk.message.content {
                        if !c.is_empty() {
                            content.push_str(&c);
                        }
                    }
                    if let Some(tc) = chunk.message.tool_calls {
                        tool_calls = Some(tc);
                    }
                    if chunk.done {
                        return Ok(build_response(content, tool_calls));
                    }
                }
                Err(e) => {
                    tracing::debug!(line = text, error = %e, "Ollama stream chunk skipped");
                }
            }
        }
    }

    // Stream closed without a done:true chunk — return what was accumulated.
    Ok(build_response(content, tool_calls))
}

/// Assemble a [`ChatResponse`] from accumulated stream fragments.
fn build_response(content: String, tool_calls: Option<Vec<ToolCall>>) -> ChatResponse {
    ChatResponse {
        message: ChatMessage {
            role: "assistant".to_owned(),
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            tool_calls,
        },
        done: true,
    }
}

/// Check if Ollama is reachable.
pub async fn health(ollama_url: Option<&str>) -> bool {
    let base = ollama_url.unwrap_or(DEFAULT_OLLAMA_URL);
    if super::security::validate_local_url(base).is_err() {
        return false;
    }
    let url = format!("{base}/api/tags");
    reqwest::get(&url)
        .await
        .is_ok_and(|r| r.status().is_success())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn gateway_tool_defs_has_three_readonly_tools() {
        let tools = gateway_tool_defs();
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t["function"]["name"].as_str())
            .collect();
        assert!(names.contains(&"read"));
        assert!(names.contains(&"search"));
        assert!(names.contains(&"glob"));
        // write, edit, bash must NOT be present.
        assert!(!names.contains(&"write"));
        assert!(!names.contains(&"edit"));
        assert!(!names.contains(&"bash"));
    }

    #[test]
    fn tool_def_has_correct_structure() {
        let tools = gateway_tool_defs();
        let read = &tools[0];
        assert_eq!(read["type"], "function");
        assert_eq!(read["function"]["name"], "read");
        assert!(read["function"]["parameters"]["required"].is_array());
    }

    #[test]
    fn chat_message_serializes_without_none_fields() {
        let msg = ChatMessage {
            role: "user".to_owned(),
            content: Some("hello".to_owned()),
            tool_calls: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("tool_calls"));
    }

    #[test]
    fn chat_request_stream_is_true() {
        let req = ChatRequest {
            model: "test".to_owned(),
            messages: vec![],
            tools: None,
            stream: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"stream\":true"));
        assert!(!json.contains("\"stream\":false"));
    }

    #[test]
    fn stream_chunk_deserializes() {
        let line =
            r#"{"model":"qwen3","message":{"role":"assistant","content":"hello"},"done":false}"#;
        let chunk: StreamChunk = serde_json::from_str(line).unwrap();
        assert_eq!(chunk.message.content.as_deref(), Some("hello"));
        assert!(!chunk.done);
    }

    #[test]
    fn stream_chunk_done_flag() {
        let line = r#"{"model":"qwen3","message":{"role":"assistant","content":""},"done":true}"#;
        let chunk: StreamChunk = serde_json::from_str(line).unwrap();
        assert!(chunk.done);
    }

    #[test]
    fn build_response_with_content() {
        let resp = build_response("hi there".to_owned(), None);
        assert_eq!(resp.message.content.as_deref(), Some("hi there"));
        assert!(resp.message.tool_calls.is_none());
        assert!(resp.done);
    }

    #[test]
    fn build_response_empty_content_is_none() {
        let resp = build_response(String::new(), None);
        assert!(resp.message.content.is_none());
    }
}
