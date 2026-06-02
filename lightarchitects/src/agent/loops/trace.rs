//! Dual-emit observability for loop steps.
//!
//! Every step emits two signals:
//!
//! 1. A `tracing::info!` event — consumed by any [`tracing::Subscriber`] wired
//!    into the process (e.g. stdout JSON, OTLP exporter).
//! 2. A [`TraceSpan`] built via [`TraceContext`] — AYIN-native structured record
//!    that callers can submit to the AYIN dashboard at `:3742`.
//!
//! Both signals are emitted unconditionally — disk write is fire-and-forget via
//! [`emit_span_background`].

use std::time::Instant;

use tracing::info;

use crate::ayin::{
    emit_span_background,
    span::{Actor, TraceContext, TraceError, TraceOutcome, TraceSpan},
};

/// Record a strategy dispatch decision and write the span to the AYIN trace store.
///
/// Called once per `run_strategy` / `dispatch_strategy_initial` invocation, before the
/// loop begins. Emits the Mixture-of-Experts routing rationale as span metadata
/// (Northstar P3 mechanical check #4):
///
/// - `expert.selected` — canonical strategy ID (e.g. `"build"`, `"react"`)
/// - `expert.selection_rationale` — human-readable routing reason
/// - `expert.composition_latency_ms` — wall-clock from dispatch start to first step
/// - `loop.role` — domain role string used to resolve the profile
/// - `loop.phase` — LASDLC phase affinity context
///
/// The disk write is fire-and-forget via [`emit_span_background`]; the `tracing::info!`
/// is always emitted regardless of whether the background write succeeds.
pub fn emit_dispatch(
    actor: &str,
    strategy_name: &str,
    role: Option<&str>,
    phase: Option<&str>,
    start: Instant,
) {
    let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let role_str = role.unwrap_or("unspecified");
    let phase_str = phase.unwrap_or("unspecified");
    let rationale = format!("strategy='{strategy_name}' role='{role_str}' phase='{phase_str}'");

    info!(
        actor,
        strategy = strategy_name,
        role = role_str,
        phase = phase_str,
        latency_ms,
        "loop dispatch"
    );

    let ctx = TraceContext::new(Actor::new(actor), "loop.dispatch")
        .outcome(TraceOutcome::Continue)
        .metadata(serde_json::json!({
            "expert.selected": strategy_name,
            "expert.selection_rationale": rationale,
            "expert.composition_latency_ms": latency_ms,
            "loop.role": role_str,
            "loop.phase": phase_str,
        }));
    emit_span_background(ctx);
}

/// Record a single loop step and return the corresponding AYIN [`TraceSpan`].
///
/// Emits `tracing::info!` unconditionally. Constructs and returns the
/// [`TraceSpan`] for optional AYIN submission by the caller.
///
/// # Arguments
///
/// * `strategy_name` — human-readable strategy identifier (e.g. `"CritiqueRefine"`).
/// * `turn` — 1-based step index within the current loop run.
/// * `step_cost_usd` — USD cost of this step as reported by the provider.
/// * `start` — wall-clock instant the step began (for duration calculation).
/// * `halted` — whether this step produced a final [`Outcome::Halt`].
/// * `session_id` — optional session correlation key for AYIN cross-referencing.
///
/// # Errors
///
/// Returns an error only if the [`TraceContext`] builder fails (e.g. missing
/// required fields). In practice this is unreachable for well-formed inputs.
pub fn emit_step(
    strategy_name: &str,
    turn: u32,
    step_cost_usd: f64,
    start: Instant,
    halted: bool,
    session_id: Option<&str>,
) -> Result<TraceSpan, TraceError> {
    let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let outcome = TraceOutcome::Continue;

    info!(
        strategy = strategy_name,
        turn, step_cost_usd, duration_ms, halted, "loop step"
    );

    let mut builder = TraceContext::new(Actor::claude(), "loop.step")
        .outcome(outcome)
        .metadata(serde_json::json!({
            "gen_ai.phase.strategy": strategy_name,
            "gen_ai.phase.turn": turn,
            "gen_ai.phase.cost_usd": step_cost_usd,
            "gen_ai.phase.halted": halted,
        }));

    if let Some(sid) = session_id {
        builder = builder.session_id(sid);
    }

    builder.finish()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emit_dispatch_does_not_panic_with_full_args() {
        // emit_dispatch spawns a background task — needs a Tokio runtime.
        let start = Instant::now();
        emit_dispatch(
            "gateway",
            "react",
            Some("researcher"),
            Some("research"),
            start,
        );
    }

    #[tokio::test]
    async fn emit_dispatch_does_not_panic_with_none_role_phase() {
        let start = Instant::now();
        emit_dispatch("copilot", "build", None, None, start);
    }

    #[test]
    fn emit_step_produces_valid_span() {
        let start = Instant::now();
        let span = emit_step("CritiqueRefine", 3, 0.001, start, false, Some("sess-1")).unwrap();
        assert_eq!(span.action, "loop.step");
        let meta = &span.metadata;
        assert_eq!(meta["gen_ai.phase.strategy"], "CritiqueRefine");
        assert_eq!(meta["gen_ai.phase.turn"], 3);
        assert!(!meta["gen_ai.phase.halted"].as_bool().unwrap());
    }
}
