//! Strategy composition combinators — `Then`, `Parallel`, `Layered`.
//!
//! Every combinator boundary calls [`ChainContext::child()`], enforcing the
//! Canon §2.6 chain depth ≤ 7 invariant. Composition depth adds up:
//!
//! ```text
//! depth 0   operator call
//! depth 1   LoopRunner::run increments
//! depth 2   Parallel::step increments for each branch
//! depth 3   Then::step increments
//! depth 4   inner Strategy::step
//! ```
//!
//! The plan instrumented test (`tests/chain_depth_compose.rs`) verifies that
//! `ReAct.then(CoVe).parallel(EnsembleStrategy)` inside a hooked
//! `ConversationSession` inside a `WorkerPool` stays at depth ≤ 7.

use async_trait::async_trait;

use super::{
    error::LoopError,
    runner::{Outcome, StepContext, Strategy},
};

// ── Then ──────────────────────────────────────────────────────────────────────

/// State for a [`Then`] combinator — either in the first strategy or the second.
pub enum ThenState<A: Strategy, B: Strategy> {
    /// First strategy has not yet halted.
    First(A::State),
    /// First halted; second is now running with converted state.
    Second(B::State),
}

impl<A: Strategy, B: Strategy> Clone for ThenState<A, B>
where
    A::State: Clone,
    B::State: Clone,
{
    fn clone(&self) -> Self {
        match self {
            ThenState::First(s) => ThenState::First(s.clone()),
            ThenState::Second(s) => ThenState::Second(s.clone()),
        }
    }
}

impl<A: Strategy, B: Strategy> std::fmt::Debug for ThenState<A, B>
where
    A::State: std::fmt::Debug,
    B::State: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThenState::First(s) => f.debug_tuple("First").field(s).finish(),
            ThenState::Second(s) => f.debug_tuple("Second").field(s).finish(),
        }
    }
}

/// Run strategy `A` to completion, then run strategy `B` with the output.
///
/// Requires `A::Output: Into<B::State>` so the handoff is type-safe.
/// Propagates a [`ChainContext::child()`] to the active sub-strategy at each
/// step, incrementing chain depth by 1.
pub struct Then<A, B> {
    first: A,
    second: B,
}

impl<A, B> Then<A, B>
where
    A: Strategy,
    B: Strategy,
    A::Output: Into<B::State>,
{
    /// Compose `first` followed by `second`.
    #[must_use]
    pub fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
}

#[async_trait]
impl<A, B> Strategy for Then<A, B>
where
    A: Strategy,
    B: Strategy,
    A::Output: Into<B::State>,
    A::State: Send + Clone + 'static,
    B::State: Send + Clone + 'static,
    B::Output: Send + 'static,
{
    type State = ThenState<A, B>;
    type Output = B::Output;

    async fn step(
        &self,
        state: ThenState<A, B>,
        ctx: &StepContext,
    ) -> Result<Outcome<ThenState<A, B>, B::Output>, LoopError> {
        // Increment chain depth at this combinator boundary (Canon §2.6).
        let child_chain = ctx
            .chain
            .child()
            .map_err(|_| LoopError::ChainDepthExceeded {
                depth: ctx.chain.depth,
            })?;
        let child_ctx = StepContext {
            chain: child_chain,
            ..ctx.clone()
        };

        match state {
            ThenState::First(a_state) => match self.first.step(a_state, &child_ctx).await? {
                Outcome::Continue(a_next) => Ok(Outcome::Continue(ThenState::First(a_next))),
                Outcome::Pause(a_next, req) => Ok(Outcome::Pause(ThenState::First(a_next), req)),
                Outcome::Halt(a_out) => {
                    // Transition: A's output becomes B's initial state.
                    Ok(Outcome::Continue(ThenState::Second(a_out.into())))
                }
            },
            ThenState::Second(b_state) => match self.second.step(b_state, &child_ctx).await? {
                Outcome::Continue(b_next) => Ok(Outcome::Continue(ThenState::Second(b_next))),
                Outcome::Pause(b_next, req) => Ok(Outcome::Pause(ThenState::Second(b_next), req)),
                Outcome::Halt(b_out) => Ok(Outcome::Halt(b_out)),
            },
        }
    }

    fn name(&self) -> &'static str {
        "Then"
    }
}

// ── Parallel ──────────────────────────────────────────────────────────────────

/// Run strategy `A` and strategy `B` concurrently on independent sub-states.
///
/// Both strategies run on every step via [`tokio::join!`]. Both must halt in
/// the same step for [`Outcome::Halt`] to be returned. If the halts are
/// mismatched (one continues while the other halts), the step returns
/// [`LoopError::StepFailed`] — callers that need asymmetric completion should
/// use [`Layered`] with custom orchestration.
///
/// Each branch receives a separate [`ChainContext::child()`] so depth is
/// incremented per branch, not globally. This models two independent chains
/// rather than one deeper chain.
pub struct Parallel<A, B> {
    left: A,
    right: B,
}

impl<A: Strategy, B: Strategy> Parallel<A, B> {
    /// Compose `left` and `right` as concurrent independent strategies.
    #[must_use]
    pub fn new(left: A, right: B) -> Self {
        Self { left, right }
    }
}

#[async_trait]
impl<A, B> Strategy for Parallel<A, B>
where
    A: Strategy,
    B: Strategy,
{
    /// Independent sub-states; each branch advances separately.
    type State = (A::State, B::State);
    /// Paired outputs — both strategies must halt to produce a result.
    type Output = (A::Output, B::Output);

    async fn step(
        &self,
        (a_state, b_state): (A::State, B::State),
        ctx: &StepContext,
    ) -> Result<Outcome<(A::State, B::State), (A::Output, B::Output)>, LoopError> {
        // Each branch gets its own child context (Canon §2.6 — parallel chains).
        let left_chain = ctx
            .chain
            .child()
            .map_err(|_| LoopError::ChainDepthExceeded {
                depth: ctx.chain.depth,
            })?;
        let right_chain = ctx
            .chain
            .child()
            .map_err(|_| LoopError::ChainDepthExceeded {
                depth: ctx.chain.depth,
            })?;

        let left_ctx = StepContext {
            chain: left_chain,
            ..ctx.clone()
        };
        let right_ctx = StepContext {
            chain: right_chain,
            ..ctx.clone()
        };

        let (a_result, b_result) = tokio::join!(
            self.left.step(a_state, &left_ctx),
            self.right.step(b_state, &right_ctx)
        );

        match (a_result?, b_result?) {
            (Outcome::Continue(a_next), Outcome::Continue(b_next)) => {
                Ok(Outcome::Continue((a_next, b_next)))
            }
            (Outcome::Halt(a_out), Outcome::Halt(b_out)) => Ok(Outcome::Halt((a_out, b_out))),
            _ => Err(LoopError::StepFailed(
                "parallel branches halted asymmetrically; use Layered for custom completion semantics".into(),
            )),
        }
    }

    fn name(&self) -> &'static str {
        "Parallel"
    }
}

// ── Layered ───────────────────────────────────────────────────────────────────

/// Type-erased strategy wrapper — erase concrete `State`/`Output` types behind
/// `dyn Strategy<State = S, Output = O>`.
///
/// Useful when composing strategies of the same input/output shape but
/// different concrete types. Build via [`Layered::new`]; use like any other
/// [`Strategy`].
pub struct Layered<State, Output> {
    inner: Box<dyn Strategy<State = State, Output = Output>>,
}

impl<S, O> Layered<S, O>
where
    S: Send + Clone + 'static,
    O: Send + 'static,
{
    /// Wrap any strategy whose `State` and `Output` match `S` and `O`.
    pub fn new<St>(strategy: St) -> Self
    where
        St: Strategy<State = S, Output = O>,
    {
        Self {
            inner: Box::new(strategy),
        }
    }
}

#[async_trait]
impl<S, O> Strategy for Layered<S, O>
where
    S: Send + Clone + 'static,
    O: Send + 'static,
{
    type State = S;
    type Output = O;

    async fn step(&self, state: S, ctx: &StepContext) -> Result<Outcome<S, O>, LoopError> {
        self.inner.step(state, ctx).await
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
}

// ── Extension trait ───────────────────────────────────────────────────────────

/// Fluent combinator methods on any [`Strategy`].
pub trait StrategyExt: Strategy + Sized {
    /// Chain `self` followed by `next` (requires `Self::Output: Into<Next::State>`).
    fn then<Next>(self, next: Next) -> Then<Self, Next>
    where
        Next: Strategy,
        Self::Output: Into<Next::State>,
    {
        Then::new(self, next)
    }

    /// Run `self` and `other` concurrently, combining their outputs.
    fn parallel<Other: Strategy>(self, other: Other) -> Parallel<Self, Other> {
        Parallel::new(self, other)
    }

    /// Erase concrete types behind `Layered<State, Output>`.
    fn layered(self) -> Layered<Self::State, Self::Output>
    where
        Self::State: Clone,
    {
        Layered::new(self)
    }
}

impl<S: Strategy + Sized> StrategyExt for S {}

// ── depth invariant helper ────────────────────────────────────────────────────

/// Verify the depth invariant at a combinator boundary.
///
/// Combinators call this instead of `chain.child()` directly so the assertion
/// is centralised and testable. Returns the child context if depth is within
/// bounds, or `LoopError::ChainDepthExceeded` otherwise.
///
/// # Errors
///
/// Returns [`LoopError::ChainDepthExceeded`] if `chain.depth` has already
/// reached [`MAX_CHAIN_DEPTH`].
#[inline]
pub fn child_ctx(ctx: &StepContext) -> Result<StepContext, LoopError> {
    let child_chain = ctx
        .chain
        .child()
        .map_err(|_| LoopError::ChainDepthExceeded {
            depth: ctx.chain.depth,
        })?;
    Ok(StepContext {
        chain: child_chain,
        ..ctx.clone()
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use futures_util::StreamExt as _;

    use crate::agent::{
        ChainContext, MAX_CHAIN_DEPTH, loops::budget::Budget, loops::runner::LoopRunner,
    };

    use super::*;

    // Stub strategy: halts immediately with a constant value.
    struct Const<S: Send + Sync + Clone + 'static, O: Send + Sync + 'static> {
        output: O,
        name: &'static str,
        _state: std::marker::PhantomData<S>,
    }

    impl<S, O> Const<S, O>
    where
        S: Send + Sync + Clone + 'static,
        O: Send + Sync + Clone + 'static,
    {
        fn new(output: O, name: &'static str) -> Self {
            Self {
                output,
                name,
                _state: std::marker::PhantomData,
            }
        }
    }

    #[async_trait::async_trait]
    impl<S, O> Strategy for Const<S, O>
    where
        S: Send + Sync + Clone + 'static,
        O: Send + Sync + Clone + 'static,
    {
        type State = S;
        type Output = O;
        async fn step(&self, _: S, _: &StepContext) -> Result<Outcome<S, O>, LoopError> {
            Ok(Outcome::Halt(self.output.clone()))
        }
        fn name(&self) -> &'static str {
            self.name
        }
    }

    #[tokio::test]
    async fn then_sequences_strategies() {
        // A halts with u32 42; B converts u32 → String state and halts with "42".
        struct StringStrategy;
        #[async_trait::async_trait]
        impl Strategy for StringStrategy {
            type State = u32;
            type Output = String;
            async fn step(
                &self,
                n: u32,
                _: &StepContext,
            ) -> Result<Outcome<u32, String>, LoopError> {
                Ok(Outcome::Halt(n.to_string()))
            }
            fn name(&self) -> &'static str {
                "String"
            }
        }

        let a: Const<u32, u32> = Const::new(42u32, "A");
        let composed = a.then(StringStrategy);
        let runner = LoopRunner::new(composed, Budget::unlimited());
        let mut stream = runner.run(ThenState::First(0u32), ChainContext::default(), None);

        let mut final_out = None;
        while let Some(r) = stream.next().await {
            let step = r.unwrap();
            if let Outcome::Halt(s) = step.outcome {
                final_out = Some(s);
            }
        }
        assert_eq!(final_out.as_deref(), Some("42"));
    }

    #[tokio::test]
    async fn parallel_combines_heterogeneous_states() {
        // Left: u32 state, Right: String state — different types.
        let left: Const<u32, u32> = Const::new(1u32, "Left");
        let right: Const<String, String> = Const::new("hello".into(), "Right");
        let composed = left.parallel(right);

        let runner = LoopRunner::new(composed, Budget::unlimited());
        let mut stream = runner.run((0u32, String::new()), ChainContext::default(), None);

        let mut final_out = None;
        while let Some(r) = stream.next().await {
            let step = r.unwrap();
            if let Outcome::Halt(out) = step.outcome {
                final_out = Some(out);
            }
        }
        let (n, s) = final_out.unwrap();
        assert_eq!(n, 1u32);
        assert_eq!(s, "hello");
    }

    #[tokio::test]
    async fn then_parallel_chain_depth_does_not_exceed_max() {
        // Topology: A.then(B) — at depth 0, Then adds 1 child hop per step.
        // Verify the stream completes without ChainDepthExceeded.
        let a: Const<u32, u32> = Const::new(1u32, "A");
        let b: Const<u32, u32> = Const::new(2u32, "B");
        let composed = a.then(b);
        let runner = LoopRunner::new(composed, Budget::unlimited());
        let mut stream = runner.run(ThenState::First(0u32), ChainContext::default(), None);

        while let Some(r) = stream.next().await {
            assert!(r.is_ok(), "unexpected error: {:?}", r.unwrap_err());
        }
    }

    #[test]
    fn child_ctx_enforces_max_depth() {
        let deep = ChainContext {
            depth: MAX_CHAIN_DEPTH,
            origin: None,
            aud: None,
        };
        let ctx = StepContext {
            turn: 1,
            chain: deep,
            session_id: None,
        };
        assert!(child_ctx(&ctx).is_err());
    }
}
