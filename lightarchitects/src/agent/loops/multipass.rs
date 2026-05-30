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
/// Requires a [`MultiPassExecutor`] to perform each pass.
/// Phase 3 implements the full `step()` logic.
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
        state: MultiPassState,
        _ctx: &StepContext,
    ) -> Result<Outcome<MultiPassState, MultiPassOutput>, LoopError> {
        // Phase 3 implements N-pass execution and aggregation.
        let passes_run = state.pass_index;
        #[allow(clippy::cast_possible_truncation)]
        let passes_passed = state.pass_results.iter().filter(|&&p| p).count() as u32;
        Ok(Outcome::Halt(MultiPassOutput {
            passes_run,
            passes_passed,
            verdict: String::new(),
            notes: state.notes,
        }))
    }

    fn name(&self) -> &'static str {
        "multipass_verify"
    }
}
