//! `DrainStrategy` — bounded queue-drain processing loop.
//!
//! An L0 strategy that processes items from a queue until exhausted or budget
//! is reached. Uses a topology-based (not fixed-phase) iteration model —
//! convergence is reached when the queue is empty.
//!
//! L0 class: custom [`DrainState`] and [`DrainOutput`]; not registered in
//! `RegisteredStrategy`. Requires a [`DrainExecutor`] for item processing.
//!
//! Full step logic implemented in Phase 3.

use async_trait::async_trait;

use super::{
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── State ─────────────────────────────────────────────────────────────────────

/// Mutable state threaded through each drain step.
#[derive(Debug, Clone)]
pub struct DrainState {
    /// Items remaining in the queue.
    pub queue: Vec<String>,
    /// Initial queue size (used for convergence percentage calculation).
    pub initial_size: usize,
    /// Items successfully processed.
    pub processed: Vec<String>,
    /// Items that failed processing (not re-queued by default).
    pub failed: Vec<String>,
}

impl DrainState {
    /// Initialise with a non-empty queue.
    #[must_use]
    pub fn new(queue: Vec<String>) -> Self {
        let initial_size = queue.len();
        Self {
            queue,
            initial_size,
            processed: Vec::new(),
            failed: Vec::new(),
        }
    }

    /// Returns `true` when the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Fraction of items drained `[0.0, 1.0]`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn drain_fraction(&self) -> f64 {
        if self.initial_size == 0 {
            return 1.0;
        }
        self.processed.len() as f64 / self.initial_size as f64
    }
}

// ── Output ────────────────────────────────────────────────────────────────────

/// Terminal output produced when `DrainStrategy` halts.
#[derive(Debug)]
pub struct DrainOutput {
    /// Number of items successfully processed.
    pub processed_count: usize,
    /// Number of items that failed.
    pub failed_count: usize,
    /// Fraction drained `[0.0, 1.0]`.
    pub drain_fraction: f64,
    /// Items still remaining (non-empty if budget exhausted before full drain).
    pub remaining: Vec<String>,
}

// ── Executor trait ────────────────────────────────────────────────────────────

/// Callback trait for queue item processing.
#[async_trait]
pub trait DrainExecutor: Send + Sync {
    /// Fetch the next item to process from the queue front.
    ///
    /// Returns `None` if the queue is logically empty (even if `state.queue`
    /// is non-empty due to filtering).
    async fn next_item(
        &self,
        queue: &[String],
        ctx: &StepContext,
    ) -> Result<Option<String>, LoopError>;

    /// Process a single queue item.
    ///
    /// Returns `true` on success, `false` on failure (item goes to `failed`).
    async fn process(&self, item: &str, ctx: &StepContext) -> Result<bool, LoopError>;

    /// Check whether the queue should be considered exhausted.
    ///
    /// Called after each `process()` call. Default convergence is `queue.is_empty()`;
    /// implementors may override to use a partial-drain threshold.
    async fn is_empty(&self, state: &DrainState, ctx: &StepContext) -> Result<bool, LoopError>;
}

// ── Strategy ──────────────────────────────────────────────────────────────────

/// Bounded queue-drain processing loop.
///
/// Requires a [`DrainExecutor`] for item processing.
/// Phase 3 implements the full topology-based `step()` logic.
pub struct DrainStrategy<E: DrainExecutor> {
    /// Executor responsible for item processing.
    pub executor: E,
}

impl<E: DrainExecutor> DrainStrategy<E> {
    /// Construct a strategy with the given executor.
    pub fn new(executor: E) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E: DrainExecutor + 'static> Strategy for DrainStrategy<E> {
    type State = DrainState;
    type Output = DrainOutput;

    async fn step(
        &self,
        state: DrainState,
        _ctx: &StepContext,
    ) -> Result<Outcome<DrainState, DrainOutput>, LoopError> {
        // Phase 3 implements queue-drain topology: next_item → process → convergence check.
        let drain_fraction = state.drain_fraction();
        let remaining = state.queue.clone();
        Ok(Outcome::Halt(DrainOutput {
            processed_count: state.processed.len(),
            failed_count: state.failed.len(),
            drain_fraction,
            remaining,
        }))
    }

    fn name(&self) -> &'static str {
        "drain"
    }
}
