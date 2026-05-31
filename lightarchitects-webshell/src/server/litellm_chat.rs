//! Lightweight `LiteLLM` chat SSE endpoint — direct streaming for the
//! `CopilotDrawer` polished chat panel. Bypasses the subprocess agents
//! and routes straight through the OpenAI-compatible provider so that
//! token-level streaming arrives at the browser with sub-second latency.
//!
//! Route:
//!   POST /api/litellm/chat   body: `{"message": "...", "history": [...]?}`
//!
//! SSE event shapes emitted (all `data: <json>`):
//!   `{"type":"delta","text":"..."}`     — one event per token chunk
//!   `{"type":"thinking_delta","text":"..."}`  — content inside `<think>...</think>` tags
//!   `{"type":"complete"}`               — terminal frame
//!   `{"type":"error","message":"..."}`  — fatal failure
//!
//! The frontend already speaks this shape (see `CopilotDrawer.svelte`).

use std::{convert::Infallible, sync::Arc};

use axum::{
    Json,
    extract::State,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::{StreamExt, stream};
use serde::Deserialize;
use tokio::time::Duration;

use lightarchitects::agent::openai_compat::OpenAICompatProvider;
use lightarchitects::agent::{AgentRequest, LlmAgentProvider, ProviderEvent};

use crate::server::AppState;

/// One message in the prior conversation. Wire shape mirrors `OpenAI`'s
/// `messages[]` array — `role` is `"user" | "assistant" | "system"`.
#[derive(Debug, Deserialize)]
pub struct HistoryTurn {
    /// Role for this turn: `"user"`, `"assistant"`, or `"system"`.
    pub role: String,
    /// Message body — rendered as-is into the `LiteLLM` request.
    pub content: String,
}

/// Request body for `POST /api/litellm/chat`.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    /// The new user turn to send to the model.
    pub message: String,
    /// Prior turns in this conversation. Optional; default empty.
    #[serde(default)]
    pub history: Vec<HistoryTurn>,
}

/// Stateful classifier that walks a token stream and assigns each delta
/// to either `Text` or `Thinking` based on `<think>...</think>` tags.
///
/// Tags can span chunk boundaries — the classifier buffers a small tail
/// (≤ longest tag) until it knows whether a tag is forming. This avoids
/// leaking `<think` partial bytes into either visible text or the
/// thinking block while the stream is mid-tag.
#[derive(Default)]
struct ThinkSplitter {
    in_think: bool,
    /// Pending bytes that look like they might be a partial tag.
    pending: String,
}

impl ThinkSplitter {
    /// Tags we recognise. Reasoning models emit `<think>...</think>`
    /// (`DeepSeek-R1`, `QwQ`); some emit `<thinking>` instead.
    const OPEN_TAGS: &'static [&'static str] = &["<think>", "<thinking>"];
    const CLOSE_TAGS: &'static [&'static str] = &["</think>", "</thinking>"];
    const MAX_TAG_LEN: usize = 10; // `</thinking>` = 11 — round down to 10 buffer cap.

    /// Push a chunk; returns 0–N classified slices. Buffers tail bytes
    /// that look like they might be the start of a tag.
    fn push(&mut self, chunk: &str) -> Vec<(bool, String)> {
        let combined = format!("{}{}", self.pending, chunk);
        self.pending.clear();
        let mut out: Vec<(bool, String)> = Vec::new();
        let mut cursor = 0usize;

        while cursor < combined.len() {
            let rest = &combined[cursor..];

            // Look for the next tag of the opposite kind.
            let tags = if self.in_think {
                Self::CLOSE_TAGS
            } else {
                Self::OPEN_TAGS
            };
            let next_tag = tags
                .iter()
                .filter_map(|t| rest.find(t).map(|i| (i, *t)))
                .min_by_key(|(i, _)| *i);

            if let Some((idx, tag)) = next_tag {
                // Emit everything before the tag with the current mode.
                if idx > 0 {
                    out.push((self.in_think, rest[..idx].to_owned()));
                }
                cursor += idx + tag.len();
                self.in_think = !self.in_think;
            } else {
                // No full tag found. Check if the tail might be a partial tag.
                let tail_start = rest.len().saturating_sub(Self::MAX_TAG_LEN);
                let tail = &rest[tail_start..];
                if tail.contains('<') {
                    // Possible partial tag — emit head, buffer tail.
                    let lt_pos = tail.find('<').map_or(rest.len(), |p| tail_start + p);
                    if lt_pos > 0 {
                        out.push((self.in_think, rest[..lt_pos].to_owned()));
                    }
                    rest[lt_pos..].clone_into(&mut self.pending);
                } else {
                    out.push((self.in_think, rest.to_owned()));
                }
                return out;
            }
        }
        out
    }

    /// Flush remaining buffered bytes at stream end.
    fn flush(self) -> Option<(bool, String)> {
        if self.pending.is_empty() {
            None
        } else {
            Some((self.in_think, self.pending))
        }
    }
}

/// `POST /api/litellm/chat` — SSE chat stream.
pub async fn chat_handler(State(state): State<AppState>, Json(req): Json<ChatRequest>) -> Response {
    let (tx, rx) = tokio::sync::mpsc::channel::<String>(128);

    let cfg = state.litellm_config.read().await;
    let model = cfg.model.clone();
    let provider = match cfg.build_provider() {
        Ok(p) => Arc::new(p),
        Err(e) => {
            tokio::spawn(async move {
                let frame =
                    serde_json::json!({"type":"error","message": format!("provider init: {e}")})
                        .to_string();
                let _ = tx.send(frame).await;
            });
            return sse_response(rx);
        }
    };
    drop(cfg);

    tokio::spawn(async move {
        run_chat_sse(provider, model, req, tx).await;
    });

    sse_response(rx)
}

/// Compose the SSE response from the receiver half.
fn sse_response(rx: tokio::sync::mpsc::Receiver<String>) -> Response {
    let event_stream = stream::unfold(rx, |mut rx| async move {
        let msg = rx.recv().await?;
        let event = Event::default().data(msg);
        Some((Ok::<_, Infallible>(event), rx))
    });
    Sse::new(event_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response()
}

/// Drive the `LiteLLM` streaming call and translate `ProviderEvent`s
/// into SSE frames for the frontend.
async fn run_chat_sse(
    provider: Arc<OpenAICompatProvider>,
    model: String,
    req: ChatRequest,
    tx: tokio::sync::mpsc::Sender<String>,
) {
    // Translate history into the OpenAI-shape JSON the provider's builder expects.
    let history: Vec<serde_json::Value> = req
        .history
        .into_iter()
        .filter_map(|h| {
            let role = match h.role.as_str() {
                "user" | "assistant" | "system" => h.role,
                _ => return None,
            };
            Some(serde_json::json!({"role": role, "content": h.content}))
        })
        .collect();

    let agent = AgentRequest {
        sibling_identity: "litellm-chat".to_owned(),
        user_prompt: req.message,
        schema: None,
        allowed_tools: vec![],
        max_turns: 1,
        max_budget_usd: 1.0,
        model_hint: Some(model),
        parent_span_id: None,
        chain_origin: None,
        chain_depth: 0,
        aud: None,
        conversation_history: history,
        tool_definitions: vec![],
    };

    let san = match agent.sanitize() {
        Ok(s) => s,
        Err(e) => {
            send(
                &tx,
                serde_json::json!({"type":"error","message": format!("sanitize: {e}")}).to_string(),
            )
            .await;
            return;
        }
    };

    let mut stream = match provider.spawn_streaming(san).await {
        Ok(s) => s,
        Err(e) => {
            send(
                &tx,
                serde_json::json!({"type":"error","message": format!("provider: {e}")}).to_string(),
            )
            .await;
            return;
        }
    };

    let mut splitter = ThinkSplitter::default();
    while let Some(ev) = stream.next().await {
        if let ProviderEvent::TextDelta { text, .. } = ev {
            for (is_thinking, chunk) in splitter.push(&text) {
                if chunk.is_empty() {
                    continue;
                }
                let typ = if is_thinking {
                    "thinking_delta"
                } else {
                    "delta"
                };
                let frame = serde_json::json!({"type": typ, "text": chunk}).to_string();
                if tx.send(frame).await.is_err() {
                    return;
                }
            }
        }
    }
    // Flush any trailing buffered bytes that didn't form a tag.
    if let Some((is_thinking, leftover)) = splitter.flush() {
        let typ = if is_thinking {
            "thinking_delta"
        } else {
            "delta"
        };
        let _ = tx
            .send(serde_json::json!({"type": typ, "text": leftover}).to_string())
            .await;
    }
    let _ = tx
        .send(serde_json::json!({"type":"complete"}).to_string())
        .await;
}

async fn send(tx: &tokio::sync::mpsc::Sender<String>, msg: String) {
    let _ = tx.send(msg).await;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn splitter_plain_text_passthrough() {
        let mut s = ThinkSplitter::default();
        let out = s.push("hello world");
        assert_eq!(out, vec![(false, "hello world".to_owned())]);
    }

    #[test]
    fn splitter_detects_complete_think_tag() {
        let mut s = ThinkSplitter::default();
        let out = s.push("answer <think>reasoning here</think> done");
        assert_eq!(
            out,
            vec![
                (false, "answer ".to_owned()),
                (true, "reasoning here".to_owned()),
                (false, " done".to_owned()),
            ]
        );
    }

    #[test]
    fn splitter_handles_tag_split_across_chunks() {
        let mut s = ThinkSplitter::default();
        let a = s.push("answer <thi");
        // Tail buffered, only "answer " emitted.
        assert_eq!(a, vec![(false, "answer ".to_owned())]);
        let b = s.push("nk>reason</think> end");
        assert_eq!(
            b,
            vec![(true, "reason".to_owned()), (false, " end".to_owned()),]
        );
    }

    #[test]
    fn splitter_supports_thinking_variant() {
        let mut s = ThinkSplitter::default();
        let out = s.push("<thinking>secret</thinking>visible");
        assert_eq!(
            out,
            vec![(true, "secret".to_owned()), (false, "visible".to_owned()),]
        );
    }
}
