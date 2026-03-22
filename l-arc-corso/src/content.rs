//! MCP content-block unwrapping for CORSO responses.
//!
//! CORSO wraps every `corsoTools` response in the MCP `ToolCallResult`
//! envelope:
//!
//! ```json
//! {
//!   "content": [{ "type": "text", "text": "<JSON string | plain text>" }],
//!   "isError": false
//! }
//! ```
//!
//! [`unwrap_json`] extracts `content[0].text` and parses it as JSON — used by
//! structured-data actions (`read_file`, `list_directory`, `search_code`, …).
//!
//! [`unwrap_text`] extracts `content[0].text` as a raw string — used by
//! AI-analysis actions (`sniff`, `guard`, `generate_code`, …) whose output is
//! prose or mixed content that the caller interprets.

use serde_json::Value;

use l_arc_core::error::{ProtocolError, SdkError, ToolError};

/// Extract and JSON-parse the first content-block text from a `ToolCallResult`.
///
/// # Errors
///
/// Returns [`SdkError::Tool`] when `isError` is `true`.
/// Returns [`SdkError::Protocol`] when the envelope is malformed or `text`
/// is not valid JSON.
pub fn unwrap_json(value: Value) -> Result<Value, SdkError> {
    let text = extract_text(value)?;
    serde_json::from_str(&text).map_err(|e| {
        SdkError::Protocol(ProtocolError::MalformedJson(format!(
            "CORSO content block text is not valid JSON: {e}"
        )))
    })
}

/// Extract the first content-block text from a `ToolCallResult` as a `String`.
///
/// # Errors
///
/// Returns [`SdkError::Tool`] when `isError` is `true`.
/// Returns [`SdkError::Protocol`] when the envelope is malformed.
pub fn unwrap_text(value: Value) -> Result<String, SdkError> {
    extract_text(value)
}

/// Shared extraction logic: validates the envelope and returns `content[0].text`.
fn extract_text(value: Value) -> Result<String, SdkError> {
    let is_error = value
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // Find the first content block with `"type": "text"` rather than blindly
    // indexing [0] — CORSO may prepend progress or image blocks.
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
            SdkError::Protocol(ProtocolError::UnexpectedShape(
                "CORSO response missing content[].text block".to_owned(),
            ))
        })?;

    if is_error {
        return Err(SdkError::Tool(ToolError {
            tool: "corsoTools".to_owned(),
            message: text,
        }));
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content_block(text: &str, is_error: bool) -> Value {
        serde_json::json!({
            "content": [{ "type": "text", "text": text }],
            "isError": is_error
        })
    }

    #[test]
    fn unwrap_json_parses_object() {
        let input = content_block(r#"{"key":"value"}"#, false);
        let result = unwrap_json(input).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn unwrap_json_returns_tool_error_on_is_error() {
        let input = content_block("file not found", true);
        let err = unwrap_json(input).unwrap_err();
        assert!(matches!(err, SdkError::Tool(_)));
    }

    #[test]
    fn unwrap_json_fails_on_non_json_text() {
        let input = content_block("plain text, not JSON", false);
        let err = unwrap_json(input).unwrap_err();
        assert!(matches!(err, SdkError::Protocol(_)));
    }

    #[test]
    fn unwrap_text_returns_raw_string() {
        let input = content_block("analysis output here", false);
        let text = unwrap_text(input).unwrap();
        assert_eq!(text, "analysis output here");
    }

    #[test]
    fn unwrap_text_returns_tool_error() {
        let input = content_block("operation failed", true);
        let err = unwrap_text(input).unwrap_err();
        assert!(matches!(err, SdkError::Tool(_)));
    }

    #[test]
    fn extract_text_missing_content_field() {
        let input = serde_json::json!({ "isError": false });
        let err = unwrap_text(input).unwrap_err();
        assert!(matches!(err, SdkError::Protocol(_)));
    }
}
