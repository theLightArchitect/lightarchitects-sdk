//! `BuildStrategy` — CORSO-primary agentic build pipeline.
//!
//! A 3-phase loop: **Plan** → **Implement** → **Verify**.
//!
//! Phase 0 (Plan) returns [`Outcome::Pause`] to await operator approval of
//! the architecture before implementation begins.  Phases 1-2 run autonomously.

use async_trait::async_trait;

use super::{
    error::LoopError,
    meta_skill::{LoopOutput, LoopState},
    runner::{HitlRequest, Outcome, StepContext, Strategy},
};

/// Three-phase CORSO build loop.
///
/// | Phase | Name | Action |
/// |-------|------|--------|
/// | 0 | Plan | Produce architecture plan → pause for approval |
/// | 1 | Implement | Execute plan |
/// | 2 | Verify | Run gates + smoke test → halt with output |
pub struct BuildStrategy {
    /// Maximum phases before the strategy force-halts (circuit breaker).
    pub max_phases: u32,
}

impl BuildStrategy {
    /// Construct with the default 3-phase limit.
    #[must_use]
    pub fn new() -> Self {
        Self { max_phases: 3 }
    }
}

impl Default for BuildStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for BuildStrategy {
    type State = LoopState;
    type Output = LoopOutput;

    async fn step(
        &self,
        mut state: LoopState,
        _ctx: &StepContext,
    ) -> Result<Outcome<LoopState, LoopOutput>, LoopError> {
        if state.phase >= self.max_phases {
            return Ok(Outcome::Halt(LoopOutput {
                strategy_name: self.name().to_string(),
                summary: "Build complete (max-phases circuit breaker)".into(),
                phases_run: state.phase,
                artifacts: state.artifacts,
            }));
        }

        match state.phase {
            0 => {
                // Plan phase: produce architecture summary, then pause for approval.
                state.artifacts.push("arch/plan.md".into());
                state.phase = 1;
                Ok(Outcome::Pause(
                    state,
                    HitlRequest {
                        question: "Architecture plan ready. Approve to begin implementation?"
                            .into(),
                        options: vec![
                            "Approve — begin implementation".into(),
                            "Revise plan".into(),
                            "Cancel build".into(),
                        ],
                        header: "Arch review".into(),
                    },
                ))
            }
            1 => {
                // Implement phase: code + commit.
                state.artifacts.push("src/".into());
                state.phase = 2;
                Ok(Outcome::Continue(state))
            }
            _ => {
                // Verify phase (phase ≥ 2): run gates, halt.
                state.artifacts.push(".gate-evals/build-phase.yaml".into());
                Ok(Outcome::Halt(LoopOutput {
                    strategy_name: self.name().to_string(),
                    summary: format!(
                        "Build verified — {} artifact(s) produced",
                        state.artifacts.len()
                    ),
                    phases_run: state.phase + 1,
                    artifacts: state.artifacts,
                }))
            }
        }
    }

    fn name(&self) -> &'static str {
        "BuildStrategy"
    }

    fn estimated_step_cost_usd(&self) -> f64 {
        0.10
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
    async fn phase_0_pauses_for_approval() {
        let s = BuildStrategy::new();
        let state = LoopState::new("build context");
        let outcome = s.step(state, &ctx()).await.unwrap();
        assert!(
            matches!(outcome, Outcome::Pause(ref st, _) if st.phase == 1),
            "phase 0 should pause with state advanced to phase 1"
        );
    }

    #[tokio::test]
    async fn phase_1_continues() {
        let s = BuildStrategy::new();
        let mut state = LoopState::new("ctx");
        state.phase = 1;
        let outcome = s.step(state, &ctx()).await.unwrap();
        assert!(matches!(outcome, Outcome::Continue(ref st) if st.phase == 2));
    }

    #[tokio::test]
    async fn phase_2_halts() {
        let s = BuildStrategy::new();
        let mut state = LoopState::new("ctx");
        state.phase = 2;
        let outcome = s.step(state, &ctx()).await.unwrap();
        assert!(matches!(outcome, Outcome::Halt(ref out) if out.phases_run == 3));
    }

    #[tokio::test]
    async fn circuit_breaker_halts_at_max_phases() {
        let s = BuildStrategy { max_phases: 1 };
        let mut state = LoopState::new("ctx");
        state.phase = 1;
        let outcome = s.step(state, &ctx()).await.unwrap();
        assert!(matches!(outcome, Outcome::Halt(_)));
    }
}
