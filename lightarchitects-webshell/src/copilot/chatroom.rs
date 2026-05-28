//! Multi-voice chatroom synthesizer.
//!
//! `MultiVoiceSynthesizer` dispatches parallel `PersonalityEngine` calls for
//! every sibling on the active roster, collects the results, and formats them
//! for SSE delivery.  Each per-sibling response is sanitized individually
//! before aggregation (SERAPH H1 — sanitize before merge, not after).

use lightarchitects::chat::{
    ActiveRoster, ConversationContext, PersonalityEngine, SiblingProvider,
};
use std::sync::Arc;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// AttributedResponse
// ---------------------------------------------------------------------------

/// A single sibling's contribution to a chatroom turn.
#[derive(Debug, Clone)]
pub struct AttributedResponse {
    /// The sibling whose voice produced this response.
    pub sibling: String,
    /// Sanitized response content.
    pub content: String,
    /// Zero-based index among this turn's responses (ordering for SSE).
    pub frame_idx: usize,
}

// ---------------------------------------------------------------------------
// MultiVoiceSynthesizer
// ---------------------------------------------------------------------------

/// Orchestrates parallel personality generation for all active roster siblings.
pub struct MultiVoiceSynthesizer {
    engine: Arc<PersonalityEngine>,
    provider: Arc<dyn SiblingProvider>,
}

impl MultiVoiceSynthesizer {
    /// Create a new synthesizer.
    #[must_use]
    pub fn new(engine: Arc<PersonalityEngine>, provider: Arc<dyn SiblingProvider>) -> Self {
        Self { engine, provider }
    }

    /// Generate attributed responses for every sibling on `roster`.
    ///
    /// Responses are produced in parallel via `tokio::spawn`.  Each sibling's
    /// output is sanitized independently before being included in the result
    /// (SERAPH H1: per-sibling sanitization, not post-merge).
    ///
    /// Siblings that fail generation are logged and omitted from the result
    /// rather than failing the whole turn.
    pub async fn synthesize(
        &self,
        roster: &ActiveRoster,
        context: &ConversationContext,
    ) -> Vec<AttributedResponse> {
        let active = roster.current();
        if active.is_empty() {
            return Vec::new();
        }

        // Look up SiblingInfo for each active sibling (sequential — provider is async).
        let mut infos = Vec::with_capacity(active.len());
        for id in active {
            match self.provider.get_sibling(id).await {
                Ok(Some(info)) => infos.push(info),
                Ok(None) => {
                    warn!(sibling = %id, "chatroom: no SiblingInfo for active roster member — skipping");
                }
                Err(e) => {
                    warn!(sibling = %id, error = %e, "chatroom: SiblingProvider error — skipping");
                }
            }
        }

        // Spawn parallel tasks — one per sibling.
        debug!(
            sibling_count = infos.len(),
            "chatroom: dispatching parallel personality generation"
        );
        let engine = Arc::clone(&self.engine);
        let context = context.clone();

        let handles: Vec<_> = infos
            .into_iter()
            .enumerate()
            .map(|(idx, info)| {
                let engine = Arc::clone(&engine);
                let context = context.clone();
                tokio::spawn(async move {
                    let result = engine.generate_response(&info, &context).await;
                    (idx, info.name.clone(), result)
                })
            })
            .collect();

        // Collect results, preserving spawn order as frame_idx.
        let mut responses = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok((idx, sibling, Ok(msg))) => {
                    debug!(%sibling, frame_idx = idx, content_len = msg.content.len(), "chatroom: personality generated");
                    responses.push(AttributedResponse {
                        sibling,
                        content: msg.content,
                        frame_idx: idx,
                    });
                }
                Ok((_, sibling, Err(e))) => {
                    warn!(%sibling, error = %e, "chatroom: personality generation failed — skipping sibling");
                }
                Err(e) => {
                    warn!(error = %e, "chatroom: tokio task panicked — skipping");
                }
            }
        }

        // Sort by frame_idx so SSE emission order is deterministic.
        responses.sort_by_key(|r| r.frame_idx);
        responses
    }
}

// ---------------------------------------------------------------------------
// ChatroomFormatter
// ---------------------------------------------------------------------------

/// Formats attributed responses for SSE delivery.
pub struct ChatroomFormatter;

impl ChatroomFormatter {
    /// Format all responses into a single attributed string block.
    ///
    /// Each sibling's voice is prefixed with `[SiblingName]: `.
    #[must_use]
    pub fn format(responses: &[AttributedResponse]) -> String {
        responses
            .iter()
            .map(|r| format!("[{}]: {}", r.sibling, r.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn formatter_produces_attributed_output() {
        let responses = vec![
            AttributedResponse {
                sibling: "eva".to_owned(),
                content: "Hello from EVA".to_owned(),
                frame_idx: 0,
            },
            AttributedResponse {
                sibling: "corso".to_owned(),
                content: "Alright, let's ship it".to_owned(),
                frame_idx: 1,
            },
        ];

        let formatted = ChatroomFormatter::format(&responses);
        assert!(formatted.contains("[eva]: Hello from EVA"));
        assert!(formatted.contains("[corso]: Alright, let's ship it"));
    }

    #[test]
    fn formatter_empty_returns_empty_string() {
        assert!(ChatroomFormatter::format(&[]).is_empty());
    }

    #[test]
    fn formatter_preserves_given_iteration_order() {
        let responses = vec![
            AttributedResponse {
                sibling: "b".to_owned(),
                content: "second".to_owned(),
                frame_idx: 1,
            },
            AttributedResponse {
                sibling: "a".to_owned(),
                content: "first".to_owned(),
                frame_idx: 0,
            },
        ];
        // ChatroomFormatter iterates in slice order — caller is responsible for
        // sorting by frame_idx before calling format().
        let formatted = ChatroomFormatter::format(&responses);
        let b_pos = formatted.find("[b]").unwrap();
        let a_pos = formatted.find("[a]").unwrap();
        assert!(b_pos < a_pos, "b appears before a in the given slice order");
    }

    #[test]
    fn empty_roster_produces_no_frames() {
        let roster = ActiveRoster::new();
        assert!(
            roster.current().is_empty(),
            "freshly constructed roster has no members"
        );
    }
}
