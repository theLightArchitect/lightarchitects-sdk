//! Strategy loop dispatcher and HITL resume registry.
//!
//! [`StrategyDispatcher`] runs a [`RegisteredStrategy`] on a `tokio::spawn`ed
//! task, streaming [`StepResult`]s through a channel.  When the strategy emits
//! [`Outcome::Pause`], the dispatcher parks the resumed [`LoopState`] in the
//! [`ResumeRegistry`] under a single-use nonce and returns the [`HitlRequest`]
//! to the caller.
//!
//! ## HITL security model
//!
//! The nonce is an 8-byte CSPRNG value hex-encoded → 16-char `request_id`.
//! The `ResumeRegistry` enforces:
//! - **Single-use**: the state is removed on first lookup (confused-deputy
//!   prevention).
//! - **30-minute TTL**: stale parked states are evicted to bound memory.
//! - **Session binding**: callers must pass the `session_id` that was present
//!   at dispatch time; mismatches are rejected as unauthorized.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use futures_util::StreamExt as _;
use lightarchitects::agent::{
    ChainContext,
    loops::{Budget, HitlRequest, LoopRunner, LoopState, Outcome, RegisteredStrategy, Strategy},
};
use tokio::sync::mpsc;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// TTL for parked HITL states (30 minutes).
const RESUME_TTL: Duration = Duration::from_secs(30 * 60);

/// Maximum number of parked HITL states (safety cap against OOM).
const MAX_PARKED_STATES: usize = 256;

// ---------------------------------------------------------------------------
// ParkedState — one suspended strategy session
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ParkedState {
    state: LoopState,
    strategy_id: String,
    session_id: String,
    /// Number of options in the original [`HitlRequest`]; validated on resolve.
    options_count: usize,
    parked_at: Instant,
}

// ---------------------------------------------------------------------------
// ResumeRegistry
// ---------------------------------------------------------------------------

/// Single-use, TTL-bound registry for suspended HITL strategy states.
///
/// Thread-safe via an internal `Mutex`.  All methods are synchronous and
/// complete in O(n) worst case (eviction sweep) or O(1) amortised (lookup).
pub struct ResumeRegistry {
    inner: Mutex<HashMap<String, ParkedState>>,
}

impl ResumeRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Park a suspended strategy state.
    ///
    /// Returns the generated `request_id` (16 hex chars, 8 CSPRNG bytes).
    /// Returns `None` if the registry is full ([`MAX_PARKED_STATES`]).
    ///
    /// # Panics
    ///
    /// Panics if the internal `Mutex` is poisoned (only if a concurrent
    /// writer panicked while holding the lock — not expected in normal use).
    pub fn park(
        &self,
        state: LoopState,
        strategy_id: impl Into<String>,
        session_id: impl Into<String>,
        options_count: usize,
    ) -> Option<String> {
        #[allow(clippy::unwrap_used)]
        let mut guard = self.inner.lock().unwrap();
        evict_expired(&mut guard);

        if guard.len() >= MAX_PARKED_STATES {
            warn!("resume_registry: at capacity ({MAX_PARKED_STATES}) — rejecting park");
            return None;
        }

        let request_id = generate_nonce();
        guard.insert(
            request_id.clone(),
            ParkedState {
                state,
                strategy_id: strategy_id.into(),
                session_id: session_id.into(),
                options_count,
                parked_at: Instant::now(),
            },
        );
        info!(request_id = %request_id, "resume_registry: parked HITL state");
        Some(request_id)
    }

    /// Retrieve and remove a parked state.
    ///
    /// Returns `None` if the `request_id` is unknown, expired, or the
    /// `session_id` does not match the one supplied at park time.
    ///
    /// # Panics
    ///
    /// Panics if the internal `Mutex` is poisoned (only if a concurrent
    /// writer panicked while holding the lock — not expected in normal use).
    pub fn take(&self, request_id: &str, session_id: &str) -> Option<(LoopState, String, usize)> {
        #[allow(clippy::unwrap_used)]
        let mut guard = self.inner.lock().unwrap();

        // Peek TTL before removing — prevents TOCTOU where a remove() fires
        // before the TTL check and silently destroys the parked state on expiry.
        let ttl_ok = guard
            .get(request_id)
            .is_some_and(|e| e.parked_at.elapsed() <= RESUME_TTL);
        if !ttl_ok {
            guard.remove(request_id); // evict the expired entry
            warn!(request_id, "resume_registry: TTL expired — rejecting take");
            return None;
        }

        // Session-id check before removal so we can reject without consuming the entry.
        let session_ok = guard
            .get(request_id)
            .is_some_and(|e| e.session_id == session_id);
        if !session_ok {
            warn!(
                request_id,
                "resume_registry: session_id mismatch — rejecting"
            );
            return None;
        }

        let entry = guard.remove(request_id)?;

        Some((entry.state, entry.strategy_id, entry.options_count))
    }
}

/// Evict all entries that have exceeded [`RESUME_TTL`].
fn evict_expired(guard: &mut HashMap<String, ParkedState>) {
    guard.retain(|_, v| v.parked_at.elapsed() <= RESUME_TTL);
}

impl Default for ResumeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Dispatch result
// ---------------------------------------------------------------------------

/// Outcome of a single strategy dispatch invocation.
#[derive(Debug)]
pub enum DispatchResult {
    /// Strategy ran to completion.
    Halted {
        /// Number of phases completed before halt.
        phases_run: u32,
    },
    /// Strategy paused for operator input.
    Paused {
        /// Registered in [`ResumeRegistry`]; caller passes to HITL route.
        request_id: String,
        /// The HITL question and options presented to the operator.
        hitl: HitlRequest,
    },
    /// Budget exhausted or internal error.
    Error(String),
}

// ---------------------------------------------------------------------------
// StrategyDispatcher
// ---------------------------------------------------------------------------

/// Dispatches a [`RegisteredStrategy`] and handles HITL pause semantics.
pub struct StrategyDispatcher {
    registry: std::sync::Arc<ResumeRegistry>,
}

impl StrategyDispatcher {
    /// Create a dispatcher backed by the given registry.
    #[must_use]
    pub fn new(registry: std::sync::Arc<ResumeRegistry>) -> Self {
        Self { registry }
    }

    /// Run the strategy to completion or first pause.
    ///
    /// `session_id` is stored in the [`ResumeRegistry`] and must be supplied
    /// by the caller on the HITL resume route to prevent session hijacking.
    pub async fn dispatch(
        &self,
        strategy: RegisteredStrategy,
        initial_state: LoopState,
        session_id: impl Into<String>,
        tx: mpsc::Sender<String>,
    ) -> DispatchResult {
        let session_id = session_id.into();
        let strategy_id = strategy.name().to_owned();

        let mut stream = LoopRunner::new(strategy, Budget::unlimited()).run(
            initial_state,
            ChainContext::default(),
            None,
        );

        while let Some(result) = stream.next().await {
            match result {
                Err(e) => {
                    let msg = format!("strategy error: {e}");
                    let _ = tx.send(msg.clone()).await;
                    return DispatchResult::Error(msg);
                }
                Ok(step) => match step.outcome {
                    Outcome::Continue(ref state) => {
                        let progress = format!(
                            "[{strategy_id}] phase {} complete",
                            state.phase.saturating_sub(1)
                        );
                        let _ = tx.send(progress).await;
                    }
                    Outcome::Halt(ref output) => {
                        let summary = output.summary.clone();
                        let phases_run = output.phases_run;
                        let _ = tx.send(format!("[{strategy_id}] halted: {summary}")).await;
                        return DispatchResult::Halted { phases_run };
                    }
                    Outcome::Pause(state, hitl) => {
                        let Some(request_id) = self.registry.park(
                            state,
                            &strategy_id,
                            &session_id,
                            hitl.options.len(),
                        ) else {
                            return DispatchResult::Error(
                                "resume_registry full — HITL rejected".into(),
                            );
                        };
                        let _ = tx
                            .send(format!("[{strategy_id}] paused: {}", hitl.question))
                            .await;
                        return DispatchResult::Paused { request_id, hitl };
                    }
                },
            }
        }

        DispatchResult::Error("strategy stream ended without Halt or Pause".into())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate an 8-byte CSPRNG nonce as a 16-char hex string.
fn generate_nonce() -> String {
    use rand::RngCore as _;
    let mut bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("{:016x}", u64::from_be_bytes(bytes))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // ── ResumeRegistry ───────────────────────────────────────────────────────

    #[test]
    fn park_and_take_succeeds() {
        let reg = ResumeRegistry::new();
        let state = LoopState::new("test context");
        let id = reg.park(state, "build", "sess-abc", 3).unwrap();
        assert_eq!(id.len(), 16, "request_id must be 16 hex chars");
        let (recovered, strategy_id, options_count) = reg.take(&id, "sess-abc").unwrap();
        assert_eq!(recovered.context, "test context");
        assert_eq!(strategy_id, "build");
        assert_eq!(options_count, 3);
    }

    #[test]
    fn take_is_single_use() {
        let reg = ResumeRegistry::new();
        let state = LoopState::new("ctx");
        let id = reg.park(state, "secure", "sess-1", 2).unwrap();
        assert!(reg.take(&id, "sess-1").is_some());
        assert!(reg.take(&id, "sess-1").is_none(), "second take must fail");
    }

    #[test]
    fn take_rejects_unknown_id() {
        let reg = ResumeRegistry::new();
        assert!(reg.take("deadbeefcafebabe", "sess-1").is_none());
    }

    #[test]
    fn take_rejects_mismatched_session() {
        let reg = ResumeRegistry::new();
        let state = LoopState::new("ctx");
        let id = reg.park(state, "scrum", "sess-correct", 1).unwrap();
        assert!(
            reg.take(&id, "sess-wrong").is_none(),
            "mismatched session must be rejected"
        );
        // State must still be available for the correct session after failed take.
        assert!(
            reg.take(&id, "sess-correct").is_some(),
            "correct session must still succeed after failed take"
        );
    }

    #[test]
    fn nonce_uniqueness_within_batch() {
        let ids: Vec<String> = (0..32).map(|_| generate_nonce()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "all nonces must be unique");
    }

    // ── StrategyDispatcher ───────────────────────────────────────────────────

    #[tokio::test]
    async fn dispatcher_halts_autonomous_strategy() {
        use lightarchitects::agent::loops::StrategyRegistry;

        let registry = std::sync::Arc::new(ResumeRegistry::new());
        let dispatcher = StrategyDispatcher::new(std::sync::Arc::clone(&registry));
        let (tx, mut rx) = mpsc::channel(16);

        let strategy = StrategyRegistry::lookup("enrich").unwrap();
        let result = dispatcher
            .dispatch(strategy, LoopState::new("ctx"), "sess-1", tx)
            .await;

        assert!(
            matches!(result, DispatchResult::Halted { .. }),
            "enrich must halt without pause"
        );

        // Drain progress messages.
        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }
        assert!(
            !messages.is_empty(),
            "halted strategy must emit progress messages"
        );
    }

    #[tokio::test]
    async fn dispatcher_parks_on_hitl_pause() {
        use lightarchitects::agent::loops::StrategyRegistry;

        let registry = std::sync::Arc::new(ResumeRegistry::new());
        let dispatcher = StrategyDispatcher::new(std::sync::Arc::clone(&registry));
        let (tx, _rx) = mpsc::channel(16);

        let strategy = StrategyRegistry::lookup("build").unwrap();
        let result = dispatcher
            .dispatch(strategy, LoopState::new("ctx"), "sess-build", tx)
            .await;

        let DispatchResult::Paused { request_id, hitl } = result else {
            panic!("build strategy must pause on phase 0");
        };
        assert_eq!(request_id.len(), 16);
        assert!(!hitl.options.is_empty());

        // Verify state was parked and is retrievable.
        assert!(
            registry.take(&request_id, "sess-build").is_some(),
            "parked state must be retrievable"
        );
    }
}
