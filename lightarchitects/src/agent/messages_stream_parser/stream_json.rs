//! Claude CLI NDJSON stream parser — converts newline-delimited JSON from
//! `claude --output-format stream-json --verbose` stdout into [`ProviderEvent`]s.
//!
//! This module is the refactor target for `copilot/mod.rs` lines 1–1806 (TS-3 §21.3).
//! It implements the same wire-shape handling in a shared SDK location so both the
//! gateway and the webshell copilot path use a single parser.

use serde::Deserialize;
use serde_json::Value;

use crate::agent::ProviderEvent;

/// Error produced while parsing a Claude CLI NDJSON line.
#[derive(Debug, thiserror::Error)]
pub enum NdjsonParseError {
    /// The line was not valid JSON.
    #[error("failed to deserialise NDJSON line: {0}")]
    Json(#[from] serde_json::Error),
    /// The event `type` field was not on the known-types list.
    #[error("unknown Claude CLI event type: {0:?}")]
    UnknownEventType(String),
}

// ── wire shapes ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct NdjsonEnvelope {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(flatten)]
    payload: Value,
}

// ── parser ───────────────────────────────────────────────────────────────────

/// Parse a single NDJSON line emitted by `claude --output-format stream-json`.
///
/// Empty lines and `system` / `debug` events are skipped (`Ok(None)`).
///
/// # Errors
///
/// Returns [`NdjsonParseError`] if the line is not valid JSON or carries an
/// unrecognised `type` field that is not on the skip-list.
pub fn parse_ndjson_line(line: &str) -> Result<Option<ProviderEvent>, NdjsonParseError> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    let envelope: NdjsonEnvelope = serde_json::from_str(line)?;
    let event = match envelope.event_type.as_str() {
        "message_start" => {
            let model = envelope.payload["message"]["model"]
                .as_str()
                .unwrap_or("claude-cli")
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
                other => return Err(NdjsonParseError::UnknownEventType(other.to_owned())),
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
        // result / system / debug lines from claude --verbose — skip silently
        "result" | "system" | "debug" => return Ok(None),
        other => return Err(NdjsonParseError::UnknownEventType(other.to_owned())),
    };
    Ok(Some(event))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parses_message_start() {
        let line = r#"{"type":"message_start","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":10}}}"#;
        let ev = parse_ndjson_line(line).unwrap().unwrap();
        assert!(matches!(
            ev,
            ProviderEvent::MessageStart { model, input_tokens: 10 }
            if model == "claude-sonnet-4-6"
        ));
    }

    #[test]
    fn parses_text_delta() {
        let line =
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#;
        let ev = parse_ndjson_line(line).unwrap().unwrap();
        assert!(matches!(ev, ProviderEvent::TextDelta { text, .. } if text == "hi"));
    }

    #[test]
    fn parses_tool_use_start() {
        let line = r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tu_01","name":"read_file"}}"#;
        let ev = parse_ndjson_line(line).unwrap().unwrap();
        assert!(matches!(
            ev,
            ProviderEvent::ContentBlockStart { block_type, tool_name: Some(name), .. }
            if block_type == "tool_use" && name == "read_file"
        ));
    }

    #[test]
    fn skips_result_line() {
        let line = r#"{"type":"result","subtype":"success","result":"done"}"#;
        assert!(parse_ndjson_line(line).unwrap().is_none());
    }

    #[test]
    fn skips_empty_line() {
        assert!(parse_ndjson_line("").unwrap().is_none());
        assert!(parse_ndjson_line("   ").unwrap().is_none());
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_ndjson_line("not json").is_err());
    }

    #[test]
    fn parses_message_stop() {
        let line = r#"{"type":"message_stop"}"#;
        let ev = parse_ndjson_line(line).unwrap().unwrap();
        assert!(matches!(ev, ProviderEvent::MessageStop));
    }
}
