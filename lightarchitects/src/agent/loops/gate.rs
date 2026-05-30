//! `GateStrategy` — 7-gate sequential evaluation loop with phase-back.
//!
//! An L2 strategy implementing the LASDLC `/GATE` protocol:
//! **V0** → **Q** → **S** → **I** → **N** → **D** → **V**.
//!
//! Each step evaluates the gate at `state.phase`. A failing gate (signalled
//! by `state.meta["gate_<label>"] = "fail"`) does NOT advance the phase —
//! the loop returns [`Outcome::Continue`] with the same phase so the caller
//! can remediate and retry. A passing gate advances the phase and records the
//! gate result in `state.meta`. All 7 gates passing halts with success.
//!
//! L2 class: uses shared [`LoopState`] and [`LoopOutput`]; joins
//! [`RegisteredStrategy`] for webshell dispatch. Gate evaluation is
//! deterministic — no [`BcraExecutor`]-style trait required.
//!
//! [`RegisteredStrategy`]: super::registry::RegisteredStrategy
//! [`BcraExecutor`]: super::bcra::BcraExecutor

use async_trait::async_trait;

use super::{
    error::LoopError,
    meta_skill::{LoopOutput, LoopState},
    runner::{Outcome, StepContext, Strategy},
};

// ── Phase ─────────────────────────────────────────────────────────────────────

/// LASDLC gate phases, executed sequentially.
///
/// A failing gate triggers a phase-back to the gate that owns the failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GatePhase {
    /// V0 — pre-flight invariants (worktree state, disk, main sync).
    V0,
    /// Q — quality gates (fmt + clippy + tests).
    Quality,
    /// S — security gates (secrets, injection, CVE).
    Security,
    /// I — integration gates (rebase, conflicts, post-rebase build).
    Integration,
    /// N — Northstar alignment (pillar advancement, no regressions).
    Northstar,
    /// D — documentation gates (public items documented).
    Docs,
    /// V — verification gates (E2E, smoke tests).
    Verify,
}

impl GatePhase {
    /// Short label used in AYIN spans and logs.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::V0 => "V0",
            Self::Quality => "Q",
            Self::Security => "S",
            Self::Integration => "I",
            Self::Northstar => "N",
            Self::Docs => "D",
            Self::Verify => "V",
        }
    }

    /// All phases in sequential order.
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::V0,
            Self::Quality,
            Self::Security,
            Self::Integration,
            Self::Northstar,
            Self::Docs,
            Self::Verify,
        ]
        .into_iter()
    }

    /// Convert a 0-based index to the corresponding phase.
    #[must_use]
    pub fn from_index(n: u32) -> Option<Self> {
        match n {
            0 => Some(Self::V0),
            1 => Some(Self::Quality),
            2 => Some(Self::Security),
            3 => Some(Self::Integration),
            4 => Some(Self::Northstar),
            5 => Some(Self::Docs),
            6 => Some(Self::Verify),
            _ => None,
        }
    }
}

// ── Strategy ──────────────────────────────────────────────────────────────────

/// Seven-gate sequential evaluation loop.
///
/// Uses [`LoopState`] and [`LoopOutput`] (L2 class).
///
/// ## Gate protocol
///
/// Each step consults `state.meta["gate_<label>"]`:
/// - `"fail"` → gate fails; phase stays the same (caller must remediate and retry).
/// - anything else (or absent) → gate passes; phase advances.
///
/// All 7 gates passing in sequence halts with a `"PASS"` summary.
/// The circuit-breaker fires if `state.phase ≥ max_iterations` without halting.
pub struct GateStrategy {
    /// Maximum gate iterations before force-halt (circuit breaker).
    pub max_iterations: u32,
}

impl GateStrategy {
    /// Construct with the default 7-iteration limit.
    #[must_use]
    pub fn new() -> Self {
        Self { max_iterations: 7 }
    }
}

impl Default for GateStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for GateStrategy {
    type State = LoopState;
    type Output = LoopOutput;

    async fn step(
        &self,
        mut state: LoopState,
        _ctx: &StepContext,
    ) -> Result<Outcome<LoopState, LoopOutput>, LoopError> {
        // Circuit breaker — should only fire if a gate has been retried
        // max_iterations times without advancing past it.
        if state.phase >= self.max_iterations {
            return Ok(Outcome::Halt(LoopOutput {
                strategy_name: self.name().to_string(),
                summary: "Gate evaluation complete (circuit breaker)".into(),
                phases_run: state.phase,
                artifacts: state.artifacts,
            }));
        }

        let gate = GatePhase::from_index(state.phase)
            .ok_or_else(|| LoopError::StepFailed(format!("no gate at index {}", state.phase)))?;

        let meta_key = format!("gate_{}", gate.label());
        let failed = state.meta.get(&meta_key).map(String::as_str) == Some("fail");

        if failed {
            // Gate failed — record failure, do NOT advance phase.
            state
                .meta
                .insert(format!("gate_{}_result", gate.label()), "FAIL".into());
            return Ok(Outcome::Continue(state));
        }

        // Gate passed — record result and advance phase.
        state
            .meta
            .insert(format!("gate_{}_result", gate.label()), "PASS".into());
        state
            .artifacts
            .push(format!(".gate-evals/{}.yaml", gate.label()));

        // Halt after the final gate (Verify, index 6).
        if gate == GatePhase::Verify {
            return Ok(Outcome::Halt(LoopOutput {
                strategy_name: self.name().to_string(),
                summary: "All 7 LASDLC gates passed — VALIDATED".into(),
                phases_run: state.phase + 1,
                artifacts: state.artifacts,
            }));
        }

        state.phase += 1;
        Ok(Outcome::Continue(state))
    }

    fn name(&self) -> &'static str {
        "gate"
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::agent::{ChainContext, loops::runner::StepContext};

    fn ctx() -> StepContext {
        StepContext {
            turn: 1,
            chain: ChainContext::default(),
            session_id: None,
        }
    }

    #[tokio::test]
    async fn all_gates_pass_in_sequence() {
        let strategy = GateStrategy::new();
        let mut state = LoopState::new("test context");

        // Run through all 7 gates.
        for i in 0..7u32 {
            let gate = GatePhase::from_index(i).unwrap();
            let outcome = strategy.step(state.clone(), &ctx()).await.unwrap();
            match outcome {
                Outcome::Continue(next) => {
                    assert_eq!(next.phase, i + 1, "phase should advance after gate {i}");
                    assert_eq!(
                        next.meta
                            .get(&format!("gate_{}_result", gate.label()))
                            .map(String::as_str),
                        Some("PASS")
                    );
                    state = next;
                }
                Outcome::Halt(output) => {
                    assert_eq!(i, 6, "should only halt at Verify gate");
                    assert!(output.summary.contains("VALIDATED"));
                    assert_eq!(output.phases_run, 7);
                    return;
                }
                Outcome::Pause(..) => panic!("GateStrategy should not pause"),
            }
        }
        panic!("should have halted at Verify gate");
    }

    #[tokio::test]
    async fn failing_gate_does_not_advance_phase() {
        let strategy = GateStrategy::new();
        let mut state = LoopState::new("ctx");
        // Force Security gate (index 2) to fail.
        state.meta.insert("gate_S".into(), "fail".into());
        state.phase = 2;

        let outcome = strategy.step(state, &ctx()).await.unwrap();
        let next = match outcome {
            Outcome::Continue(s) => s,
            other => panic!("expected Continue, got {other:?}"),
        };
        // Phase must NOT advance.
        assert_eq!(next.phase, 2);
        assert_eq!(
            next.meta.get("gate_S_result").map(String::as_str),
            Some("FAIL")
        );
    }

    #[tokio::test]
    async fn circuit_breaker_fires_at_max_iterations() {
        let strategy = GateStrategy::new();
        let mut state = LoopState::new("ctx");
        state.phase = 7; // beyond Verify

        let outcome = strategy.step(state, &ctx()).await.unwrap();
        assert!(matches!(outcome, Outcome::Halt(_)));
    }

    #[test]
    fn gate_phase_labels_are_correct() {
        assert_eq!(GatePhase::V0.label(), "V0");
        assert_eq!(GatePhase::Quality.label(), "Q");
        assert_eq!(GatePhase::Security.label(), "S");
        assert_eq!(GatePhase::Integration.label(), "I");
        assert_eq!(GatePhase::Northstar.label(), "N");
        assert_eq!(GatePhase::Docs.label(), "D");
        assert_eq!(GatePhase::Verify.label(), "V");
    }

    #[test]
    fn gate_phase_from_index_round_trips() {
        for i in 0..7u32 {
            let gate = GatePhase::from_index(i).unwrap();
            assert!(GatePhase::all().any(|g| g == gate));
        }
        assert!(GatePhase::from_index(7).is_none());
    }
}
