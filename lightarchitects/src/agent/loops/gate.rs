//! `GateStrategy` — 7-gate sequential evaluation loop with phase-back.
//!
//! An L2 strategy implementing the LASDLC `/GATE` protocol:
//! **V0** → **Q** → **S** → **I** → **N** → **D** → **V**.
//!
//! L2 class: uses shared [`LoopState`] and [`LoopOutput`]; joins
//! [`RegisteredStrategy`] for webshell dispatch. Gate evaluation is
//! deterministic — no [`BcraExecutor`]-style trait required.
//!
//! Full phase logic implemented in Phase 3.
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
}

// ── Strategy ──────────────────────────────────────────────────────────────────

/// Seven-gate sequential evaluation loop.
///
/// Uses [`LoopState`] and [`LoopOutput`] (L2 class).
/// Phase 3 implements the full `step()` logic including phase-back.
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
        // Phase 3 implements gate-by-gate evaluation with phase-back.
        state.phase = state.phase.saturating_add(1);
        if state.phase >= self.max_iterations {
            return Ok(Outcome::Halt(LoopOutput {
                strategy_name: self.name().to_string(),
                summary: "Gate evaluation complete".into(),
                phases_run: state.phase,
                artifacts: state.artifacts,
            }));
        }
        Ok(Outcome::Continue(state))
    }

    fn name(&self) -> &'static str {
        "gate"
    }
}
