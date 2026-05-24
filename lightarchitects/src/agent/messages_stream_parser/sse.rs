//! Anthropic SSE stream parser — converts `data: {json}` lines into [`ProviderEvent`]s.
//!
//! Also handles Ollama `/v1/messages` (Anthropic-compat SSE envelope — same wire shape).

use serde::Deserialize;
use serde_json::Value;

use crate::agent::ProviderEvent;

/// Error produced while parsing an SSE line.
#[derive(Debug, thiserror::Error)]
pub enum SseParseError {
    /// The line had no `data: ` prefix and was not empty or a comment.
    #[error("malformed SSE line (missing 'data: ' prefix): {0:?}")]
    MalformedLine(String),
    /// The `data:` payload was not valid JSON.
    #[error("failed to deserialise SSE event JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The event `type` field was not on the known-types list.
    #[error("unknown event type: {0:?}")]
    UnknownEventType(String),
}

// ── wire shapes ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SseEnvelope {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(flatten)]
    payload: Value,
}

// ── parser ───────────────────────────────────────────────────────────────────

/// Parses a single SSE `data:` line into a [`ProviderEvent`].
///
/// Lines that are empty or that begin with `event:` / `:` (comment) are
/// silently skipped (returns `Ok(None)`).
///
/// # Errors
///
/// Returns [`SseParseError`] when the line has a `data: ` prefix but the JSON
/// payload is malformed or contains an unrecognised `type` field.
pub fn parse_sse_line(line: &str) -> Result<Option<ProviderEvent>, SseParseError> {
    let line = line.trim();
    if line.is_empty() || line.starts_with("event:") || line.starts_with(':') {
        return Ok(None);
    }
    let json_str = line
        .strip_prefix("data: ")
        .ok_or_else(|| SseParseError::MalformedLine(line.to_owned()))?;

    if json_str == "[DONE]" {
        return Ok(Some(ProviderEvent::MessageStop));
    }

    let envelope: SseEnvelope = serde_json::from_str(json_str)?;
    let event = match envelope.event_type.as_str() {
        "message_start" => {
            let model = envelope.payload["message"]["model"]
                .as_str()
                .unwrap_or("unknown")
                .to_owned();
            let input_tokens = envelope.payload["message"]["usage"]["input_tokens"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0);
            ProviderEvent::MessageStart {
                model,
                input_tokens,
            }
        }
        "content_block_start" => {
            let index = envelope.payload["index"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0);
            let block = &envelope.payload["content_block"];
            let block_type = block["type"].as_str().unwrap_or("text").to_owned();
            let tool_use_id = block["id"].as_str().map(str::to_owned);
            let tool_name = block["name"].as_str().map(str::to_owned);
            ProviderEvent::ContentBlockStart {
                index,
                block_type,
                tool_use_id,
                tool_name,
            }
        }
        "content_block_delta" => {
            let index = envelope.payload["index"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0);
            let delta = &envelope.payload["delta"];
            match delta["type"].as_str().unwrap_or("") {
                "text_delta" => {
                    let text = delta["text"].as_str().unwrap_or("").to_owned();
                    ProviderEvent::TextDelta { index, text }
                }
                "input_json_delta" => {
                    let partial_json = delta["partial_json"].as_str().unwrap_or("").to_owned();
                    ProviderEvent::InputJsonDelta {
                        index,
                        partial_json,
                    }
                }
                other => return Err(SseParseError::UnknownEventType(other.to_owned())),
            }
        }
        "content_block_stop" => {
            let index = envelope.payload["index"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0);
            ProviderEvent::ContentBlockStop { index }
        }
        "message_delta" => {
            let stop_reason = envelope.payload["delta"]["stop_reason"]
                .as_str()
                .unwrap_or("end_turn")
                .to_owned();
            let output_tokens = envelope.payload["usage"]["output_tokens"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0);
            ProviderEvent::MessageDelta {
                stop_reason,
                output_tokens,
            }
        }
        "message_stop" => ProviderEvent::MessageStop,
        // ping / error events from the Anthropic API — skip silently
        "ping" | "error" => return Ok(None),
        other => return Err(SseParseError::UnknownEventType(other.to_owned())),
    };
    Ok(Some(event))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parses_message_start() {
        let line = r#"data: {"type":"message_start","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":42}}}"#;
        let ev = parse_sse_line(line).unwrap().unwrap();
        assert!(matches!(
            ev,
            ProviderEvent::MessageStart { model, input_tokens: 42 }
            if model == "claude-sonnet-4-6"
        ));
    }

    #[test]
    fn parses_text_delta() {
        let line = r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let ev = parse_sse_line(line).unwrap().unwrap();
        assert!(matches!(ev, ProviderEvent::TextDelta { index: 0, text } if text == "Hello"));
    }

    #[test]
    fn parses_input_json_delta() {
        let line = r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"k\""}}"#;
        let ev = parse_sse_line(line).unwrap().unwrap();
        assert!(matches!(ev, ProviderEvent::InputJsonDelta { index: 1, .. }));
    }

    #[test]
    fn parses_message_stop() {
        let ev = parse_sse_line(r#"data: {"type":"message_stop"}"#)
            .unwrap()
            .unwrap();
        assert!(matches!(ev, ProviderEvent::MessageStop));
    }

    #[test]
    fn done_sentinel_is_message_stop() {
        let ev = parse_sse_line("data: [DONE]").unwrap().unwrap();
        assert!(matches!(ev, ProviderEvent::MessageStop));
    }

    #[test]
    fn skips_empty_lines() {
        assert!(parse_sse_line("").unwrap().is_none());
        assert!(parse_sse_line("event: message_start").unwrap().is_none());
        assert!(parse_sse_line(": comment").unwrap().is_none());
    }

    #[test]
    fn skips_ping() {
        let line = r#"data: {"type":"ping"}"#;
        assert!(parse_sse_line(line).unwrap().is_none());
    }

    #[test]
    fn rejects_missing_data_prefix() {
        let err = parse_sse_line("garbage line without prefix").unwrap_err();
        assert!(matches!(err, SseParseError::MalformedLine(_)));
    }
}
