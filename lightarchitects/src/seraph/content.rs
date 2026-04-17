//! MCP content-block unwrapping for SERAPH responses.
//!
//! SERAPH uses `Content-Length` header framing (not newline-delimited JSON),
//! but the `ToolCallResult` envelope format is identical to all siblings:
//!
//! ```json
//! {
//!   "content": [{ "type": "text", "text": "<AI-generated prose>" }],
//!   "isError": false
//! }
//! ```
//!
//! All 18 SERAPH actions return AI-generated pentest/investigation prose.
//! [`unwrap_text`] is the only extraction function needed.

use serde_json::Value;

use crate::core::error::{ProtocolError, SdkError, ToolError};

/// Extract the first content-block text from a `ToolCallResult` as a `String`.
///
/// # Errors
///
/// Returns [`SdkError::Tool`] when `isError` is `true`.
/// Returns [`SdkError::Protocol`] when the envelope is malformed.
pub fn unwrap_text(value: Value) -> Result<String, SdkError> {
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
            SdkError::Protocol(ProtocolError::UnexpectedShape(
                "SERAPH response missing content[].text block".to_owned(),
            ))
        })?;

    if is_error {
        return Err(SdkError::Tool(ToolError {
            tool: "penTools".to_owned(),
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
    fn unwrap_text_returns_raw_string() {
        let input = content_block("Recon complete — 12 hosts discovered.", false);
        let text = unwrap_text(input).unwrap();
        assert_eq!(text, "Recon complete — 12 hosts discovered.");
    }

    #[test]
    fn unwrap_text_returns_tool_error_on_is_error() {
        let input = content_block("target out of scope", true);
        let err = unwrap_text(input).unwrap_err();
        assert!(matches!(err, SdkError::Tool(_)));
    }

    #[test]
    fn unwrap_text_fails_on_missing_content() {
        let input = serde_json::json!({ "isError": false });
        let err = unwrap_text(input).unwrap_err();
        assert!(matches!(err, SdkError::Protocol(_)));
    }

    #[test]
    fn unwrap_text_skips_non_text_blocks() {
        let input = serde_json::json!({
            "content": [
                { "type": "progress", "text": "scanning..." },
                { "type": "text", "text": "Scan complete." }
            ],
            "isError": false
        });
        let text = unwrap_text(input).unwrap();
        assert_eq!(text, "Scan complete.");
    }
}
