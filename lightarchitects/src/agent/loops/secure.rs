//! `SecureStrategy` — SERAPH-primary security assessment loop.
//!
//! A 3-phase loop: **Recon** → **Analyze** → **Report**.
//!
//! Phase 0 (Recon) pauses before crossing a trust boundary to scan external
//! targets, requiring operator confirmation of scope.

use async_trait::async_trait;

use super::{
    error::LoopError,
    meta_skill::{LoopOutput, LoopState},
    runner::{HitlRequest, Outcome, StepContext, Strategy},
};

/// Three-phase SERAPH security assessment loop.
///
/// | Phase | Name | Action |
/// |-------|------|--------|
/// | 0 | Recon | Map attack surface → pause for scope confirmation |
/// | 1 | Analyze | Enumerate vulnerabilities |
/// | 2 | Report | Produce findings → halt with output |
pub struct SecureStrategy {
    /// Maximum phases before force-halt.
    pub max_phases: u32,
}

impl SecureStrategy {
    /// Construct with the default 3-phase limit.
    #[must_use]
    pub fn new() -> Self {
        Self { max_phases: 3 }
    }
}

impl Default for SecureStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for SecureStrategy {
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
                summary: "Assessment complete (max-phases circuit breaker)".into(),
                phases_run: state.phase,
                artifacts: state.artifacts,
            }));
        }

        match state.phase {
            0 => {
                // Recon: enumerate attack surface, then pause for scope approval.
                state.artifacts.push("seraph/recon.md".into());
                state.phase = 1;
                Ok(Outcome::Pause(
                    state,
                    HitlRequest {
                        question: "Recon complete. Approve active scanning of identified targets?"
                            .into(),
                        // SECURITY: Options are binary (Approve/Cancel) — "Limit scope" is
                        // a false affordance until strategy can receive and honour the choice.
                        options: vec![
                            "Approve — begin active scan".into(),
                            "Cancel assessment".into(),
                        ],
                        header: "Scope gate".into(),
                    },
                ))
            }
            1 => {
                // Analyze: enumerate CVEs, misconfigs, injection vectors.
                state.artifacts.push("seraph/findings.json".into());
                state.phase = 2;
                Ok(Outcome::Continue(state))
            }
            _ => {
                // Report: produce SARIF + narrative, halt.
                state.artifacts.push("seraph/report.sarif".into());
                Ok(Outcome::Halt(LoopOutput {
                    strategy_name: self.name().to_string(),
                    summary: format!(
                        "Security assessment complete — {} finding(s)",
                        state.artifacts.len()
                    ),
                    phases_run: state.phase + 1,
                    artifacts: state.artifacts,
                }))
            }
        }
    }

    fn name(&self) -> &'static str {
        "SecureStrategy"
    }

    fn estimated_step_cost_usd(&self) -> f64 {
        0.15
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
    async fn phase_0_pauses_for_scope_approval() {
        let s = SecureStrategy::new();
        let state = LoopState::new("gateway API");
        let outcome = s.step(state, &ctx()).await.unwrap();
        assert!(matches!(outcome, Outcome::Pause(ref st, _) if st.phase == 1));
    }

    #[tokio::test]
    async fn phase_1_continues() {
        let s = SecureStrategy::new();
        let mut state = LoopState::new("ctx");
        state.phase = 1;
        let outcome = s.step(state, &ctx()).await.unwrap();
        assert!(matches!(outcome, Outcome::Continue(ref st) if st.phase == 2));
    }

    #[tokio::test]
    async fn phase_2_halts_with_report() {
        let s = SecureStrategy::new();
        let mut state = LoopState::new("ctx");
        state.phase = 2;
        let outcome = s.step(state, &ctx()).await.unwrap();
        assert!(matches!(outcome, Outcome::Halt(_)));
    }
}
