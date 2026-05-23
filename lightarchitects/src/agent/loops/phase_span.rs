//! `OTel` semantic-convention span helpers for loop phase boundaries.
//!
//! Emits `gen_ai.phase.*` attributes per the LASDLC v2.6.1 observability
//! contract (manifest.yaml `observability_contract`). These complement the
//! per-step spans in [`super::trace`] with coarser phase-boundary events.

use crate::ayin::{
    TraceError,
    span::{Actor, TraceContext, TraceOutcome, TraceSpan},
};

/// Emit an AYIN [`TraceSpan`] marking the start of a named loop phase.
///
/// Useful for wrapping strategy runs in an outer phase span (e.g. "critique",
/// "refine", "ensemble.vote").
///
/// # Errors
///
/// Returns [`TraceError`] if the span builder fails (unreachable in practice).
pub fn phase_start(
    phase_name: &str,
    strategy_name: &str,
    session_id: Option<&str>,
) -> Result<TraceSpan, TraceError> {
    let mut builder = TraceContext::new(Actor::claude(), "gen_ai.phase.start")
        .outcome(TraceOutcome::Continue)
        .metadata(serde_json::json!({
            "gen_ai.phase.name": phase_name,
            "gen_ai.phase.strategy": strategy_name,
        }));
    if let Some(sid) = session_id {
        builder = builder.session_id(sid);
    }
    builder.finish()
}

/// Emit an AYIN [`TraceSpan`] marking the completion of a named loop phase.
///
/// # Errors
///
/// Returns [`TraceError`] if the span builder fails (unreachable in practice).
pub fn phase_end(
    phase_name: &str,
    strategy_name: &str,
    turns_completed: u32,
    total_cost_usd: f64,
    halted_normally: bool,
    session_id: Option<&str>,
) -> Result<TraceSpan, TraceError> {
    let outcome = if halted_normally {
        TraceOutcome::Continue
    } else {
        TraceOutcome::Error("phase terminated abnormally".into())
    };
    let mut builder = TraceContext::new(Actor::claude(), "gen_ai.phase.end")
        .outcome(outcome)
        .metadata(serde_json::json!({
            "gen_ai.phase.name": phase_name,
            "gen_ai.phase.strategy": strategy_name,
            "gen_ai.phase.turns_completed": turns_completed,
            "gen_ai.phase.total_cost_usd": total_cost_usd,
            "gen_ai.phase.halted_normally": halted_normally,
        }));
    if let Some(sid) = session_id {
        builder = builder.session_id(sid);
    }
    builder.finish()
}
