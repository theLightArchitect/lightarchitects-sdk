//! MCP content-block unwrapping for LÆX responses.
//!
//! LÆX is dispatched **inline** within the lightarchitects-gateway via the
//! `laexTools` virtual orchestrator. Responses follow the standard MCP
//! `ToolCallResult` envelope:
//!
//! ```json
//! {
//!   "content": [{ "type": "text", "text": "<JSON string>" }],
//!   "isError": false
//! }
//! ```
//!
//! [`unwrap_json`] extracts `content[].text` and parses it as JSON — used by
//! all 9 LÆX typed methods.
//!
//! [`unwrap_text`] extracts `content[].text` as a raw string — used by the
//! generic [`crate::laex::LaexClient::action`] adapter.

use serde_json::Value;

use crate::core::error::{ProtocolError, SdkError, ToolError};

/// Extract and JSON-parse the first content-block text from a `ToolCallResult`.
///
/// `action` is the LÆX action name (e.g. `"canon_check"`) and is included in
/// any error messages to aid debugging.
///
/// # Errors
///
/// Returns [`SdkError::Tool`] when `isError` is `true`.
/// Returns [`SdkError::Protocol`] when the envelope is malformed or `text`
/// is not valid JSON.
pub fn unwrap_json(value: Value, action: &str) -> Result<Value, SdkError> {
    let text = extract_text(value, action)?;
    serde_json::from_str(&text).map_err(|e| {
        SdkError::Protocol(ProtocolError::MalformedJson(format!(
            "LÆX `{action}` content block text is not valid JSON: {e}"
        )))
    })
}

/// Extract the first content-block text from a `ToolCallResult` as a `String`.
///
/// `action` is the LÆX action name included in any error messages.
///
/// # Errors
///
/// Returns [`SdkError::Tool`] when `isError` is `true`.
/// Returns [`SdkError::Protocol`] when the envelope is malformed.
pub fn unwrap_text(value: Value, action: &str) -> Result<String, SdkError> {
    extract_text(value, action)
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
                "LÆX `{action}` response missing content[].text block"
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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
        let input = content_block(r#"{"framework":"<canonical-context>"}"#, false);
        let result = unwrap_json(input, "canon_check").unwrap();
        assert_eq!(result["framework"], "<canonical-context>");
    }

    #[test]
    fn unwrap_json_returns_tool_error_on_is_error() {
        let input = content_block("registry not found", true);
        let err = unwrap_json(input, "canon_check").unwrap_err();
        assert!(matches!(err, SdkError::Tool(ref e) if e.tool == "canon_check"));
    }

    #[test]
    fn unwrap_text_returns_raw_string() {
        let input = content_block("retro summary text", false);
        let text = unwrap_text(input, "reflect").unwrap();
        assert_eq!(text, "retro summary text");
    }

    #[test]
    fn extract_text_missing_content_field_is_protocol_error() {
        let input = serde_json::json!({ "isError": false });
        let err = unwrap_text(input, "matrix_ratify").unwrap_err();
        assert!(matches!(err, SdkError::Protocol(_)));
    }
}
