//! Ensemble strategy — parallel multi-branch execution, SDK port of oracle/client.rs fan-out.
//!
//! Runs N instances of the same strategy concurrently (different configs / seeds) and
//! collects all outputs. Halts when every branch has halted.
//!
//! Equivalent to the `Parallel` combinator but for N homogeneous branches rather than 2.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::future;

use super::{
    compose::child_ctx,
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── State ─────────────────────────────────────────────────────────────────────

/// State for one step of the [`EnsembleStrategy`] loop.
///
/// `active[i]` is `Some(state)` while branch `i` is still running, `None` once
/// it has halted. `outputs[i]` holds the terminal output once branch `i` halts.
pub struct EnsembleState<S: Strategy>
where
    S::State: Clone,
    S::Output: Clone,
{
    /// Per-branch states (`None` = halted).
    pub active: Vec<Option<S::State>>,
    /// Per-branch terminal outputs (`None` = still running).
    pub outputs: Vec<Option<S::Output>>,
}

impl<S: Strategy> Clone for EnsembleState<S>
where
    S::State: Clone,
    S::Output: Clone,
{
    fn clone(&self) -> Self {
        Self {
            active: self.active.clone(),
            outputs: self.outputs.clone(),
        }
    }
}

impl<S: Strategy> std::fmt::Debug for EnsembleState<S>
where
    S::State: std::fmt::Debug + Clone,
    S::Output: std::fmt::Debug + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnsembleState")
            .field("active", &self.active)
            .field("outputs", &self.outputs)
            .finish()
    }
}

// ── Strategy ─────────────────────────────────────────────────────────────────

/// Ensemble strategy — N parallel branches of the same strategy type.
///
/// Create one branch per model / seed / config variant, then drive them all
/// with a single [`crate::agent::loops::LoopRunner`]. Outputs are collected in
/// branch order; `None` entries indicate branches that errored before halting.
pub struct EnsembleStrategy<S: Strategy>
where
    S::State: Clone,
    S::Output: Clone,
{
    branches: Vec<Arc<S>>,
    name: &'static str,
}

impl<S: Strategy> EnsembleStrategy<S>
where
    S::State: Clone,
    S::Output: Clone,
{
    /// Create an ensemble from a list of strategy instances.
    ///
    /// Panics in debug builds if `branches` is empty.
    #[must_use]
    pub fn new(branches: Vec<S>) -> Self {
        debug_assert!(
            !branches.is_empty(),
            "EnsembleStrategy requires at least one branch"
        );
        Self {
            branches: branches.into_iter().map(Arc::new).collect(),
            name: "Ensemble",
        }
    }

    /// Override the strategy name.
    #[must_use]
    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }

    /// Return the number of branches.
    #[must_use]
    pub fn branch_count(&self) -> usize {
        self.branches.len()
    }

    /// Build the initial [`EnsembleState`] from a list of per-branch initial states.
    ///
    /// Panics in debug builds if `initial_states.len() != self.branch_count()`.
    #[must_use]
    pub fn initial_state(&self, initial_states: Vec<S::State>) -> EnsembleState<S> {
        debug_assert_eq!(
            initial_states.len(),
            self.branches.len(),
            "initial_states must match branch_count"
        );
        let n = self.branches.len();
        EnsembleState {
            active: initial_states.into_iter().map(Some).collect(),
            outputs: vec![None; n],
        }
    }
}

#[async_trait]
impl<S: Strategy> Strategy for EnsembleStrategy<S>
where
    S::State: Clone,
    S::Output: Clone + Send + 'static,
{
    type State = EnsembleState<S>;
    type Output = Vec<Option<S::Output>>;

    async fn step(
        &self,
        state: EnsembleState<S>,
        ctx: &StepContext,
    ) -> Result<Outcome<EnsembleState<S>, Vec<Option<S::Output>>>, LoopError> {
        // All active branches share the same child chain depth (siblings, not nested).
        let branch_ctx = child_ctx(ctx)?;

        let mut new_active = state.active;
        let mut new_outputs = state.outputs;

        // Collect futures for all active branches.
        let futures: Vec<_> = new_active
            .iter()
            .enumerate()
            .filter_map(|(i, opt)| opt.as_ref().map(|s| (i, s.clone())))
            .map(|(i, branch_state)| {
                let branch = Arc::clone(&self.branches[i]);
                let bctx = branch_ctx.clone();
                async move { (i, branch.step(branch_state, &bctx).await) }
            })
            .collect();

        let results = future::join_all(futures).await;

        for (i, result) in results {
            match result? {
                Outcome::Continue(next) => {
                    new_active[i] = Some(next);
                }
                Outcome::Halt(out) => {
                    new_active[i] = None;
                    new_outputs[i] = Some(out);
                }
            }
        }

        let next_state = EnsembleState {
            active: new_active,
            outputs: new_outputs,
        };

        if next_state.active.iter().all(Option::is_none) {
            Ok(Outcome::Halt(next_state.outputs))
        } else {
            Ok(Outcome::Continue(next_state))
        }
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use futures_util::StreamExt as _;

    use crate::agent::{
        ChainContext,
        loops::{Budget, LoopRunner, Outcome, StepContext, Strategy, error::LoopError},
    };

    use super::*;

    /// Counts down from N and halts with the final count.
    struct CountDown(u32);

    #[async_trait::async_trait]
    impl Strategy for CountDown {
        type State = u32;
        type Output = u32;

        async fn step(&self, n: u32, _ctx: &StepContext) -> Result<Outcome<u32, u32>, LoopError> {
            if n == 0 {
                Ok(Outcome::Halt(self.0))
            } else {
                Ok(Outcome::Continue(n - 1))
            }
        }

        fn name(&self) -> &'static str {
            "CountDown"
        }
    }

    #[tokio::test]
    async fn ensemble_two_branches_both_halt() {
        let strategy = EnsembleStrategy::new(vec![CountDown(10), CountDown(20)]);
        let init = strategy.initial_state(vec![3u32, 2u32]);
        let runner = LoopRunner::new(strategy, Budget::unlimited());
        let mut stream = runner.run(init, ChainContext::default(), None);

        let mut outputs: Option<Vec<Option<u32>>> = None;
        while let Some(result) = stream.next().await {
            if let Outcome::Halt(out) = result.unwrap().outcome {
                outputs = Some(out);
            }
        }
        let out = outputs.unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], Some(10)); // branch 0 started at 3, halts with id=10
        assert_eq!(out[1], Some(20)); // branch 1 started at 2, halts with id=20
    }

    #[tokio::test]
    async fn ensemble_single_branch_halts() {
        let strategy = EnsembleStrategy::new(vec![CountDown(42)]);
        let init = strategy.initial_state(vec![1u32]);
        let runner = LoopRunner::new(strategy, Budget::unlimited());
        let mut stream = runner.run(init, ChainContext::default(), None);

        let mut halted = false;
        while let Some(result) = stream.next().await {
            if let Outcome::Halt(out) = result.unwrap().outcome {
                assert_eq!(out[0], Some(42));
                halted = true;
            }
        }
        assert!(halted);
    }

    #[test]
    fn ensemble_branch_count() {
        let e: EnsembleStrategy<CountDown> =
            EnsembleStrategy::new(vec![CountDown(1), CountDown(2), CountDown(3)]);
        assert_eq!(e.branch_count(), 3);
    }
}
