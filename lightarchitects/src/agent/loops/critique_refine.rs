//! `CritiqueRefineStrategy` — three-phase critique-and-refinement loop.
//!
//! Maps directly to QUANTUM's `theorize → verify → close` lifecycle:
//!
//! | Phase | QUANTUM action | SDK method |
//! |-------|---------------|------------|
//! | 1 | `theorize` | [`CritiquePhase::Critique`] |
//! | 2 | `verify` | [`CritiquePhase::Refine`] |
//! | 3 | `close` | [`CritiquePhase::Close`] |
//!
//! The strategy drives a provider to generate a draft, critique it, and refine
//! it until either the quality threshold is met or `max_rounds` is exhausted.
//!
//! # QUANTUM wiring
//!
//! `QUANTUM/MCP/QUANTUM-DEV/src/mcp.rs` delegates its `theorize`, `verify`,
//! and `close` actions to this strategy via the SDK (Wave 1-3).

use async_trait::async_trait;

use super::{
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── Phase state ───────────────────────────────────────────────────────────────

/// Which phase of the critique-refine loop we are in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CritiquePhase {
    /// Phase 1 — generate initial draft and critiques (maps to QUANTUM `theorize`).
    Critique,
    /// Phase 2 — apply critiques to produce a refined draft (maps to QUANTUM `verify`).
    Refine,
    /// Phase 3 — finalize and close the loop (maps to QUANTUM `close`).
    Close,
}

/// State threaded through each step of the critique-refine loop.
#[derive(Debug, Clone)]
pub struct CritiqueState {
    /// Current phase of the loop.
    pub phase: CritiquePhase,
    /// Working text being refined across steps.
    pub draft: String,
    /// Critique notes accumulated during the `Critique` phase.
    pub critiques: Vec<String>,
    /// Number of completed critique-refine cycles.
    pub rounds: u32,
}

impl CritiqueState {
    /// Initialise with an input prompt, starting in the `Critique` phase.
    #[must_use]
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            phase: CritiquePhase::Critique,
            draft: input.into(),
            critiques: Vec::new(),
            rounds: 0,
        }
    }
}

// ── CritiqueRefineStrategy ────────────────────────────────────────────────────

/// A self-improving loop that critiques its output and refines it iteratively.
///
/// The loop runs for at most `max_rounds` critique-refine cycles. When the
/// `Close` phase is reached the loop halts and returns the final draft.
///
/// # Provider-agnostic
///
/// `CritiqueRefineStrategy` accepts any [`CritiqueExecutor`] to decouple the
/// loop mechanics from the provider implementation. Tests inject a stub;
/// production code injects a real LLM executor.
pub struct CritiqueRefineStrategy<E> {
    executor: E,
    max_rounds: u32,
    name: &'static str,
}

impl<E: CritiqueExecutor> CritiqueRefineStrategy<E> {
    /// Create a strategy with the given executor and maximum rounds.
    #[must_use]
    pub fn new(executor: E, max_rounds: u32) -> Self {
        Self {
            executor,
            max_rounds,
            name: "CritiqueRefine",
        }
    }

    /// Override the strategy name (useful for composed strategies).
    #[must_use]
    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }
}

/// Executor interface for the three critique-refine phases.
///
/// Implementors provide the LLM logic for each phase. The loop mechanics
/// (phase transitions, round counting, halt conditions) are handled by
/// [`CritiqueRefineStrategy`].
#[async_trait]
pub trait CritiqueExecutor: Send + Sync + 'static {
    /// Phase 1: generate critiques for the current draft.
    ///
    /// Returns a list of critique notes to be applied in the Refine phase.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn theorize(&self, draft: &str, ctx: &StepContext) -> Result<Vec<String>, LoopError>;

    /// Phase 2: apply critiques and return a refined draft.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn verify(
        &self,
        draft: &str,
        critiques: &[String],
        ctx: &StepContext,
    ) -> Result<String, LoopError>;

    /// Phase 3: finalize the draft and return the closed output.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on provider or domain failures.
    async fn close(&self, draft: &str, ctx: &StepContext) -> Result<String, LoopError>;
}

#[async_trait]
impl<E: CritiqueExecutor> Strategy for CritiqueRefineStrategy<E> {
    type State = CritiqueState;
    type Output = String;

    async fn step(
        &self,
        state: CritiqueState,
        ctx: &StepContext,
    ) -> Result<Outcome<CritiqueState, String>, LoopError> {
        match state.phase {
            CritiquePhase::Critique => {
                let critiques = self.executor.theorize(&state.draft, ctx).await?;
                Ok(Outcome::Continue(CritiqueState {
                    phase: CritiquePhase::Refine,
                    critiques,
                    ..state
                }))
            }
            CritiquePhase::Refine => {
                let refined = self
                    .executor
                    .verify(&state.draft, &state.critiques, ctx)
                    .await?;
                let rounds = state.rounds + 1;
                let next_phase = if rounds >= self.max_rounds {
                    CritiquePhase::Close
                } else {
                    CritiquePhase::Critique
                };
                Ok(Outcome::Continue(CritiqueState {
                    phase: next_phase,
                    draft: refined,
                    critiques: Vec::new(),
                    rounds,
                }))
            }
            CritiquePhase::Close => {
                let final_output = self.executor.close(&state.draft, ctx).await?;
                Ok(Outcome::Halt(final_output))
            }
        }
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::items_after_statements)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use futures_util::StreamExt as _;

    use crate::agent::{ChainContext, loops::budget::Budget};

    use super::*;

    /// Stub executor: theorize appends "[critique]", verify appends "[refined]",
    /// close appends "[closed]".
    struct StubExecutor;

    #[async_trait]
    impl CritiqueExecutor for StubExecutor {
        async fn theorize(
            &self,
            draft: &str,
            _ctx: &StepContext,
        ) -> Result<Vec<String>, LoopError> {
            Ok(vec![format!("critique of: {draft}")])
        }
        async fn verify(
            &self,
            draft: &str,
            critiques: &[String],
            _ctx: &StepContext,
        ) -> Result<String, LoopError> {
            Ok(format!("{draft} [refined with {} notes]", critiques.len()))
        }
        async fn close(&self, draft: &str, _ctx: &StepContext) -> Result<String, LoopError> {
            Ok(format!("{draft} [closed]"))
        }
    }

    #[tokio::test]
    async fn single_round_produces_closed_output() {
        let strategy = CritiqueRefineStrategy::new(StubExecutor, 1);
        let runner = super::super::runner::LoopRunner::new(strategy, Budget::unlimited());
        let mut stream = runner.run(CritiqueState::new("hello"), ChainContext::default(), None);

        let mut outputs = vec![];
        while let Some(result) = stream.next().await {
            let step = result.unwrap();
            if let Outcome::Halt(ref out) = step.outcome {
                outputs.push(out.clone());
            }
        }
        assert_eq!(outputs.len(), 1);
        assert!(outputs[0].contains("[closed]"), "got: {}", outputs[0]);
    }

    #[tokio::test]
    async fn multi_round_increments_rounds() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        struct CountingExecutor(Arc<AtomicU32>);

        #[async_trait]
        impl CritiqueExecutor for CountingExecutor {
            async fn theorize(&self, _: &str, _: &StepContext) -> Result<Vec<String>, LoopError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(vec!["c".into()])
            }
            async fn verify(
                &self,
                d: &str,
                _: &[String],
                _: &StepContext,
            ) -> Result<String, LoopError> {
                Ok(d.to_owned())
            }
            async fn close(&self, d: &str, _: &StepContext) -> Result<String, LoopError> {
                Ok(d.to_owned())
            }
        }

        let strategy = CritiqueRefineStrategy::new(CountingExecutor(counter_clone), 3);
        let runner = super::super::runner::LoopRunner::new(strategy, Budget::unlimited());
        let mut stream = runner.run(CritiqueState::new("x"), ChainContext::default(), None);

        let mut final_output = None;
        while let Some(result) = stream.next().await {
            let step = result.unwrap();
            if let Outcome::Halt(out) = step.outcome {
                final_output = Some(out);
                break;
            }
        }

        assert!(final_output.is_some());
        // 3 rounds = 3 critique calls
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn budget_halt_stops_loop() {
        let strategy = CritiqueRefineStrategy::new(StubExecutor, 100);
        let runner = super::super::runner::LoopRunner::new(strategy, Budget::new(2, f64::MAX));
        let mut stream = runner.run(CritiqueState::new("x"), ChainContext::default(), None);

        let mut count = 0u32;
        let mut saw_error = false;
        while let Some(result) = stream.next().await {
            match result {
                Ok(_) => count += 1,
                Err(LoopError::BudgetExceeded { .. }) => {
                    saw_error = true;
                    break;
                }
                Err(e) => panic!("unexpected error: {e}"),
            }
        }
        assert!(saw_error, "expected budget error after {count} steps");
        assert!(count <= 2);
    }

    #[tokio::test]
    async fn chain_depth_exceeded_stops_loop() {
        // Start at depth 6; child() will hit depth 7 on first step, which is
        // still within MAX_CHAIN_DEPTH=7, so second child() hits 8 → error.
        let ctx = ChainContext {
            depth: 6,
            origin: None,
            aud: None,
        };
        let strategy = CritiqueRefineStrategy::new(StubExecutor, 100);
        let runner = super::super::runner::LoopRunner::new(strategy, Budget::unlimited());
        let mut stream = runner.run(CritiqueState::new("x"), ctx, None);

        let mut saw_depth_err = false;
        while let Some(result) = stream.next().await {
            if let Err(LoopError::ChainDepthExceeded { .. }) = result {
                saw_depth_err = true;
                break;
            }
        }
        assert!(saw_depth_err, "expected chain depth error");
    }

    #[test]
    fn phase_transitions_are_correct() {
        let s = CritiqueState::new("x");
        assert_eq!(s.phase, CritiquePhase::Critique);
        assert_eq!(s.rounds, 0);
    }
}
