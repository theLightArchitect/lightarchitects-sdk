//! `DrainStrategy` — bounded queue-drain processing loop.
//!
//! An L0 strategy that processes items from a queue until exhausted or budget
//! is reached. Uses a topology-based (not fixed-phase) iteration model —
//! convergence is reached when the queue is empty.
//!
//! L0 class: custom [`DrainState`] and [`DrainOutput`]; not registered in
//! `RegisteredStrategy`. Requires a [`DrainExecutor`] for item processing.
//!
//! ## Step topology
//!
//! Each step call:
//! 1. Asks the executor for the next item via [`DrainExecutor::next_item`].
//! 2. Calls [`DrainExecutor::process`] on that item.
//! 3. Moves the item to `processed` (success) or `failed` (failure) and pops it from the queue.
//! 4. Calls [`DrainExecutor::is_empty`] — if `true`, halts; otherwise returns `Continue`.

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
/// Requires a [`DrainExecutor`] for item processing. Each step pops one item
/// from the queue, processes it, and checks the drain convergence condition.
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
        mut state: DrainState,
        ctx: &StepContext,
    ) -> Result<Outcome<DrainState, DrainOutput>, LoopError> {
        // Topology: next_item → process → convergence check.

        let Some(item) = self.executor.next_item(&state.queue, ctx).await? else {
            // Executor signalled logical exhaustion even though queue may be non-empty.
            return Ok(Outcome::Halt(DrainOutput {
                processed_count: state.processed.len(),
                failed_count: state.failed.len(),
                drain_fraction: state.drain_fraction(),
                remaining: state.queue,
            }));
        };

        // Remove the item from the queue (front-of-queue pop by matching value).
        if let Some(pos) = state.queue.iter().position(|q| q == &item) {
            state.queue.remove(pos);
        }

        let succeeded = self.executor.process(&item, ctx).await?;
        if succeeded {
            state.processed.push(item);
        } else {
            state.failed.push(item);
        }

        // Check convergence after processing.
        if self.executor.is_empty(&state, ctx).await? {
            return Ok(Outcome::Halt(DrainOutput {
                processed_count: state.processed.len(),
                failed_count: state.failed.len(),
                drain_fraction: state.drain_fraction(),
                remaining: state.queue,
            }));
        }

        Ok(Outcome::Continue(state))
    }

    fn name(&self) -> &'static str {
        "drain"
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

    /// Simple executor: processes all items successfully, drains by queue emptiness.
    struct PassAllExecutor;

    #[async_trait]
    impl DrainExecutor for PassAllExecutor {
        async fn next_item(
            &self,
            queue: &[String],
            _ctx: &StepContext,
        ) -> Result<Option<String>, LoopError> {
            Ok(queue.first().cloned())
        }

        async fn process(&self, _item: &str, _ctx: &StepContext) -> Result<bool, LoopError> {
            Ok(true)
        }

        async fn is_empty(
            &self,
            state: &DrainState,
            _ctx: &StepContext,
        ) -> Result<bool, LoopError> {
            Ok(state.queue.is_empty())
        }
    }

    /// Executor that always fails items.
    struct FailAllExecutor;

    #[async_trait]
    impl DrainExecutor for FailAllExecutor {
        async fn next_item(
            &self,
            queue: &[String],
            _ctx: &StepContext,
        ) -> Result<Option<String>, LoopError> {
            Ok(queue.first().cloned())
        }

        async fn process(&self, _item: &str, _ctx: &StepContext) -> Result<bool, LoopError> {
            Ok(false)
        }

        async fn is_empty(
            &self,
            state: &DrainState,
            _ctx: &StepContext,
        ) -> Result<bool, LoopError> {
            Ok(state.queue.is_empty())
        }
    }

    #[tokio::test]
    async fn drains_all_items_successfully() {
        let strategy = DrainStrategy::new(PassAllExecutor);
        let mut state = DrainState::new(vec!["a".into(), "b".into(), "c".into()]);

        for _ in 0..3 {
            state = match strategy.step(state, &ctx()).await.unwrap() {
                Outcome::Continue(s) => s,
                Outcome::Halt(out) => {
                    assert_eq!(out.processed_count, 3);
                    assert_eq!(out.failed_count, 0);
                    assert!((out.drain_fraction - 1.0).abs() < f64::EPSILON);
                    assert!(out.remaining.is_empty());
                    return;
                }
                Outcome::Pause(..) => panic!("DrainStrategy should not pause"),
            };
        }
        panic!("should have halted after draining 3 items");
    }

    #[tokio::test]
    async fn failed_items_go_to_failed_vec() {
        let strategy = DrainStrategy::new(FailAllExecutor);
        let mut state = DrainState::new(vec!["x".into()]);

        let outcome = strategy.step(state.clone(), &ctx()).await.unwrap();
        state = match outcome {
            Outcome::Halt(out) => {
                assert_eq!(out.failed_count, 1);
                assert_eq!(out.processed_count, 0);
                return;
            }
            Outcome::Continue(s) => s,
            Outcome::Pause(..) => panic!(),
        };
        // If single item didn't halt, it means queue has more items.
        let _ = state;
    }

    #[test]
    fn drain_fraction_empty_queue_is_one() {
        let state = DrainState::new(vec![]);
        assert!((state.drain_fraction() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn drain_fraction_partial() {
        let mut state = DrainState::new(vec!["a".into(), "b".into()]);
        state.processed.push("a".into());
        let frac = state.drain_fraction();
        assert!((frac - 0.5).abs() < f64::EPSILON);
    }
}
