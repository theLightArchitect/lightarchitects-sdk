//! Strategy routing bridge — dispatches a conversation turn to the correct strategy
//! or falls back to native [`ConversationSession`] dispatch.
//!
//! [`should_route_to_strategy`] covers all 19 strategy profiles via
//! [`StrategyRegistry::profile`] (including Class B strategies such as `react`).
//!
//! [`dispatch_native_turn`] builds a [`ConversationSession`] from the session's
//! accumulated history, runs one turn via `LiteLLM`, and emits [`ConvSSEEvent`]s
//! on the session's broadcast channel.

use std::{
    io,
    sync::{Arc, atomic::Ordering},
};

use async_trait::async_trait;
use lightarchitects::agent::{
    ChainContext,
    conversation::{
        ConversationEvent, ConversationMemory, ConversationSession, InMemoryConversationMemory,
        SessionConfig, Transport,
    },
    loops::StrategyRegistry,
    openai_compat::OpenAICompatProvider,
};
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

use crate::server::litellm_state::LitellmConfig;

use super::session::{ConvSSEEvent, ConvSessionHandle};

// ── Strategy routing ──────────────────────────────────────────────────────────

/// Return the canonical strategy name if `message` begins with a known strategy prefix.
///
/// Matching is case-insensitive and strips leading whitespace.
/// Checks all registered strategy profiles via [`StrategyRegistry::profile`].
/// Returns `None` for native dispatch.
///
/// # Examples
///
/// ```
/// # use lightarchitects_webshell::conversation::strategy_bridge::should_route_to_strategy;
/// assert_eq!(should_route_to_strategy("/build scaffold auth"), Some("build"));
/// assert_eq!(should_route_to_strategy("/REACT audit"), Some("react"));
/// assert_eq!(should_route_to_strategy("plain message"), None);
/// assert_eq!(should_route_to_strategy("/unknown cmd"), None);
/// ```
pub fn should_route_to_strategy(message: &str) -> Option<&'static str> {
    let trimmed = message.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }
    let slug = trimmed[1..].split_whitespace().next().unwrap_or("");
    if slug.is_empty() {
        return None;
    }
    // Profile registry covers all 19 strategies including Class B (react, etc.)
    StrategyRegistry::profile(&slug.to_lowercase()).map(|p| p.strategy_name)
}

// ── BroadcastTransport ────────────────────────────────────────────────────────

/// Transport implementation that fans [`ConversationEvent`]s out to the
/// session's broadcast channel as [`ConvSSEEvent`]s.
///
/// Text chunks become `Activity` events reusing the [`CopilotActivityEvent`]
/// wire shape so the frontend can render them without new event-type handling.
struct BroadcastTransport {
    tx: broadcast::Sender<ConvSSEEvent>,
    session_id: Uuid,
}

impl BroadcastTransport {
    fn new(tx: broadcast::Sender<ConvSSEEvent>, session_id: Uuid) -> Self {
        Self { tx, session_id }
    }

    fn send_activity(&self, kind: &str, text: &str) {
        use crate::events::types::CopilotActivityEvent;
        let event = CopilotActivityEvent {
            build_id: self.session_id.to_string(),
            kind: kind.to_owned(),
            summary: Some(text.chars().take(500).collect()),
            raw: serde_json::json!({ "type": kind, "content": text }),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let _ = self.tx.send(ConvSSEEvent::Activity(event));
    }
}

#[async_trait]
impl Transport for BroadcastTransport {
    async fn emit(&mut self, event: &ConversationEvent) -> io::Result<()> {
        match event {
            ConversationEvent::Text { chunk } => self.send_activity("assistant", chunk),
            ConversationEvent::Thinking { content } => self.send_activity("thinking", content),
            ConversationEvent::StatusUpdate { text } => self.send_activity("system", text),
            ConversationEvent::Error { message, .. } => {
                let _ = self.tx.send(ConvSSEEvent::Error {
                    message: message.clone(),
                });
            }
            // Complete is handled post-run_turn (Done event with turn_id)
            // ToolStart, ToolComplete, TokenUsage are internal — not forwarded in v1
            _ => {}
        }
        Ok(())
    }
}

// ── Native dispatch ───────────────────────────────────────────────────────────

/// Dispatch a native (non-strategy) conversation turn.
///
/// Builds an [`OpenAICompatProvider`] from `litellm_config`, replays the
/// session's accumulated history into a fresh [`ConversationSession`], runs
/// one turn, and emits [`ConvSSEEvent`]s on `handle.event_tx`.
///
/// Sync-writes updated history back to [`super::session::ConvSessionInner`]
/// after the turn completes so subsequent turns see the full context window.
///
/// Emits `{"type":"error","message":"..."}` and returns early if:
/// - `LiteLLM` is not configured
/// - A turn is already in progress
pub async fn dispatch_native_turn(
    handle: Arc<ConvSessionHandle>,
    message: String,
    litellm_config: Arc<RwLock<LitellmConfig>>,
) {
    let turn_id = Uuid::new_v4();

    // Extract history snapshot and guard against concurrent turns.
    let history = {
        let inner = handle
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.active_run.is_some() {
            let _ = handle.event_tx.send(ConvSSEEvent::Error {
                message: "A turn is already in progress — send interrupt first".to_owned(),
            });
            return;
        }
        // Snapshot as owned Vec so we can release the lock before the async span.
        inner.memory.turns().to_vec()
    };

    // Build provider from current LiteLLM config.
    let provider: Arc<OpenAICompatProvider> = {
        let cfg = litellm_config.read().await;
        match cfg.build_provider() {
            Ok(p) => Arc::new(p),
            Err(e) => {
                let _ = handle.event_tx.send(ConvSSEEvent::Error {
                    // User-facing message: actionable (configure LiteLLM in settings)
                    message: format!("LiteLLM not configured — check Settings: {e}"),
                });
                return;
            }
        }
    };

    // Replay accumulated history into a fresh in-memory session.
    let mut fresh_mem = InMemoryConversationMemory::new();
    for turn in &history {
        fresh_mem.push(turn.role, turn.content.clone());
    }

    // Clear any stale interrupt from a previous turn before starting.
    handle.interrupt.store(false, Ordering::SeqCst);

    let interrupt_flag = Arc::clone(&handle.interrupt);
    let mut session = ConversationSession::new(SessionConfig::default(), provider)
        .with_memory(Box::new(fresh_mem))
        .with_interrupt_flag(interrupt_flag);

    let mut transport = BroadcastTransport::new(handle.event_tx.clone(), handle.session_id);
    let ctx = ChainContext::default();

    match session.run_turn(&message, &mut transport, &ctx).await {
        Ok(_) => {
            // Sync updated history (user + assistant turns) back to the session store.
            let updated_turns = session.memory.turns().to_vec();
            if let Ok(mut inner) = handle.inner.lock() {
                inner.memory.clear();
                for t in updated_turns {
                    inner.memory.push(t.role, t.content);
                }
                if inner.turn_count == 0 {
                    inner.title = Some(message.chars().take(80).collect());
                }
                inner.turn_count += 1;
                inner.active_run = None;
            }
            let _ = handle.event_tx.send(ConvSSEEvent::Done { turn_id });
        }
        Err(e) => {
            if let Ok(mut inner) = handle.inner.lock() {
                inner.active_run = None;
            }
            let msg = if matches!(
                e,
                lightarchitects::agent::conversation::SessionError::Interrupted
            ) {
                "Turn was interrupted".to_owned()
            } else {
                format!("Turn failed: {e}")
            };
            let _ = handle.event_tx.send(ConvSSEEvent::Error { message: msg });
        }
    }
}

/// Dispatch a conversation turn using strategy routing.
///
/// Sends a `StrategyPhase` event on `handle.event_tx` immediately, then
/// returns a stub Error event (Phase 3 — strategy execution wired in Phase 5).
pub fn dispatch_conversation_strategy(handle: Arc<ConvSessionHandle>, strategy_name: &str) {
    let _ = handle.event_tx.send(ConvSSEEvent::StrategyPhase {
        phase: "pending".to_owned(),
        strategy: strategy_name.to_owned(),
    });
    // WHY: Phase 5 wires StrategyRegistry::lookup + full strategy execution.
    let _ = handle.event_tx.send(ConvSSEEvent::Error {
        message: format!(
            "Strategy '{strategy_name}' execution not yet wired — use native messages for now"
        ),
    });
    let turn_id = Uuid::new_v4();
    let _ = handle.event_tx.send(ConvSSEEvent::Done { turn_id });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_known_strategies() {
        assert_eq!(
            should_route_to_strategy("/build scaffold auth"),
            Some("build")
        );
        assert_eq!(should_route_to_strategy("/secure scan"), Some("secure"));
        assert_eq!(should_route_to_strategy("/scrum review"), Some("scrum"));
        assert_eq!(should_route_to_strategy("/enrich"), Some("enrich"));
    }

    #[test]
    fn routes_case_insensitive() {
        assert_eq!(should_route_to_strategy("/BUILD test"), Some("build"));
        assert_eq!(should_route_to_strategy("  /SECURE scan"), Some("secure"));
    }

    #[test]
    fn does_not_route_plain_messages() {
        assert_eq!(should_route_to_strategy("build this"), None);
        assert_eq!(should_route_to_strategy(""), None);
        assert_eq!(should_route_to_strategy("/"), None);
        assert_eq!(should_route_to_strategy("/unknown_xyz cmd"), None);
    }
}
