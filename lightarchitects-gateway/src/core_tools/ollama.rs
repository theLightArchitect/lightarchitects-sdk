//! Ollama `/api/chat` client for Arena tool-calling workflows.
//!
//! Speaks Ollama's native tool-calling protocol:
//! - Request: `POST /api/chat` with `tools` array (JSON Schema functions)
//! - Response: `message.tool_calls[].function.{name, arguments}`
//! - Tool results: `role: "tool"` messages fed back into the conversation
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

/// Timeout for a single chat completion.
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
    /// Whether to stream the response (always false for Arena).
    stream: bool,
}

/// Ollama `/api/chat` response body.
#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    /// The assistant's response message.
    pub message: ChatMessage,
    /// Whether the response is complete.
    #[serde(default)]
    pub done: bool,
}

// ── Tool definitions ──────────────────────────────────────────────────────────

/// Build Ollama-format tool definitions from the gateway's core tools.
///
/// These are the tools the model can call during a spar exercise.
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
            "write",
            "Create or overwrite a file",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path"},
                    "content": {"type": "string", "description": "File content"}
                },
                "required": ["path", "content"]
            }),
        ),
        tool_def(
            "edit",
            "Replace a string in a file",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path"},
                    "old_string": {"type": "string", "description": "String to find"},
                    "new_string": {"type": "string", "description": "Replacement string"}
                },
                "required": ["path", "old_string", "new_string"]
            }),
        ),
        tool_def(
            "bash",
            "Execute a shell command",
            json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Shell command to execute"}
                },
                "required": ["command"]
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

/// Send a chat completion to Ollama and return the response.
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
/// Returns [`GatewayError::Internal`] on HTTP or JSON errors.
pub async fn chat(
    model: &str,
    messages: Vec<ChatMessage>,
    tools: Option<Vec<Value>>,
    ollama_url: Option<&str>,
) -> Result<ChatResponse, GatewayError> {
    let url = format!("{}/api/chat", ollama_url.unwrap_or(DEFAULT_OLLAMA_URL));

    let body = ChatRequest {
        model: model.to_owned(),
        messages,
        tools,
        stream: false,
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

    response
        .json::<ChatResponse>()
        .await
        .map_err(|e| GatewayError::Internal(format!("Ollama response parse error: {e}")))
}

/// Check if Ollama is reachable.
pub async fn health(ollama_url: Option<&str>) -> bool {
    let url = format!("{}/api/tags", ollama_url.unwrap_or(DEFAULT_OLLAMA_URL));
    reqwest::get(&url)
        .await
        .is_ok_and(|r| r.status().is_success())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_tool_defs_has_six_tools() {
        let tools = gateway_tool_defs();
        assert_eq!(tools.len(), 6);
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
}
