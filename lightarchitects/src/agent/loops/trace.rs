//! Dual-emit observability for loop steps.
//!
//! Every step emits two signals:
//!
//! 1. A `tracing::info!` event — consumed by any [`tracing::Subscriber`] wired
//!    into the process (e.g. stdout JSON, OTLP exporter).
//! 2. A [`TraceSpan`] built via [`TraceContext`] — AYIN-native structured record
//!    that callers can submit to the AYIN dashboard at `:3742`.
//!
//! Callers that do not need AYIN spans can ignore the returned [`TraceSpan`].
//! The `tracing::info!` is always emitted regardless.

use std::time::Instant;

use tracing::info;

use crate::ayin::{
    TraceError,
    span::{Actor, TraceContext, TraceOutcome, TraceSpan},
};

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
