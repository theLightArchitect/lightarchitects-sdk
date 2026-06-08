//! Conversation session state — `ConvSessionHandle`, `ConvSessionStore`, `ConvSSEEvent`.
//!
//! A [`ConvSessionHandle`] is created by `POST /api/conversation` and lives until
//! TTL eviction or `DELETE /api/conversation/{id}`. All state is behind `Arc` so
//! handles can be cloned across Axum handler threads at zero cost.

use std::{
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};

use dashmap::DashMap;
use lightarchitects::agent::conversation::{ConversationMemory, InMemoryConversationMemory};
use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{copilot::strategy_runner::ResumeRegistry, events::types::CopilotActivityEvent};

/// How long an idle session lives before automatic eviction.
const SESSION_TTL: Duration = Duration::from_secs(3600);

/// How often the eviction sweep wakes up.
const EVICTION_INTERVAL: Duration = Duration::from_secs(300);

/// SSE broadcast channel capacity per session.
const SSE_CHANNEL_CAPACITY: usize = 256;

/// A live conversation session.
///
/// All fields are `Arc`-wrapped so the handle can be cheaply cloned across Axum
/// handler threads. The `inner` mutex is held only during active turn dispatch.
pub struct ConvSessionHandle {
    /// Stable identifier minted at session creation.
    pub session_id: Uuid,
    /// Monotonic clock snapshot when the session was created.
    pub created_at: Instant,
    /// Updated on every incoming message — used for TTL eviction.
    pub last_active: Arc<Mutex<Instant>>,
    /// SSE event fan-out: `event_tx.subscribe()` for a new receiver.
    pub event_tx: broadcast::Sender<ConvSSEEvent>,
    /// Set to `true` by `POST /api/conversation/{id}/interrupt`.
    pub interrupt: Arc<AtomicBool>,
    /// HITL nonce registry — same model as `strategy_runner::ResumeRegistry`.
    pub resume_registry: Arc<ResumeRegistry>,
    /// Mutable turn state — locked during active dispatch.
    pub inner: Arc<Mutex<ConvSessionInner>>,
}

/// Mutable interior of a [`ConvSessionHandle`].
pub struct ConvSessionInner {
    /// Number of completed turns in this session.
    pub turn_count: usize,
    /// Conversation memory — `InMemoryConversationMemory` for v1 (NG1: `HelixSessionMemory`).
    pub memory: Box<dyn ConversationMemory + Send>,
    /// Active dispatch handle — `Some` while a turn is executing, `None` when idle.
    pub active_run: Option<tokio::task::JoinHandle<()>>,
    /// Title derived from the first user message (chars 0..80). `None` until first turn.
    pub title: Option<String>,
}

/// Session store — keyed by session UUID.
pub type ConvSessionStore = DashMap<Uuid, Arc<ConvSessionHandle>>;

/// SSE event emitted to subscribers on the conversation stream.
///
/// Wire format uses `#[serde(tag = "type", rename_all = "snake_case")]`:
/// `{"type":"done","turn_id":"..."}`, `{"type":"strategy_phase","phase":"...","strategy":"..."}`, etc.
#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConvSSEEvent {
    /// Forwarded copilot activity — reuses the existing SSE shape.
    Activity(CopilotActivityEvent),
    /// Strategy phase transition emitted by the strategy runner.
    StrategyPhase {
        /// Strategy execution phase label (e.g. `"analyze"`, `"act"`).
        phase: String,
        /// Canonical strategy name (e.g. `"build"`, `"react"`).
        strategy: String,
    },
    /// Conversation paused waiting for operator input.
    HitlPause {
        /// Single-use resume nonce — POST to `/api/conversation/{id}/resume`.
        nonce: String,
        /// Human-readable prompt shown to the operator.
        prompt: String,
    },
    /// Turn completed successfully.
    Done {
        /// Stable identifier for this completed turn.
        turn_id: Uuid,
    },
    /// Turn failed or provider not available.
    Error {
        /// Human-readable error message suitable for display.
        message: String,
    },
    /// Subscriber fell behind — some events were dropped from the broadcast channel.
    Lag {
        /// Number of events dropped since the last successfully received event.
        skipped: u64,
    },
}

impl ConvSessionHandle {
    /// Create a new session with an empty in-memory conversation memory.
    #[must_use]
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(SSE_CHANNEL_CAPACITY);
        let now = Instant::now();
        Self {
            session_id: Uuid::new_v4(),
            created_at: now,
            last_active: Arc::new(Mutex::new(now)),
            event_tx,
            interrupt: Arc::new(AtomicBool::new(false)),
            resume_registry: Arc::new(ResumeRegistry::new()),
            inner: Arc::new(Mutex::new(ConvSessionInner {
                turn_count: 0,
                memory: Box::new(InMemoryConversationMemory::new()),
                active_run: None,
                title: None,
            })),
        }
    }

    /// Update `last_active` to now — called on every incoming `POST /api/conversation/{id}`.
    pub fn touch(&self) {
        if let Ok(mut ts) = self.last_active.lock() {
            *ts = Instant::now();
        }
    }
}

impl Default for ConvSessionHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn the TTL eviction background task.
///
/// Wakes every [`EVICTION_INTERVAL`] and removes sessions idle longer than [`SESSION_TTL`].
/// Dropping the returned `JoinHandle` aborts the task.
pub fn spawn_eviction_task(store: Arc<ConvSessionStore>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(EVICTION_INTERVAL).await;
            let now = Instant::now();
            store.retain(|_, handle| {
                handle.last_active.lock().is_ok_and(|ts| {
                    // Edge case: ts > now (session created after our snapshot) → retain
                    now.checked_duration_since(*ts)
                        .is_none_or(|age| age < SESSION_TTL)
                })
            });
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn session_handle_new_creates_unique_ids() {
        let a = ConvSessionHandle::new();
        let b = ConvSessionHandle::new();
        assert_ne!(a.session_id, b.session_id);
    }

    #[test]
    fn session_touch_updates_last_active() {
        let handle = ConvSessionHandle::new();
        let before = *handle.last_active.lock().unwrap();
        // Brief sleep to ensure Instant advances
        std::thread::sleep(std::time::Duration::from_millis(1));
        handle.touch();
        let after = *handle.last_active.lock().unwrap();
        assert!(after >= before);
    }

    #[test]
    fn conv_sse_event_done_serializes_correctly() {
        let id = Uuid::new_v4();
        let event = ConvSSEEvent::Done { turn_id: id };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"done\""));
        assert!(json.contains(&id.to_string()));
    }

    #[test]
    fn conv_sse_event_error_serializes_correctly() {
        let event = ConvSSEEvent::Error {
            message: "oops".to_owned(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("oops"));
    }

    #[test]
    fn conv_sse_event_strategy_phase_serializes_correctly() {
        let event = ConvSSEEvent::StrategyPhase {
            phase: "analyze".to_owned(),
            strategy: "build".to_owned(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"strategy_phase\""));
        assert!(json.contains("analyze"));
    }
}
