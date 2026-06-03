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
/// Empty lines and `result` / `system` / `debug` events are skipped (`Ok(None)`).
/// Delegates to [`parse_ndjson_value`] after the initial JSON parse so callers
/// that already hold a [`Value`] can avoid a second deserialisation.
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
    let val: Value = serde_json::from_str(line)?;
    parse_ndjson_value(&val)
}

/// Parse a pre-deserialised NDJSON [`Value`] into a [`ProviderEvent`].
///
/// Use this when the caller already holds the parsed `Value` (e.g. to extract
/// other fields before routing) to avoid parsing the JSON a second time.
///
/// `result` / `system` / `debug` events are skipped (`Ok(None)`).
///
/// # Errors
///
/// Returns [`NdjsonParseError::UnknownEventType`] for unrecognised `type` fields
/// that are not on the skip-list.
///
/// ## Note
///
/// This function returns at most one event per line. The current Claude CLI
/// also emits `assistant` envelopes that contain a full message with multiple
/// content blocks. For those, prefer [`parse_ndjson_line_multi`] /
/// [`parse_ndjson_value_multi`] which fan out into the equivalent streaming
/// event sequence.
pub fn parse_ndjson_value(val: &Value) -> Result<Option<ProviderEvent>, NdjsonParseError> {
    let envelope = NdjsonEnvelope {
        event_type: val["type"].as_str().unwrap_or("").to_owned(),
        payload: val.clone(),
    };
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
        // result / system / debug / rate_limit_event lines from claude --verbose
        // — skip silently. `assistant` events carry full messages and are
        // fanned out by `parse_ndjson_value_multi`; this single-event variant
        // skips them rather than erroring.
        "result" | "system" | "debug" | "rate_limit_event" | "assistant" | "user" => {
            return Ok(None);
        }
        other => return Err(NdjsonParseError::UnknownEventType(other.to_owned())),
    };
    Ok(Some(event))
}

/// Parse a Claude CLI NDJSON line into one or more [`ProviderEvent`]s.
///
/// The current Claude CLI (`--output-format stream-json --verbose`) emits
/// `assistant` envelopes containing a complete assistant message with a
/// `content[]` array of `text` / `tool_use` blocks. This function fans those
/// envelopes out into the equivalent SSE-style event sequence that downstream
/// consumers (e.g. `LlmReActExecutor`) already understand:
///
/// ```text
/// assistant{ content:[text "hi"] }
///   →  MessageStart, ContentBlockStart{text}, TextDelta{"hi"},
///      ContentBlockStop, MessageDelta{stop_reason}*
/// ```
///
/// `result` envelopes (the final summary line) emit `MessageDelta` +
/// `MessageStop`. Legacy `message_*` / `content_block_*` lines pass through to
/// the single-event parser unchanged.
///
/// # Errors
///
/// Returns [`NdjsonParseError`] on invalid JSON. Unknown `type` fields fall
/// through to the single-event parser, which may surface `UnknownEventType`.
pub fn parse_ndjson_line_multi(line: &str) -> Result<Vec<ProviderEvent>, NdjsonParseError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let val: Value = serde_json::from_str(trimmed)?;
    parse_ndjson_value_multi(&val)
}

/// Multi-event variant of [`parse_ndjson_value`]. See [`parse_ndjson_line_multi`].
///
/// # Errors
///
/// Returns [`NdjsonParseError::UnknownEventType`] only for envelopes that are
/// not recognised by EITHER the multi-event or single-event parser.
pub fn parse_ndjson_value_multi(val: &Value) -> Result<Vec<ProviderEvent>, NdjsonParseError> {
    let event_type = val["type"].as_str().unwrap_or("");
    match event_type {
        "assistant" => Ok(fan_out_assistant_message(&val["message"])),
        "result" => Ok(fan_out_result(val)),
        "rate_limit_event" | "system" | "debug" | "user" => Ok(Vec::new()),
        _ => match parse_ndjson_value(val)? {
            Some(ev) => Ok(vec![ev]),
            None => Ok(Vec::new()),
        },
    }
}

fn fan_out_assistant_message(msg: &Value) -> Vec<ProviderEvent> {
    let model = msg["model"].as_str().unwrap_or("claude-cli").to_owned();
    let input_tokens = msg["usage"]["input_tokens"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(0);

    let mut events = vec![ProviderEvent::MessageStart {
        model,
        input_tokens,
    }];

    if let Some(content) = msg["content"].as_array() {
        for (i, block) in content.iter().enumerate() {
            let index = u32::try_from(i).unwrap_or(0);
            let block_type = block["type"].as_str().unwrap_or("text").to_owned();
            let tool_use_id = block["id"].as_str().map(str::to_owned);
            let tool_name = block["name"].as_str().map(str::to_owned);

            events.push(ProviderEvent::ContentBlockStart {
                index,
                block_type: block_type.clone(),
                tool_use_id,
                tool_name,
            });

            match block_type.as_str() {
                "text" => {
                    let text = block["text"].as_str().unwrap_or("").to_owned();
                    if !text.is_empty() {
                        events.push(ProviderEvent::TextDelta { index, text });
                    }
                }
                "tool_use" => {
                    if let Some(input) = block.get("input") {
                        if !input.is_null() {
                            events.push(ProviderEvent::InputJsonDelta {
                                index,
                                partial_json: input.to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }

            events.push(ProviderEvent::ContentBlockStop { index });
        }
    }

    // Some `assistant` envelopes carry an intermediate stop_reason; emit it
    // so downstream consumers can detect normal termination.
    if let Some(stop_reason) = msg["stop_reason"].as_str() {
        let output_tokens = msg["usage"]["output_tokens"]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0);
        events.push(ProviderEvent::MessageDelta {
            stop_reason: stop_reason.to_owned(),
            output_tokens,
        });
    }

    events
}

fn fan_out_result(val: &Value) -> Vec<ProviderEvent> {
    let stop_reason = val["stop_reason"].as_str().unwrap_or("end_turn").to_owned();
    let output_tokens = val["usage"]["output_tokens"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(0);
    vec![
        ProviderEvent::MessageDelta {
            stop_reason,
            output_tokens,
        },
        ProviderEvent::MessageStop,
    ]
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

    #[test]
    fn multi_fans_out_assistant_text_message() {
        let line = r#"{"type":"assistant","message":{"model":"claude-sonnet-4-6","content":[{"type":"text","text":"Hi! What are we building today?"}],"stop_reason":null,"usage":{"input_tokens":3,"output_tokens":1}}}"#;
        let events = parse_ndjson_line_multi(line).unwrap();
        // MessageStart + ContentBlockStart + TextDelta + ContentBlockStop
        assert_eq!(events.len(), 4, "got {events:?}");
        assert!(
            matches!(events[0], ProviderEvent::MessageStart { ref model, input_tokens: 3 } if model == "claude-sonnet-4-6")
        );
        assert!(
            matches!(events[1], ProviderEvent::ContentBlockStart { ref block_type, .. } if block_type == "text")
        );
        assert!(
            matches!(events[2], ProviderEvent::TextDelta { ref text, .. } if text.contains("Hi"))
        );
        assert!(matches!(events[3], ProviderEvent::ContentBlockStop { .. }));
    }

    #[test]
    fn multi_fans_out_assistant_tool_use_message() {
        let line = r#"{"type":"assistant","message":{"model":"claude-sonnet-4-6","content":[{"type":"tool_use","id":"toolu_01","name":"Read","input":{"file_path":"/tmp/x"}}],"stop_reason":"tool_use","usage":{"input_tokens":5,"output_tokens":7}}}"#;
        let events = parse_ndjson_line_multi(line).unwrap();
        // MessageStart + ContentBlockStart + InputJsonDelta + ContentBlockStop + MessageDelta
        assert_eq!(events.len(), 5, "got {events:?}");
        assert!(matches!(events[0], ProviderEvent::MessageStart { .. }));
        assert!(matches!(
            &events[1],
            ProviderEvent::ContentBlockStart {
                block_type,
                tool_use_id: Some(id),
                tool_name: Some(name),
                ..
            } if block_type == "tool_use" && id == "toolu_01" && name == "Read"
        ));
        assert!(matches!(
            &events[2],
            ProviderEvent::InputJsonDelta { partial_json, .. } if partial_json.contains("/tmp/x")
        ));
        assert!(matches!(events[3], ProviderEvent::ContentBlockStop { .. }));
        assert!(matches!(
            &events[4],
            ProviderEvent::MessageDelta { stop_reason, output_tokens: 7 } if stop_reason == "tool_use"
        ));
    }

    #[test]
    fn multi_skips_rate_limit_event() {
        let line = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}"#;
        let events = parse_ndjson_line_multi(line).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn multi_emits_message_stop_for_result_envelope() {
        let line = r#"{"type":"result","subtype":"success","stop_reason":"end_turn","usage":{"output_tokens":11},"result":"Hi"}"#;
        let events = parse_ndjson_line_multi(line).unwrap();
        assert_eq!(events.len(), 2, "got {events:?}");
        assert!(matches!(
            &events[0],
            ProviderEvent::MessageDelta { stop_reason, output_tokens: 11 } if stop_reason == "end_turn"
        ));
        assert!(matches!(events[1], ProviderEvent::MessageStop));
    }

    #[test]
    fn multi_passes_through_legacy_message_start() {
        let line = r#"{"type":"message_start","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":10}}}"#;
        let events = parse_ndjson_line_multi(line).unwrap();
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], ProviderEvent::MessageStart { model, input_tokens: 10 } if model == "claude-sonnet-4-6")
        );
    }
}
