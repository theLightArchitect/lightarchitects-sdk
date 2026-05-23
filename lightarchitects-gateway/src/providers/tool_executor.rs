//! [`GatewayToolExecutor`] — bridges the SDK [`ToolExecutor`] trait to the
//! gateway's `core_tools/*` handlers.
//!
//! Exposes six core tools to the LLM: `bash`, `read`, `write`, `edit`,
//! `glob`, `search`. Each maps to the corresponding `core_tools` handler.
//! Error conversion maps [`GatewayError`] variants to typed [`ToolError`]s.

use std::sync::Arc;

use async_trait::async_trait;
use lightarchitects::agent::{ToolDefinition, ToolError, ToolExecutor, ToolOutput};
use serde_json::{Value, json};

use crate::config::GatewayConfig;
use crate::core_tools::{bash, edit, glob, read, search, write};
use crate::error::GatewayError;

/// Tool executor that dispatches LLM `tool_use` blocks to gateway core tools.
///
/// Holds a shared [`GatewayConfig`] and exposes six tools:
/// `bash`, `read`, `write`, `edit`, `glob`, `search`.
pub struct GatewayToolExecutor {
    config: Arc<GatewayConfig>,
}

impl GatewayToolExecutor {
    /// Create a new executor backed by the given config.
    #[must_use]
    pub fn new(config: Arc<GatewayConfig>) -> Self {
        Self { config }
    }
}

/// Convert a [`GatewayError`] into a [`ToolError`], preserving semantic detail.
fn gateway_err_to_tool_err(tool_name: &str, err: GatewayError) -> ToolError {
    match err {
        GatewayError::MissingParam(p) => ToolError::InvalidInput {
            tool_name: tool_name.to_owned(),
            reason: format!("missing required parameter: {p}"),
        },
        GatewayError::InvalidParam(reason) | GatewayError::InvalidRequest(reason) => {
            ToolError::InvalidInput {
                tool_name: tool_name.to_owned(),
                reason,
            }
        }
        GatewayError::UnknownTool(t) => ToolError::UnknownTool(t),
        GatewayError::Governance { reason, .. } => ToolError::PermissionDenied {
            tool_name: tool_name.to_owned(),
            reason,
        },
        other => ToolError::Internal(format!("{other}")),
    }
}

/// Extract a plain text string from the MCP tool-result envelope.
fn result_to_content(v: Value) -> Value {
    // Standard envelope: {"content":[{"type":"text","text":"..."}]}
    // Pass it through unchanged — callers can inspect the structure.
    v
}

#[async_trait]
impl ToolExecutor for GatewayToolExecutor {
    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "bash".to_owned(),
                description: "Execute a shell command and return stdout + stderr. Non-zero exit codes are not errors — the exit code is embedded in the response.".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "command":    {"type": "string",  "description": "Shell command to run."},
                        "timeout_ms": {"type": "integer", "description": "Timeout in milliseconds (default 120 000)."},
                        "cwd":        {"type": "string",  "description": "Working directory for the command."}
                    },
                    "required": ["command"]
                }),
            },
            ToolDefinition {
                name: "read".to_owned(),
                description: "Read the contents of a file, optionally selecting a line range.".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path":   {"type": "string",  "description": "File path (~/... expanded)."},
                        "offset": {"type": "integer", "description": "1-indexed first line to return."},
                        "limit":  {"type": "integer", "description": "Maximum number of lines to return."}
                    },
                    "required": ["path"]
                }),
            },
            ToolDefinition {
                name: "write".to_owned(),
                description: "Write content to a file, creating or replacing it.".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path":    {"type": "string", "description": "Destination file path."},
                        "content": {"type": "string", "description": "File contents to write."}
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolDefinition {
                name: "edit".to_owned(),
                description: "Replace an exact substring in a file with new text.".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path":        {"type": "string",  "description": "File to edit."},
                        "old_string":  {"type": "string",  "description": "Exact text to find (must be unique)."},
                        "new_string":  {"type": "string",  "description": "Replacement text."},
                        "replace_all": {"type": "boolean", "description": "Replace every occurrence (default false)."}
                    },
                    "required": ["path", "old_string", "new_string"]
                }),
            },
            ToolDefinition {
                name: "glob".to_owned(),
                description: "List files matching a glob pattern in a directory.".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string", "description": "Glob pattern (e.g. '**/*.rs')."},
                        "cwd":     {"type": "string", "description": "Root directory (defaults to project root)."}
                    },
                    "required": ["pattern"]
                }),
            },
            ToolDefinition {
                name: "search".to_owned(),
                description: "Search for a regex pattern in files (ripgrep / grep fallback).".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string",  "description": "Regex pattern to search for."},
                        "path":    {"type": "string",  "description": "Directory or file to search in."},
                        "glob":    {"type": "string",  "description": "Limit to files matching this glob."},
                        "context": {"type": "integer", "description": "Lines of context around each match."}
                    },
                    "required": ["pattern"]
                }),
            },
        ]
    }

    async fn execute(
        &self,
        tool_use_id: &str,
        tool_name: &str,
        input: Value,
    ) -> Result<ToolOutput, ToolError> {
        let config = Arc::clone(&self.config);
        let tool_use_id = tool_use_id.to_owned();

        let result: Result<Value, GatewayError> = match tool_name {
            "bash" => bash::run(input).await,
            "read" => {
                let cfg = Arc::clone(&config);
                match tokio::task::spawn_blocking(move || read::run(input, &cfg)).await {
                    Ok(r) => r,
                    Err(e) => Err(GatewayError::Internal(format!("read task panicked: {e}"))),
                }
            }
            "write" => {
                let cfg = Arc::clone(&config);
                match tokio::task::spawn_blocking(move || write::run(input, &cfg)).await {
                    Ok(r) => r,
                    Err(e) => Err(GatewayError::Internal(format!("write task panicked: {e}"))),
                }
            }
            "edit" => {
                let cfg = Arc::clone(&config);
                match tokio::task::spawn_blocking(move || edit::run(input, &cfg)).await {
                    Ok(r) => r,
                    Err(e) => Err(GatewayError::Internal(format!("edit task panicked: {e}"))),
                }
            }
            "glob" => glob::run(input, &config).await,
            "search" => search::run(input, &config).await,
            unknown => return Err(ToolError::UnknownTool(unknown.to_owned())),
        };

        match result {
            Ok(v) => Ok(ToolOutput {
                tool_use_id,
                content: result_to_content(v),
                is_error: false,
            }),
            Err(e) => {
                // Application-level errors become is_error=true ToolOutputs rather than
                // ToolErrors so the LLM can observe the failure and react (e.g., retry
                // with a corrected command).
                let message = e.to_string();
                let is_perm = matches!(e, GatewayError::Governance { .. });
                if is_perm {
                    Err(gateway_err_to_tool_err(tool_name, e))
                } else {
                    Ok(ToolOutput {
                        tool_use_id,
                        content: json!({
                            "content": [{"type": "text", "text": message}]
                        }),
                        is_error: true,
                    })
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_executor() -> GatewayToolExecutor {
        GatewayToolExecutor::new(Arc::new(GatewayConfig::default()))
    }

    #[test]
    fn tool_definitions_returns_six_tools() {
        let exec = make_executor();
        let defs = exec.tool_definitions();
        assert_eq!(defs.len(), 6);
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read"));
        assert!(names.contains(&"write"));
        assert!(names.contains(&"edit"));
        assert!(names.contains(&"glob"));
        assert!(names.contains(&"search"));
    }

    #[tokio::test]
    async fn execute_unknown_tool_returns_unknown_tool_error() {
        let exec = make_executor();
        let err = exec
            .execute("id_1", "nonexistent_tool", json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::UnknownTool(ref n) if n == "nonexistent_tool"));
    }

    #[tokio::test]
    async fn execute_bash_missing_command_returns_is_error_output() {
        // Missing `command` parameter → core tool returns MissingParam error.
        // GatewayToolExecutor converts non-permission errors to is_error outputs
        // so the LLM can observe and retry.
        let exec = make_executor();
        let out = exec.execute("id_2", "bash", json!({})).await.unwrap();
        assert!(out.is_error);
    }

    #[tokio::test]
    async fn execute_bash_echo_succeeds() {
        let exec = make_executor();
        let out = exec
            .execute("id_3", "bash", json!({"command": "echo hello"}))
            .await
            .unwrap();
        assert!(!out.is_error);
        let text = out.content["content"][0]["text"].as_str().unwrap_or("");
        assert!(text.contains("hello"));
    }
}
