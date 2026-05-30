//! `MultiPassVerifyStrategy` — N-pass independent verification loop.
//!
//! An L0 strategy that runs an arbitrary number of verification passes,
//! converging when all passes agree (or a configured majority is reached).
//!
//! L0 class: custom [`MultiPassState`] and [`MultiPassOutput`]; not registered
//! in `RegisteredStrategy`. Requires a [`MultiPassExecutor`] for each pass.
//!
//! Full phase logic implemented in Phase 3.

use async_trait::async_trait;

use super::{
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── State ─────────────────────────────────────────────────────────────────────

/// Mutable state threaded through each multi-pass verification step.
#[derive(Debug, Clone)]
pub struct MultiPassState {
    /// 0-based index of the next pass to execute.
    pub pass_index: u32,
    /// Maximum number of passes to run.
    pub max_passes: u32,
    /// Results of completed passes (`true` = passed).
    pub pass_results: Vec<bool>,
    /// Subject being verified (file path, PR ID, etc.).
    pub subject: String,
    /// Accumulated reviewer notes from all passes.
    pub notes: Vec<String>,
}

impl MultiPassState {
    /// Initialise for `max_passes` rounds over `subject`.
    #[must_use]
    pub fn new(subject: impl Into<String>, max_passes: u32) -> Self {
        Self {
            pass_index: 0,
            max_passes,
            pass_results: Vec::new(),
            subject: subject.into(),
            notes: Vec::new(),
        }
    }
}

// ── Output ────────────────────────────────────────────────────────────────────

/// Terminal output produced when `MultiPassVerifyStrategy` halts.
#[derive(Debug)]
pub struct MultiPassOutput {
    /// Total number of passes completed.
    pub passes_run: u32,
    /// How many passes returned a passing result.
    pub passes_passed: u32,
    /// Aggregated verdict from all passes.
    pub verdict: String,
    /// All reviewer notes collected across passes.
    pub notes: Vec<String>,
}

// ── Executor trait ────────────────────────────────────────────────────────────

/// Callback trait for a single verification pass.
#[async_trait]
pub trait MultiPassExecutor: Send + Sync {
    /// Execute the `n`th (0-based) verification pass against `subject`.
    ///
    /// Returns `true` if the pass succeeds, along with any reviewer notes.
    async fn verify_pass(
        &self,
        n: u32,
        subject: &str,
        ctx: &StepContext,
    ) -> Result<(bool, String), LoopError>;

    /// Aggregate all pass results into a human-readable verdict.
    async fn aggregate(
        &self,
        results: &[bool],
        notes: &[String],
        ctx: &StepContext,
    ) -> Result<String, LoopError>;
}

// ── Strategy ──────────────────────────────────────────────────────────────────

/// N-pass independent verification loop.
///
/// Requires a [`MultiPassExecutor`] to perform each pass. Each step runs one
/// pass via [`MultiPassExecutor::verify_pass`]. After all `max_passes` are
/// complete, calls [`MultiPassExecutor::aggregate`] and halts.
pub struct MultiPassVerifyStrategy<E: MultiPassExecutor> {
    /// Executor that performs individual verification passes.
    pub executor: E,
}

impl<E: MultiPassExecutor> MultiPassVerifyStrategy<E> {
    /// Construct a strategy with the given executor.
    pub fn new(executor: E) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E: MultiPassExecutor + 'static> Strategy for MultiPassVerifyStrategy<E> {
    type State = MultiPassState;
    type Output = MultiPassOutput;

    async fn step(
        &self,
        mut state: MultiPassState,
        ctx: &StepContext,
    ) -> Result<Outcome<MultiPassState, MultiPassOutput>, LoopError> {
        // All passes complete — aggregate and halt.
        if state.pass_index >= state.max_passes {
            let verdict = self
                .executor
                .aggregate(&state.pass_results, &state.notes, ctx)
                .await?;
            #[allow(clippy::cast_possible_truncation)]
            let passes_passed = state.pass_results.iter().filter(|&&p| p).count() as u32;
            return Ok(Outcome::Halt(MultiPassOutput {
                passes_run: state.pass_index,
                passes_passed,
                verdict,
                notes: state.notes,
            }));
        }

        // Execute next pass.
        let (passed, note) = self
            .executor
            .verify_pass(state.pass_index, &state.subject, ctx)
            .await?;

        state.pass_results.push(passed);
        if !note.is_empty() {
            state.notes.push(note);
        }
        state.pass_index += 1;

        Ok(Outcome::Continue(state))
    }

    fn name(&self) -> &'static str {
        "multipass_verify"
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

    /// Executor that always passes with a fixed note.
    struct AlwaysPass;

    #[async_trait]
    impl MultiPassExecutor for AlwaysPass {
        async fn verify_pass(
            &self,
            n: u32,
            _subject: &str,
            _ctx: &StepContext,
        ) -> Result<(bool, String), LoopError> {
            Ok((true, format!("pass {n} ok")))
        }

        async fn aggregate(
            &self,
            results: &[bool],
            _notes: &[String],
            _ctx: &StepContext,
        ) -> Result<String, LoopError> {
            let passed = results.iter().filter(|&&p| p).count();
            Ok(format!("PASS {passed}/{}", results.len()))
        }
    }

    /// Executor where pass 1 (0-based) always fails.
    struct FailSecondPass;

    #[async_trait]
    impl MultiPassExecutor for FailSecondPass {
        async fn verify_pass(
            &self,
            n: u32,
            _subject: &str,
            _ctx: &StepContext,
        ) -> Result<(bool, String), LoopError> {
            Ok((n != 1, String::new()))
        }

        async fn aggregate(
            &self,
            results: &[bool],
            _notes: &[String],
            _ctx: &StepContext,
        ) -> Result<String, LoopError> {
            let passed = results.iter().filter(|&&p| p).count();
            Ok(if passed == results.len() {
                "PASS".into()
            } else {
                "FAIL".into()
            })
        }
    }

    #[tokio::test]
    async fn three_pass_all_succeed() {
        let strategy = MultiPassVerifyStrategy::new(AlwaysPass);
        let mut state = MultiPassState::new("src/lib.rs", 3);

        // Steps 0, 1, 2 run passes; step 3 aggregates and halts.
        for _ in 0..=3 {
            match strategy.step(state.clone(), &ctx()).await.unwrap() {
                Outcome::Continue(s) => state = s,
                Outcome::Halt(out) => {
                    assert_eq!(out.passes_run, 3);
                    assert_eq!(out.passes_passed, 3);
                    assert!(out.verdict.contains("3/3"));
                    assert_eq!(out.notes.len(), 3);
                    return;
                }
                Outcome::Pause(..) => panic!("MultiPassVerify should not pause"),
            }
        }
        panic!("should have halted after 3 passes");
    }

    #[tokio::test]
    async fn one_failing_pass_reflected_in_verdict() {
        let strategy = MultiPassVerifyStrategy::new(FailSecondPass);
        let mut state = MultiPassState::new("lib.rs", 3);

        for _ in 0..=3 {
            match strategy.step(state.clone(), &ctx()).await.unwrap() {
                Outcome::Continue(s) => state = s,
                Outcome::Halt(out) => {
                    assert_eq!(out.passes_passed, 2);
                    assert_eq!(out.verdict, "FAIL");
                    return;
                }
                Outcome::Pause(..) => panic!(),
            }
        }
        panic!("should have halted");
    }

    #[tokio::test]
    async fn zero_passes_aggregates_immediately() {
        let strategy = MultiPassVerifyStrategy::new(AlwaysPass);
        let state = MultiPassState::new("lib.rs", 0);

        let outcome = strategy.step(state, &ctx()).await.unwrap();
        let out = match outcome {
            Outcome::Halt(o) => o,
            other => panic!("expected Halt, got {other:?}"),
        };
        assert_eq!(out.passes_run, 0);
        assert_eq!(out.passes_passed, 0);
    }
}
