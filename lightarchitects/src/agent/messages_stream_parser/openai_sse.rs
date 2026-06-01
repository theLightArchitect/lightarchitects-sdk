//! OpenAI-compatible SSE chunk deserializer.
//!
//! Parses `data: {json}` lines from an `OpenAI` `/chat/completions` SSE stream
//! into [`OpenAiChunk`] structs. Callers accumulate state (model name, tool-call
//! block indices, token counts) and convert chunks into [`ProviderEvent`]s using
//! [`OpenAiStreamState`].
//!
//! # Wire format
//!
//! Each SSE line is `data: <json>` where the JSON has the shape:
//!
//! ```json
//! {
//!   "id": "chatcmpl-…",
//!   "object": "chat.completion.chunk",
//!   "model": "Qwen/Qwen2.5-Coder-32B-Instruct",
//!   "choices": [{
//!     "index": 0,
//!     "delta": { "content": "Hello" },
//!     "finish_reason": null
//!   }],
//!   "usage": null
//! }
//! ```
//!
//! The stream terminates with `data: [DONE]`.
//!
//! # Token-count delivery
//!
//! Some providers (vLLM ≥ 0.4 with `stream_options.include_usage: true`) send
//! the `usage` object on a **separate** final chunk that arrives *after* the
//! `finish_reason: "stop"` chunk.  [`OpenAiStreamState`] buffers both the stop
//! reason and the completion token count, emitting [`ProviderEvent::MessageDelta`]
//! only in [`OpenAiStreamState::finalize`] once the stream is fully drained.

use std::collections::HashMap;

use serde::Deserialize;

use crate::agent::ProviderEvent;

// ── Wire shapes (serde) ───────────────────────────────────────────────────────

/// One SSE chunk from an `OpenAI` `/chat/completions` stream.
#[derive(Debug, Deserialize)]
pub struct OpenAiChunk {
    /// Model name; present on every chunk, captured from the first.
    pub model: Option<String>,
    /// One entry per completion candidate (always one in non-batch mode).
    pub choices: Vec<OpenAiChoice>,
    /// Token counts; only present on the final `[DONE]`-preceding chunk
    /// when `stream_options.include_usage = true`.
    pub usage: Option<OpenAiUsage>,
}

/// One choice slot within a chunk.
#[derive(Debug, Deserialize)]
pub struct OpenAiChoice {
    /// Incremental content or tool-call fragment.
    pub delta: OpenAiDelta,
    /// Set on the final chunk: `"stop"`, `"tool_calls"`, `"length"`, etc.
    pub finish_reason: Option<String>,
}

/// Incremental delta within a choice.
#[derive(Debug, Deserialize, Default)]
pub struct OpenAiDelta {
    /// Text fragment; mutually exclusive with `tool_calls`.
    pub content: Option<String>,
    /// `DeepSeek` R1 chain-of-thought fragment. `LiteLLM` passes this through
    /// as-is from the native `DeepSeek` API. Emitted as `<think>…</think>`
    /// so the [`crate::agent::ThinkSplitter`] in downstream consumers can
    /// classify it without additional state.
    #[serde(default)]
    pub reasoning_content: Option<String>,
    /// Tool-call fragments; accumulate across multiple chunks.
    #[serde(default)]
    pub tool_calls: Vec<OpenAiToolCallDelta>,
}

/// One tool-call slot within a delta.
#[derive(Debug, Deserialize)]
pub struct OpenAiToolCallDelta {
    /// `OpenAI`'s tool-call slot index (mapped to our block indices via `tool_block_map`).
    pub index: u32,
    /// Call ID; present only on the first delta for this slot.
    pub id: Option<String>,
    /// Function name and argument fragment.
    pub function: Option<OpenAiFunctionDelta>,
}

/// Incremental function-call fragment.
#[derive(Debug, Deserialize)]
pub struct OpenAiFunctionDelta {
    /// Function name; present only on the first delta for this call.
    pub name: Option<String>,
    /// JSON argument fragment; accumulates across multiple deltas.
    pub arguments: Option<String>,
}

/// Token-usage summary from the final stream chunk.
#[derive(Debug, Deserialize)]
pub struct OpenAiUsage {
    /// Input (prompt) token count.
    pub prompt_tokens: u32,
    /// Output (completion) token count.
    pub completion_tokens: u32,
}

// ── Parse error ───────────────────────────────────────────────────────────────

/// Error returned when an `OpenAI` SSE line cannot be parsed.
#[derive(Debug, thiserror::Error)]
pub enum OpenAiSseParseError {
    /// Missing `data: ` prefix on a non-empty, non-comment line.
    #[error("malformed SSE line (missing 'data: ' prefix): {0:?}")]
    MalformedLine(String),
    /// The JSON payload was invalid.
    #[error("JSON deserialise error: {0}")]
    Json(#[from] serde_json::Error),
}

// ── Line parser ───────────────────────────────────────────────────────────────

/// Parses one SSE `data:` line into a raw [`OpenAiChunk`].
///
/// Returns `Ok(None)` for empty lines, comment lines, and `data: [DONE]`.
/// Callers should call [`OpenAiStreamState::apply`] to convert chunks to events.
///
/// # Errors
///
/// Returns [`OpenAiSseParseError`] for malformed or invalid JSON lines.
pub fn parse_openai_sse_line(line: &str) -> Result<Option<OpenAiChunk>, OpenAiSseParseError> {
    let line = line.trim();
    if line.is_empty() || line.starts_with("event:") || line.starts_with(':') {
        return Ok(None);
    }
    let json_str = line
        .strip_prefix("data: ")
        .ok_or_else(|| OpenAiSseParseError::MalformedLine(line.to_owned()))?;

    if json_str == "[DONE]" {
        return Ok(None); // caller emits MessageStop via stream end
    }

    Ok(Some(serde_json::from_str(json_str)?))
}

// ── Stateful mapper ───────────────────────────────────────────────────────────

/// Stateful converter from [`OpenAiChunk`]s to [`ProviderEvent`]s.
///
/// Maintains cross-chunk state:
/// - Whether the opening [`ProviderEvent::MessageStart`] has been emitted.
/// - The model name (carried on every chunk; captured from the first).
/// - A map from `OpenAI` tool-call stream index → our block index.
/// - Buffered prompt/completion token counts (may arrive on a separate final
///   chunk after `finish_reason`, per vLLM ≥ 0.4 behaviour).
/// - A buffered stop reason — [`ProviderEvent::MessageDelta`] is emitted in
///   [`finalize`] so the completion token count is available.
///
/// [`finalize`]: OpenAiStreamState::finalize
pub struct OpenAiStreamState {
    first_seen: bool,
    model: String,
    /// Next available block index (0 = text block).
    next_block_index: u32,
    /// Maps `OpenAI` `tool_call.index` → our `ContentBlockStart.index`.
    tool_block_map: HashMap<u32, u32>,
    input_tokens: u32,
    /// Buffered from any chunk carrying `usage.completion_tokens`.
    completion_tokens: u32,
    /// Buffered `finish_reason` — kept until `finalize()` when token counts
    /// are final.  `None` means no stop signal has been seen yet.
    pending_stop_reason: Option<String>,
}

impl OpenAiStreamState {
    /// Construct fresh state for a new stream.
    pub fn new() -> Self {
        Self {
            first_seen: false,
            model: String::new(),
            next_block_index: 0,
            tool_block_map: HashMap::new(),
            input_tokens: 0,
            completion_tokens: 0,
            pending_stop_reason: None,
        }
    }

    /// Convert a raw [`OpenAiChunk`] into zero or more [`ProviderEvent`]s.
    ///
    /// Events are returned in the order the stream consumer expects:
    /// `MessageStart` (once) → `ContentBlockStart` / `TextDelta` / `InputJsonDelta`
    /// / `ContentBlockStop` (per block) → the caller calls [`finalize`] at
    /// stream end to emit the final [`ProviderEvent::MessageDelta`] with
    /// accurate token counts.
    ///
    /// [`finalize`]: OpenAiStreamState::finalize
    pub fn apply(&mut self, chunk: OpenAiChunk) -> Vec<ProviderEvent> {
        let mut events = Vec::new();

        // Capture model name on first chunk with one.
        if let Some(m) = &chunk.model {
            if !m.is_empty() && self.model.is_empty() {
                self.model.clone_from(m);
            }
        }

        // Buffer token counts from any chunk's usage field.
        // vLLM sends these on a *separate* final chunk after finish_reason.
        if let Some(u) = &chunk.usage {
            if u.prompt_tokens > 0 {
                self.input_tokens = u.prompt_tokens;
            }
            if u.completion_tokens > 0 {
                self.completion_tokens = u.completion_tokens;
            }
        }

        // Emit MessageStart exactly once — on the first chunk with a non-empty model.
        if !self.first_seen && !self.model.is_empty() {
            self.first_seen = true;
            events.push(ProviderEvent::MessageStart {
                model: self.model.clone(),
                input_tokens: self.input_tokens,
            });
            // Emit ContentBlockStart for the text block (index 0).
            events.push(ProviderEvent::ContentBlockStart {
                index: 0,
                block_type: "text".to_owned(),
                tool_use_id: None,
                tool_name: None,
            });
            self.next_block_index = 1;
        }

        for choice in chunk.choices {
            // ── Reasoning content delta (DeepSeek R1 / QwQ) ──────────────────
            // LiteLLM passes `delta.reasoning_content` through from the native
            // DeepSeek API. Wrap in `<think>…</think>` so downstream
            // `ThinkSplitter` consumers classify it as thinking without extra
            // state. Each chunk is self-contained — no cross-chunk tag pairing
            // needed because each wrapping is a complete open+close pair.
            if let Some(reasoning) = choice.delta.reasoning_content {
                if !reasoning.is_empty() {
                    let text = format!("<think>{reasoning}</think>");
                    events.push(ProviderEvent::TextDelta { index: 0, text });
                }
            }

            // ── Text content delta ────────────────────────────────────────────
            if let Some(text) = choice.delta.content {
                if !text.is_empty() {
                    events.push(ProviderEvent::TextDelta { index: 0, text });
                }
            }

            // ── Tool-call deltas ──────────────────────────────────────────────
            for tc in choice.delta.tool_calls {
                let block_index = if let Some(existing) = self.tool_block_map.get(&tc.index) {
                    *existing
                } else {
                    // First delta for this tool-call: emit ContentBlockStart.
                    let new_idx = self.next_block_index;
                    self.tool_block_map.insert(tc.index, new_idx);
                    self.next_block_index += 1;

                    let (tool_use_id, tool_name) = tc
                        .function
                        .as_ref()
                        .map(|f| (tc.id.clone(), f.name.clone()))
                        .unwrap_or_default();

                    events.push(ProviderEvent::ContentBlockStart {
                        index: new_idx,
                        block_type: "tool_use".to_owned(),
                        tool_use_id,
                        tool_name,
                    });
                    new_idx
                };

                // Subsequent deltas append to the arguments JSON.
                if let Some(args) = tc.function.as_ref().and_then(|f| f.arguments.as_deref()) {
                    if !args.is_empty() {
                        events.push(ProviderEvent::InputJsonDelta {
                            index: block_index,
                            partial_json: args.to_owned(),
                        });
                    }
                }
            }

            // ── Stop signal ───────────────────────────────────────────────────
            if let Some(reason) = choice.finish_reason {
                // Close all open tool-call blocks.
                for &block_idx in self.tool_block_map.values() {
                    events.push(ProviderEvent::ContentBlockStop { index: block_idx });
                }
                // Close the text block.
                events.push(ProviderEvent::ContentBlockStop { index: 0 });

                // Normalise finish_reason vocabulary to Anthropic's for callers.
                let stop_reason = match reason.as_str() {
                    "tool_calls" => "tool_use".to_owned(),
                    "length" => "max_tokens".to_owned(),
                    other => other.to_owned(),
                };
                // Buffer the stop reason — MessageDelta is emitted in finalize()
                // once the final usage chunk (which may arrive AFTER this chunk
                // on vLLM endpoints) has updated completion_tokens.
                self.pending_stop_reason = Some(stop_reason);
            }
        }

        events
    }

    /// Flush state at stream end and return any remaining events.
    ///
    /// Emits (in order, as applicable):
    /// 1. [`ProviderEvent::MessageStart`] + [`ProviderEvent::ContentBlockStart`]
    ///    (index 0, text) if the stream ended before a model field was seen.
    /// 2. [`ProviderEvent::MessageDelta`] with the final token counts, if a
    ///    stop reason was buffered by [`apply`].
    ///
    /// The caller is responsible for appending [`ProviderEvent::MessageStop`]
    /// after the events returned here.
    ///
    /// [`apply`]: OpenAiStreamState::apply
    pub fn finalize(&mut self) -> Vec<ProviderEvent> {
        let mut events = Vec::new();

        // Ensure MessageStart + ContentBlockStart(0) have been emitted.
        if !self.first_seen {
            self.first_seen = true;
            events.push(ProviderEvent::MessageStart {
                model: if self.model.is_empty() {
                    "openai-compat".to_owned()
                } else {
                    self.model.clone()
                },
                input_tokens: self.input_tokens,
            });
            events.push(ProviderEvent::ContentBlockStart {
                index: 0,
                block_type: "text".to_owned(),
                tool_use_id: None,
                tool_name: None,
            });
        }

        // Emit buffered MessageDelta with final token counts.
        if let Some(stop_reason) = self.pending_stop_reason.take() {
            events.push(ProviderEvent::MessageDelta {
                stop_reason,
                output_tokens: self.completion_tokens,
            });
        }

        events
    }
}

impl Default for OpenAiStreamState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn skips_empty_and_comment_lines() {
        assert!(parse_openai_sse_line("").unwrap().is_none());
        assert!(parse_openai_sse_line(": ping").unwrap().is_none());
        assert!(parse_openai_sse_line("event: heartbeat").unwrap().is_none());
    }

    #[test]
    fn skips_done_sentinel() {
        assert!(parse_openai_sse_line("data: [DONE]").unwrap().is_none());
    }

    #[test]
    fn rejects_missing_data_prefix() {
        let err = parse_openai_sse_line("garbage line").unwrap_err();
        assert!(matches!(err, OpenAiSseParseError::MalformedLine(_)));
    }

    #[test]
    fn parses_text_delta_chunk() {
        let line = r#"data: {"id":"c1","object":"chat.completion.chunk","model":"qwen","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}],"usage":null}"#;
        let chunk = parse_openai_sse_line(line).unwrap().unwrap();
        assert_eq!(chunk.model.as_deref(), Some("qwen"));
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
        assert!(chunk.choices[0].finish_reason.is_none());
    }

    #[test]
    fn state_emits_message_start_on_first_non_empty_model() {
        let line = r#"data: {"id":"c1","object":"chat.completion.chunk","model":"qwen","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}],"usage":null}"#;
        let chunk = parse_openai_sse_line(line).unwrap().unwrap();
        let mut state = OpenAiStreamState::new();
        let evs = state.apply(chunk);
        assert!(
            evs.iter()
                .any(|e| matches!(e, ProviderEvent::MessageStart { model, .. } if model == "qwen"))
        );
        assert!(
            evs.iter()
                .any(|e| matches!(e, ProviderEvent::TextDelta { text, .. } if text == "Hi"))
        );
    }

    #[test]
    fn state_message_start_emitted_only_once() {
        let line = r#"data: {"id":"c1","object":"chat.completion.chunk","model":"qwen","choices":[{"index":0,"delta":{"content":"a"},"finish_reason":null}],"usage":null}"#;
        let chunk1 = parse_openai_sse_line(line).unwrap().unwrap();
        let chunk2 = parse_openai_sse_line(line).unwrap().unwrap();
        let mut state = OpenAiStreamState::new();
        let evs1 = state.apply(chunk1);
        let evs2 = state.apply(chunk2);
        let starts: usize = evs1
            .iter()
            .chain(evs2.iter())
            .filter(|e| matches!(e, ProviderEvent::MessageStart { .. }))
            .count();
        assert_eq!(starts, 1);
    }

    #[test]
    fn state_emits_message_delta_in_finalize_with_token_counts() {
        // finish_reason chunk — usage is null (arrives on a later chunk)
        let stop_line = r#"data: {"id":"c1","object":"chat.completion.chunk","model":"qwen","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":null}"#;
        // Separate usage chunk (vLLM pattern)
        let usage_line = r#"data: {"id":"c1","object":"chat.completion.chunk","model":"qwen","choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;

        let mut state = OpenAiStreamState::new();
        state.first_seen = true;
        state.model = "qwen".to_owned();

        let evs1 = state.apply(parse_openai_sse_line(stop_line).unwrap().unwrap());
        // MessageDelta must NOT be in evs1 — it is buffered.
        assert!(
            !evs1
                .iter()
                .any(|e| matches!(e, ProviderEvent::MessageDelta { .. })),
            "MessageDelta should not be emitted during apply"
        );

        let _evs2 = state.apply(parse_openai_sse_line(usage_line).unwrap().unwrap());

        // finalize() should emit MessageDelta with correct completion_tokens.
        let final_evs = state.finalize();
        let delta = final_evs
            .iter()
            .find(|e| matches!(e, ProviderEvent::MessageDelta { .. }))
            .expect("MessageDelta must be in finalize output");
        assert!(matches!(
            delta,
            ProviderEvent::MessageDelta {
                stop_reason,
                output_tokens: 5
            }
            if stop_reason == "stop"
        ));
    }

    #[test]
    fn tool_calls_finish_reason_normalised_to_tool_use() {
        let line = r#"data: {"id":"c1","object":"chat.completion.chunk","model":"qwen","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":8,"completion_tokens":3,"total_tokens":11}}"#;
        let chunk = parse_openai_sse_line(line).unwrap().unwrap();
        let mut state = OpenAiStreamState::new();
        state.first_seen = true;
        state.model = "qwen".to_owned();
        let _evs = state.apply(chunk);
        let final_evs = state.finalize();
        assert!(final_evs
            .iter()
            .any(|e| matches!(e, ProviderEvent::MessageDelta { stop_reason, .. } if stop_reason == "tool_use")));
    }

    #[test]
    fn finalize_emits_message_start_and_content_block_start_when_no_chunks() {
        let mut state = OpenAiStreamState::new();
        let evs = state.finalize();
        assert!(
            evs.iter()
                .any(|e| matches!(e, ProviderEvent::MessageStart { .. }))
        );
        assert!(evs.iter().any(|e| matches!(
            e,
            ProviderEvent::ContentBlockStart {
                index: 0,
                block_type,
                ..
            }
            if block_type == "text"
        )));
    }

    #[test]
    fn tool_call_first_delta_emits_content_block_start() {
        let line = r#"data: {"id":"c1","object":"chat.completion.chunk","model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"bash","arguments":""}}]},"finish_reason":null}],"usage":null}"#;
        let chunk = parse_openai_sse_line(line).unwrap().unwrap();
        let mut state = OpenAiStreamState::new();
        state.first_seen = true;
        state.model = "m".to_owned();
        state.next_block_index = 1; // text block already at 0
        let evs = state.apply(chunk);
        assert!(evs.iter().any(|e| matches!(
            e,
            ProviderEvent::ContentBlockStart {
                index: 1,
                block_type,
                tool_use_id: Some(id),
                tool_name: Some(name),
            }
            if block_type == "tool_use" && id == "call_1" && name == "bash"
        )));
    }

    #[test]
    fn tool_call_subsequent_delta_emits_input_json_delta() {
        let first = r#"data: {"id":"c1","object":"chat.completion.chunk","model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"bash","arguments":""}}]},"finish_reason":null}],"usage":null}"#;
        let second = r#"data: {"id":"c1","object":"chat.completion.chunk","model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"command\":"}}]},"finish_reason":null}],"usage":null}"#;
        let mut state = OpenAiStreamState::new();
        state.first_seen = true;
        state.model = "m".to_owned();
        state.next_block_index = 1;
        let _ = state.apply(parse_openai_sse_line(first).unwrap().unwrap());
        let evs2 = state.apply(parse_openai_sse_line(second).unwrap().unwrap());
        assert!(evs2.iter().any(|e| matches!(
            e,
            ProviderEvent::InputJsonDelta { index: 1, partial_json }
            if partial_json == "{\"command\":"
        )));
    }

    #[test]
    fn reasoning_content_wrapped_in_think_tags() {
        // DeepSeek R1 / LiteLLM pass-through: reasoning_content in the delta.
        let line = r#"data: {"id":"r1","object":"chat.completion.chunk","model":"deepseek-reasoner","choices":[{"index":0,"delta":{"reasoning_content":"Let me think"},"finish_reason":null}],"usage":null}"#;
        let chunk = parse_openai_sse_line(line).unwrap().unwrap();
        assert_eq!(
            chunk.choices[0].delta.reasoning_content.as_deref(),
            Some("Let me think")
        );

        let mut state = OpenAiStreamState::new();
        let events = state.apply(chunk);
        let text_delta = events.iter().find_map(|e| {
            if let ProviderEvent::TextDelta { text, .. } = e {
                Some(text.as_str())
            } else {
                None
            }
        });
        assert_eq!(text_delta, Some("<think>Let me think</think>"));
    }

    #[test]
    fn reasoning_content_empty_string_is_suppressed() {
        let line = r#"data: {"id":"r1","object":"chat.completion.chunk","model":"deepseek-reasoner","choices":[{"index":0,"delta":{"reasoning_content":""},"finish_reason":null}],"usage":null}"#;
        let chunk = parse_openai_sse_line(line).unwrap().unwrap();
        let mut state = OpenAiStreamState::new();
        let events = state.apply(chunk);
        assert!(
            !events.iter().any(
                |e| matches!(e, ProviderEvent::TextDelta { text, .. } if text.contains("<think>"))
            ),
            "empty reasoning_content must not emit think tags"
        );
    }
}
