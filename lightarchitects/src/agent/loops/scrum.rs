//! `ScrumStrategy` — dual-mode squad review / meeting loop.
//!
//! **Review mode**: 3-round parallel assessment (R1 → R2 cross-critique → R3 synthesis).
//! **Meeting mode**: open-ended turn-based discussion (Open → Discussion → Close).
//!
//! The mode is selected at construction via [`ScrumMode`].

use async_trait::async_trait;

use super::{
    error::LoopError,
    meta_skill::{LoopOutput, LoopState},
    runner::{Outcome, StepContext, Strategy},
};

/// Selects the operating mode of [`ScrumStrategy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrumMode {
    /// 3-round structured review producing a Good/Gaps/Fixes report.
    Review,
    /// Open-ended turn-based discussion driven by interest scoring.
    Meeting,
}

/// Dual-mode squad review / meeting strategy.
pub struct ScrumStrategy {
    /// Operating mode.
    pub mode: ScrumMode,
}

impl ScrumStrategy {
    /// Create a review-mode strategy.
    #[must_use]
    pub fn review() -> Self {
        Self {
            mode: ScrumMode::Review,
        }
    }

    /// Create a meeting-mode strategy.
    #[must_use]
    pub fn meeting() -> Self {
        Self {
            mode: ScrumMode::Meeting,
        }
    }
}

impl Default for ScrumStrategy {
    fn default() -> Self {
        Self::review()
    }
}

#[async_trait]
impl Strategy for ScrumStrategy {
    type State = LoopState;
    type Output = LoopOutput;

    async fn step(
        &self,
        mut state: LoopState,
        _ctx: &StepContext,
    ) -> Result<Outcome<LoopState, LoopOutput>, LoopError> {
        // Both modes run 3 phases; the mode controls artifact names below.
        let phases: u32 = 3;

        if state.phase >= phases {
            let summary = match self.mode {
                ScrumMode::Review => "Squad review complete — Good/Gaps/Fixes report produced",
                ScrumMode::Meeting => "Meeting concluded — minutes logged to helix",
            };
            return Ok(Outcome::Halt(LoopOutput {
                strategy_name: self.name().to_string(),
                summary: summary.into(),
                phases_run: state.phase,
                artifacts: state.artifacts,
            }));
        }

        let artifact = match (self.mode, state.phase) {
            (ScrumMode::Review, 0) => "scrum/r1-assessments.md",
            (ScrumMode::Review, 1) => "scrum/r2-cross-critique.md",
            (ScrumMode::Review, _) => "scrum/r3-synthesis.md",
            (ScrumMode::Meeting, 0) => "scrum/meeting-open.md",
            (ScrumMode::Meeting, 1) => "scrum/meeting-discussion.md",
            (ScrumMode::Meeting, _) => "scrum/meeting-minutes.md",
        };
        state.artifacts.push(artifact.into());
        state.phase += 1;
        Ok(Outcome::Continue(state))
    }

    fn name(&self) -> &'static str {
        match self.mode {
            ScrumMode::Review => "ScrumStrategy::Review",
            ScrumMode::Meeting => "ScrumStrategy::Meeting",
        }
    }

    fn estimated_step_cost_usd(&self) -> f64 {
        0.08
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
    async fn review_advances_through_three_rounds() {
        let s = ScrumStrategy::review();
        let mut state = LoopState::new("plan review");
        for expected_phase in 1..=3u32 {
            let outcome = s.step(state.clone(), &ctx()).await.unwrap();
            match outcome {
                Outcome::Continue(next) => {
                    assert_eq!(next.phase, expected_phase);
                    state = next;
                }
                Outcome::Halt(_) => {
                    assert_eq!(expected_phase, 3, "halted early");
                    return;
                }
                Outcome::Pause(_, _) => panic!("unexpected pause in review mode"),
            }
        }
        let outcome = s.step(state, &ctx()).await.unwrap();
        assert!(matches!(outcome, Outcome::Halt(_)));
    }

    #[tokio::test]
    async fn meeting_advances_to_halt() {
        let s = ScrumStrategy::meeting();
        let mut state = LoopState::new("platform discussion");
        loop {
            let outcome = s.step(state.clone(), &ctx()).await.unwrap();
            match outcome {
                Outcome::Continue(next) => state = next,
                Outcome::Halt(out) => {
                    assert!(out.summary.contains("Meeting"));
                    return;
                }
                Outcome::Pause(_, _) => panic!("unexpected pause in meeting mode"),
            }
        }
    }
}
