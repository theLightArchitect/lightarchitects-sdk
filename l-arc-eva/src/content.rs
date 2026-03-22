//! MCP content-block unwrapping for EVA responses.
//!
//! EVA exposes 8 individual tools (not a single orchestrator). Each tool wraps
//! its response in the standard MCP `ToolCallResult` envelope:
//!
//! ```json
//! {
//!   "content": [{ "type": "text", "text": "<JSON string>" }],
//!   "isError": false
//! }
//! ```
//!
//! The `text` field always contains `serde_json::to_string_pretty(&result)` —
//! the entire serialised result struct. For the `visualize` tool, `image_base64`
//! is embedded as a field inside that JSON rather than appearing as a separate
//! `ContentBlock::Image` block.
//!
//! [`unwrap_json`] extracts `content[].text` and parses it as JSON — used by all
//! 8 EVA tools.
//!
//! [`unwrap_text`] extracts `content[].text` as a raw string — used by the
//! generic [`crate::EvaClient::action`] adapter which returns [`crate::ActionOutput`].
//!
//! [`extract_image`] scans content blocks for an `Image` block — forward
//! compatibility only; EVA currently embeds images inside the JSON text block.

use serde_json::Value;

use l_arc_core::error::{ProtocolError, SdkError, ToolError};

/// Extract and JSON-parse the first content-block text from a `ToolCallResult`.
///
/// `tool` is the EVA tool name (e.g. `"teach"`, `"visualize"`) and is included
/// in any error messages to aid debugging.
///
/// # Errors
///
/// Returns [`SdkError::Tool`] when `isError` is `true`.
/// Returns [`SdkError::Protocol`] when the envelope is malformed or `text`
/// is not valid JSON.
pub fn unwrap_json(value: Value, tool: &str) -> Result<Value, SdkError> {
    let text = extract_text(value, tool)?;
    serde_json::from_str(&text).map_err(|e| {
        SdkError::Protocol(ProtocolError::MalformedJson(format!(
            "EVA `{tool}` content block text is not valid JSON: {e}"
        )))
    })
}

/// Extract the first content-block text from a `ToolCallResult` as a `String`.
///
/// `tool` is the EVA tool name included in any error messages.
///
/// # Errors
///
/// Returns [`SdkError::Tool`] when `isError` is `true`.
/// Returns [`SdkError::Protocol`] when the envelope is malformed.
pub fn unwrap_text(value: Value, tool: &str) -> Result<String, SdkError> {
    extract_text(value, tool)
}

/// Scan content blocks for an `Image` block and return its base64 data.
///
/// EVA currently embeds `image_base64` inside the JSON text block rather than
/// using a dedicated image content block, so this function will typically return
/// `None`. It is retained for forward compatibility in case EVA's wire format
/// evolves to use image blocks directly.
#[must_use]
pub fn extract_image(value: &Value) -> Option<String> {
    value
        .get("content")
        .and_then(Value::as_array)?
        .iter()
        .find(|b| b.get("type").and_then(Value::as_str) == Some("image"))
        .and_then(|b| b.get("data"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Shared extraction logic: validates the envelope and returns `content[].text`.
fn extract_text(value: Value, tool: &str) -> Result<String, SdkError> {
    let is_error = value
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // Find the first content block with `"type": "text"` rather than blindly
    // indexing [0] — EVA may prepend progress or image blocks.
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
                "EVA `{tool}` response missing content[].text block"
            )))
        })?;

    if is_error {
        return Err(SdkError::Tool(ToolError {
            tool: tool.to_owned(),
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

    fn image_block(data: &str) -> Value {
        serde_json::json!({
            "content": [{ "type": "image", "data": data, "mimeType": "image/png" }],
            "isError": false
        })
    }

    #[test]
    fn unwrap_json_parses_object() {
        let input = content_block(r#"{"response":"hello"}"#, false);
        let result = unwrap_json(input, "teach").unwrap();
        assert_eq!(result["response"], "hello");
    }

    #[test]
    fn unwrap_json_returns_tool_error_on_is_error() {
        let input = content_block("quota exceeded", true);
        let err = unwrap_json(input, "ideate").unwrap_err();
        assert!(matches!(err, SdkError::Tool(ref e) if e.tool == "ideate"));
    }

    #[test]
    fn unwrap_json_fails_on_non_json_text() {
        let input = content_block("plain prose, not JSON", false);
        let err = unwrap_json(input, "research").unwrap_err();
        assert!(matches!(err, SdkError::Protocol(_)));
    }

    #[test]
    fn unwrap_text_returns_raw_string() {
        let input = content_block("some output here", false);
        let text = unwrap_text(input, "build").unwrap();
        assert_eq!(text, "some output here");
    }

    #[test]
    fn unwrap_text_returns_tool_error_with_tool_name() {
        let input = content_block("operation failed", true);
        let err = unwrap_text(input, "secure").unwrap_err();
        match err {
            SdkError::Tool(e) => assert_eq!(e.tool, "secure"),
            other => panic!("expected Tool error, got {other:?}"),
        }
    }

    #[test]
    fn extract_text_missing_content_field() {
        let input = serde_json::json!({ "isError": false });
        let err = unwrap_text(input, "bible").unwrap_err();
        assert!(matches!(err, SdkError::Protocol(_)));
    }

    #[test]
    fn extract_image_finds_image_block() {
        let input = image_block("base64data==");
        let data = extract_image(&input);
        assert_eq!(data.as_deref(), Some("base64data=="));
    }

    #[test]
    fn extract_image_returns_none_for_text_only() {
        let input = content_block(r#"{"image_base64":"abc"}"#, false);
        // Image data is in the JSON text, not in an image block.
        let data = extract_image(&input);
        assert!(data.is_none());
    }
}
