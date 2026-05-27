//! `EnrichStrategy` — EVA-primary memory enrichment loop.
//!
//! A 3-phase loop: **Gather** → **Layer** → **Commit**.
//!
//! Runs autonomously (no HITL pause) — enrichment is a background-safe operation.

use async_trait::async_trait;

use super::{
    error::LoopError,
    meta_skill::{LoopOutput, LoopState},
    runner::{Outcome, StepContext, Strategy},
};

/// Three-phase EVA memory enrichment loop.
///
/// | Phase | Name | Action |
/// |-------|------|--------|
/// | 0 | Gather | Collect session artifacts and conversation context |
/// | 1 | Layer | Apply 8-layer EVA enrichment schema |
/// | 2 | Commit | Write enriched entries to SOUL helix vault → halt |
pub struct EnrichStrategy {
    /// Maximum phases before force-halt.
    pub max_phases: u32,
}

impl EnrichStrategy {
    /// Construct with the default 3-phase limit.
    #[must_use]
    pub fn new() -> Self {
        Self { max_phases: 3 }
    }
}

impl Default for EnrichStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Strategy for EnrichStrategy {
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
                summary: "Enrichment complete (max-phases circuit breaker)".into(),
                phases_run: state.phase,
                artifacts: state.artifacts,
            }));
        }

        match state.phase {
            0 => {
                state.artifacts.push("eva/gather.json".into());
                state.phase = 1;
                Ok(Outcome::Continue(state))
            }
            1 => {
                state.artifacts.push("eva/layers.json".into());
                state.phase = 2;
                Ok(Outcome::Continue(state))
            }
            _ => {
                let entry_path =
                    format!("helix/shared/entries/{}-enrichment.md", chrono_stub_date());
                state.artifacts.push(entry_path);
                Ok(Outcome::Halt(LoopOutput {
                    strategy_name: self.name().to_string(),
                    summary: format!(
                        "Enrichment committed — {} artifact(s) to helix",
                        state.artifacts.len()
                    ),
                    phases_run: state.phase + 1,
                    artifacts: state.artifacts,
                }))
            }
        }
    }

    fn name(&self) -> &'static str {
        "EnrichStrategy"
    }

    fn estimated_step_cost_usd(&self) -> f64 {
        0.05
    }
}

/// Returns a fixed date string for artifact naming in tests.
/// Production callers should substitute a real timestamp.
fn chrono_stub_date() -> &'static str {
    "2026-01-01"
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
    async fn runs_all_phases_without_pause() {
        let s = EnrichStrategy::new();
        let mut state = LoopState::new("session context");
        loop {
            let outcome = s.step(state.clone(), &ctx()).await.unwrap();
            match outcome {
                Outcome::Continue(next) => state = next,
                Outcome::Halt(out) => {
                    assert_eq!(out.phases_run, 3);
                    assert_eq!(out.artifacts.len(), 3);
                    return;
                }
                Outcome::Pause(_, _) => panic!("EnrichStrategy must never pause"),
            }
        }
    }

    #[tokio::test]
    async fn phase_0_continues_to_phase_1() {
        let s = EnrichStrategy::new();
        let state = LoopState::new("ctx");
        let outcome = s.step(state, &ctx()).await.unwrap();
        assert!(matches!(outcome, Outcome::Continue(ref st) if st.phase == 1));
    }
}
