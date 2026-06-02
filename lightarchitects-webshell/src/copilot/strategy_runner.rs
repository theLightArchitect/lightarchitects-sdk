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
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use futures_util::StreamExt as _;
use lightarchitects::agent::{
    ChainContext,
    loops::{
        Budget, HitlRequest, LoopRunner, LoopState, Outcome, RegisteredStrategy, Strategy,
        StrategyRegistry,
    },
};
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::copilot::routes::emit_disk_span;

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

        // Resolve budget from the strategy's LoopProfile; fall back to unlimited
        // for Class B strategies that are not in the registry (defensive only —
        // copilot exclusively dispatches Class A strategies via RegisteredStrategy).
        let budget = StrategyRegistry::profile(&strategy_id)
            .map_or_else(Budget::unlimited, |p| Budget::from_policy(&p.budget_policy));

        let mut stream =
            LoopRunner::new(strategy, budget).run(initial_state, ChainContext::default(), None);

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
// Initial dispatch (pre-emption router)
// ---------------------------------------------------------------------------

/// Dispatch a strategy turn from the copilot's pre-emption router.
///
/// Spawns the strategy on a background task and streams progress through the
/// build session's SSE broadcast channel (the same channel the frontend
/// subscribes to for copilot responses). Returns a JSON acknowledgement so
/// the frontend knows the strategy has started and can watch for events.
#[allow(clippy::too_many_lines)]
pub async fn dispatch_strategy_initial(
    strategy: RegisteredStrategy,
    build_id: Uuid,
    message: &str,
    turn_span_id: String,
    event_tx: tokio::sync::broadcast::Sender<crate::events::WebEventV2>,
    resume_registry: Arc<ResumeRegistry>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let strategy_id = strategy.name().to_owned();
    let initial_state = LoopState::new(message);
    let session_id = build_id.to_string();
    let span_id_for_response = turn_span_id.clone();

    // ── P3 mechanical check #4: emit loop.dispatch span with MoE routing metadata ──
    // Role and phase are derived from the LoopProfile so callers don't need to supply them.
    let dispatch_start = Instant::now();
    {
        let profile = StrategyRegistry::profile(&strategy_id);
        let role = profile.and_then(|p| p.optimal_domains.first().copied());
        let phase = profile.map(|p| p.phase_affinity.as_str());
        lightarchitects::agent::loops::trace::emit_dispatch(
            "copilot",
            &strategy_id,
            role,
            phase,
            dispatch_start,
        );
    }

    // ── AYIN lineage: strategy dispatch span ──
    let strategy_span_id = uuid::Uuid::new_v4().to_string();
    emit_strategy_start_span(
        &event_tx,
        &strategy_id,
        build_id,
        &turn_span_id,
        &strategy_span_id,
    );

    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<String>(64);

    // Bridge: forward progress messages as CopilotResponse events.
    let event_tx_progress = event_tx.clone();
    let span_for_progress = turn_span_id.clone();
    let strategy_span_for_progress = strategy_span_id.clone();
    tokio::spawn(async move {
        while let Some(msg) = progress_rx.recv().await {
            // Per-phase AYIN span — child of strategy dispatch span.
            emit_phase_span(
                &event_tx_progress,
                build_id,
                &strategy_span_for_progress,
                &msg,
            );
            let _ = event_tx_progress.send(crate::events::WebEventV2::from_event(
                crate::events::WebEvent::CopilotResponse {
                    chunk: msg,
                    done: false,
                    sibling: None,
                    turn_span_id: Some(span_for_progress.clone()),
                },
                None,
            ));
        }
    });

    // Strategy execution: dispatch to completion or HITL pause.
    let event_tx_final = event_tx;
    let span_for_final = turn_span_id;
    let sid_for_final = strategy_id.clone();
    let strategy_span_for_final = strategy_span_id;
    let build_id_for_final = build_id;
    tokio::spawn(async move {
        let dispatcher = StrategyDispatcher::new(resume_registry);
        let result = dispatcher
            .dispatch(strategy, initial_state, session_id, progress_tx)
            .await;

        emit_strategy_result_span(
            &event_tx_final,
            &sid_for_final,
            build_id_for_final,
            &strategy_span_for_final,
            &result,
        );

        let summary = match &result {
            DispatchResult::Halted { phases_run } => {
                format!("[{sid_for_final}] complete ({phases_run} phases)")
            }
            DispatchResult::Error(e) => format!("[{sid_for_final}] error: {e}"),
            DispatchResult::Paused { hitl, .. } => {
                format!("[{sid_for_final}] paused: {}", hitl.question)
            }
        };
        let _ = event_tx_final.send(crate::events::WebEventV2::from_event(
            crate::events::WebEvent::CopilotResponse {
                chunk: summary,
                done: true,
                sibling: None,
                turn_span_id: Some(span_for_final),
            },
            None,
        ));
    });

    // Return immediately with a JSON acknowledgement — the frontend receives
    // the strategy progress and result through the SSE event stream.
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "status": "strategy_dispatched",
            "strategy_id": strategy_id,
            "build_id": build_id.to_string(),
            "turn_span_id": span_id_for_response,
        })),
    )
        .into_response()
}

// Helpers
// ---------------------------------------------------------------------------

/// Emit the initial AYIN strategy dispatch span (child of `turn_span_id`).
fn emit_strategy_start_span(
    event_tx: &tokio::sync::broadcast::Sender<crate::events::WebEventV2>,
    strategy_id: &str,
    build_id: Uuid,
    turn_span_id: &str,
    strategy_span_id: &str,
) {
    let _ = event_tx.send(crate::events::WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(crate::events::types::TraceSpanSummary {
            id: strategy_span_id.to_owned(),
            parent_id: Some(turn_span_id.to_owned()),
            actor: "copilot".to_owned(),
            action: format!("strategy.dispatch.{strategy_id}"),
            timestamp: chrono::Utc::now().to_rfc3339(),
            duration_ms: 0,
            outcome: serde_json::json!("Continue"),
            metadata: serde_json::json!({
                "strategy_id": strategy_id,
                "build_id": build_id.to_string(),
            }),
            strand_activations: Vec::new(),
            session_id: None,
            decision_points: Vec::new(),
        }),
        Some(build_id),
    ));
    emit_disk_span(
        "copilot",
        &format!("strategy.dispatch.{strategy_id}"),
        serde_json::json!({
            "strategy_id": strategy_id,
            "build_id": build_id.to_string(),
        }),
        lightarchitects::ayin::TraceOutcome::Continue,
        turn_span_id.parse::<Uuid>().ok(),
        Some(build_id),
    );
}

/// Emit a per-phase AYIN span (child of the strategy dispatch span).
fn emit_phase_span(
    event_tx: &tokio::sync::broadcast::Sender<crate::events::WebEventV2>,
    build_id: Uuid,
    strategy_span_id: &str,
    message: &str,
) {
    let phase_span_id = uuid::Uuid::new_v4().to_string();
    let _ = event_tx.send(crate::events::WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(crate::events::types::TraceSpanSummary {
            id: phase_span_id,
            parent_id: Some(strategy_span_id.to_owned()),
            actor: "copilot".to_owned(),
            action: format!("strategy.phase.{message}"),
            timestamp: chrono::Utc::now().to_rfc3339(),
            duration_ms: 0,
            outcome: serde_json::json!("Continue"),
            metadata: serde_json::json!({
                "build_id": build_id.to_string(),
            }),
            strand_activations: Vec::new(),
            session_id: None,
            decision_points: Vec::new(),
        }),
        Some(build_id),
    ));
}

/// Emit a terminal AYIN span for the strategy outcome.
fn emit_strategy_result_span(
    event_tx: &tokio::sync::broadcast::Sender<crate::events::WebEventV2>,
    strategy_id: &str,
    build_id: Uuid,
    strategy_span_id: &str,
    result: &DispatchResult,
) {
    let (outcome, outcome_json) = match result {
        DispatchResult::Halted { phases_run } => (
            "Finish",
            serde_json::json!({
                "phases_run": phases_run,
                "build_id": build_id.to_string(),
            }),
        ),
        DispatchResult::Error(e) => (
            "Error",
            serde_json::json!({
                "error": e,
                "build_id": build_id.to_string(),
            }),
        ),
        DispatchResult::Paused { hitl, .. } => (
            "Paused",
            serde_json::json!({
                "question": &hitl.question,
                "options_count": hitl.options.len(),
                "build_id": build_id.to_string(),
            }),
        ),
    };
    let _ = event_tx.send(crate::events::WebEventV2::from_event(
        crate::events::WebEvent::AyinSpan(crate::events::types::TraceSpanSummary {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: Some(strategy_span_id.to_owned()),
            actor: "copilot".to_owned(),
            action: format!("strategy.{outcome}.{strategy_id}"),
            timestamp: chrono::Utc::now().to_rfc3339(),
            duration_ms: 0,
            outcome: serde_json::json!(outcome),
            metadata: outcome_json,
            strand_activations: Vec::new(),
            session_id: None,
            decision_points: Vec::new(),
        }),
        Some(build_id),
    ));
}

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
