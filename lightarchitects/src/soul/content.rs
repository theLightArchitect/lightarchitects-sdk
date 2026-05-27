//! MCP content-block unwrapping for SOUL responses.
//!
//! All SOUL tools (via `soulTools` orchestrator) wrap their responses in the
//! standard MCP `ToolCallResult` envelope:
//!
//! ```json
//! {
//!   "content": [{ "type": "text", "text": "<JSON string>" }],
//!   "isError": false
//! }
//! ```
//!
//! [`unwrap_json`] extracts `content[].text` and parses it as JSON — used by
//! every `call()` method in the SOUL SDK builder chain.

use serde_json::Value;

use crate::core::error::{ProtocolError, SdkError, ToolError};

/// Extract and JSON-parse the first content-block text from a `ToolCallResult`.
///
/// `action` is the SOUL action name (e.g. `"graphrag_ingest"`, `"helix"`) and
/// is included in any error messages to aid debugging.
///
/// # Errors
///
/// Returns [`SdkError::Tool`] when `isError` is `true`.
/// Returns [`SdkError::Protocol`] when the envelope is malformed or `text`
/// is not valid JSON.
pub(crate) fn unwrap_json(value: Value, action: &str) -> Result<Value, SdkError> {
    let text = extract_text(value, action)?;
    serde_json::from_str(&text).map_err(|e| {
        SdkError::Protocol(ProtocolError::MalformedJson(format!(
            "SOUL `{action}` content block text is not valid JSON: {e}"
        )))
    })
}

/// Shared extraction logic: validates the envelope and returns `content[].text`.
fn extract_text(value: Value, action: &str) -> Result<String, SdkError> {
    let is_error = value
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let text = value
        .get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| {
            blocks
                .iter()
                .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        })
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            SdkError::Protocol(ProtocolError::UnexpectedShape(format!(
                "SOUL `{action}` response missing content[].text block"
            )))
        })?;

    if is_error {
        return Err(SdkError::Tool(ToolError {
            tool: action.to_owned(),
            message: text,
        }));
    }

    Ok(text)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn envelope(text: &str, is_error: bool) -> Value {
        json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
    }

    #[test]
    fn unwrap_json_parses_object() {
        let input = envelope(r#"{"nodes_created":1,"edges_created":0}"#, false);
        let result = unwrap_json(input, "graphrag_ingest").unwrap();
        assert_eq!(result["nodes_created"], 1);
    }

    #[test]
    fn unwrap_json_returns_tool_error_on_is_error() {
        let input = envelope("Neo4j unavailable", true);
        let err = unwrap_json(input, "graphrag_ingest").unwrap_err();
        assert!(matches!(err, SdkError::Tool(_)));
    }

    #[test]
    fn unwrap_json_fails_on_non_json_text() {
        let input = envelope("not json", false);
        let err = unwrap_json(input, "graphrag_ingest").unwrap_err();
        assert!(matches!(err, SdkError::Protocol(_)));
    }

    #[test]
    fn unwrap_json_missing_content_field() {
        let input = json!({ "isError": false });
        let err = unwrap_json(input, "helix").unwrap_err();
        assert!(matches!(err, SdkError::Protocol(_)));
    }
}
