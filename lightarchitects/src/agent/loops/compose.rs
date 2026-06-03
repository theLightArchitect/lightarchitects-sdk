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

// ── Race ──────────────────────────────────────────────────────────────────────

/// Run strategy `A` and strategy `B` concurrently; the **first to halt wins**.
///
/// On every step both branches are driven via [`tokio::join!`]. As soon as
/// either branch produces [`Outcome::Halt`], the `Race` halts with that
/// output — the other branch is abandoned. When both branches halt in the same
/// step, `A`'s output wins (converted via `A::Output: Into<B::Output>`).
///
/// [`Outcome::Pause`] signals from either branch are treated as continue
/// signals for the purposes of the race — the state is preserved but the HITL
/// request is discarded. Use [`Parallel`] or custom orchestration when you need
/// to honour HITL pauses inside a race.
///
/// Like [`Parallel`], each branch receives a separate [`ChainContext::child()`]
/// to model independent chains rather than a single deeper chain.
pub struct Race<A, B> {
    left: A,
    right: B,
}

impl<A: Strategy, B: Strategy> Race<A, B>
where
    A::Output: Into<B::Output>,
{
    /// Compose `left` and `right` as a racing pair; first to halt wins.
    #[must_use]
    pub fn new(left: A, right: B) -> Self {
        Self { left, right }
    }
}

#[async_trait]
impl<A, B> Strategy for Race<A, B>
where
    A: Strategy,
    B: Strategy,
    A::Output: Into<B::Output>,
{
    /// Independent sub-states; both advance until one halts.
    type State = (A::State, B::State);
    /// The winner's output — `A::Output` is converted via `Into<B::Output>`.
    type Output = B::Output;

    async fn step(
        &self,
        (a_state, b_state): (A::State, B::State),
        ctx: &StepContext,
    ) -> Result<Outcome<(A::State, B::State), B::Output>, LoopError> {
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
            // A halts first (or both halt simultaneously — A wins).
            (Outcome::Halt(a_out), _) => Ok(Outcome::Halt(a_out.into())),
            // B halts; A has not yet halted.
            (_, Outcome::Halt(b_out)) => Ok(Outcome::Halt(b_out)),
            // Neither halted; extract next states (Pause treated as Continue).
            (a_outcome, b_outcome) => {
                let a_next = match a_outcome {
                    Outcome::Continue(s) | Outcome::Pause(s, _) => s,
                    Outcome::Halt(_) => unreachable!(),
                };
                let b_next = match b_outcome {
                    Outcome::Continue(s) | Outcome::Pause(s, _) => s,
                    Outcome::Halt(_) => unreachable!(),
                };
                Ok(Outcome::Continue((a_next, b_next)))
            }
        }
    }

    fn name(&self) -> &'static str {
        "Race"
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

    /// Race `self` against `other`; first to halt wins.
    ///
    /// Requires `Self::Output: Into<Other::Output>` so both outputs share a
    /// common target type.
    fn race<Other>(self, other: Other) -> Race<Self, Other>
    where
        Other: Strategy,
        Self::Output: Into<Other::Output>,
    {
        Race::new(self, other)
    }

    /// Fallback composition: run `self`; on [`Outcome::Pause`] or step error,
    /// invoke `fallback` with the original state.
    ///
    /// [`Outcome::Continue`] and [`Outcome::Halt`] pass through unchanged.
    ///
    /// # Semantics
    ///
    /// | Primary outcome      | `WithFallback` behaviour               |
    /// |----------------------|----------------------------------------|
    /// | `Continue(state)`    | `Continue(state)` — pass through       |
    /// | `Halt(output)`       | `Halt(output)` — pass through          |
    /// | `Pause(state, _req)` | invoke `fallback.step(original_state)` |
    /// | `Err(_)`             | invoke `fallback.step(original_state)` |
    ///
    /// WHY Pause triggers fallback: Pause signals the primary cannot proceed
    /// without HITL input. `.with_fallback()` provides graceful degradation
    /// in lieu of a HITL interrupt — the operator opts in explicitly.
    fn with_fallback<F>(self, fallback: F) -> WithFallback<Self, F>
    where
        F: Strategy<State = Self::State, Output = Self::Output>,
    {
        WithFallback {
            primary: self,
            fallback,
        }
    }

    /// Cache-wrap: memoize [`Outcome::Halt`] outputs via the SOUL-backed cache.
    ///
    /// Requires the `soul-cache` feature. [`Outcome::Continue`] and
    /// [`Outcome::Pause`] are never cached — they reflect transient state.
    ///
    /// # Cache contract
    ///
    /// 1. Check [`SoulCache::get`] before running `self`.
    /// 2. On cache hit: return `Halt(cached_output)` without stepping.
    /// 3. On cache miss + `Halt(output)`: persist via [`SoulCache::put`], return `Halt(output)`.
    /// 4. On cache miss + other outcome: pass through without caching.
    ///
    /// [`SoulCache`]: crate::agent::cache::SoulCache
    #[cfg(feature = "soul-cache")]
    fn cached(
        self,
        cache: crate::agent::cache::SoulCache<Self::State, Self::Output>,
    ) -> Cached<Self>
    where
        Self::State: crate::agent::cache::CacheKey + serde::Serialize + Clone,
        Self::Output: serde::Serialize + serde::de::DeserializeOwned + Clone + Send + Sync,
    {
        Cached { inner: self, cache }
    }
}

impl<S: Strategy + Sized> StrategyExt for S {}

// ── WithFallback ──────────────────────────────────────────────────────────────

/// Invoke `fallback` on [`Outcome::Pause`] or step error; pass other outcomes through.
///
/// Constructed via [`StrategyExt::with_fallback`].
pub struct WithFallback<S, F> {
    primary: S,
    fallback: F,
}

#[async_trait]
impl<S, F> Strategy for WithFallback<S, F>
where
    S: Strategy,
    F: Strategy<State = S::State, Output = S::Output>,
    S::State: Clone,
{
    type State = S::State;
    type Output = S::Output;

    async fn step(
        &self,
        state: Self::State,
        ctx: &StepContext,
    ) -> Result<Outcome<Self::State, Self::Output>, LoopError> {
        // Pass-through arms: Continue and Halt need no fallback.
        // Fallback arms: Pause (primary can't self-proceed) and step Err.
        // WHY merged: both trigger the same fallback path; merging prevents
        // the `match_same_arms` lint without changing semantics.
        let primary_result = self.primary.step(state.clone(), ctx).await;
        match primary_result {
            Ok(Outcome::Continue(next)) => Ok(Outcome::Continue(next)),
            Ok(Outcome::Halt(out)) => Ok(Outcome::Halt(out)),
            Ok(Outcome::Pause(_, _)) | Err(_) => self.fallback.step(state, ctx).await,
        }
    }

    fn name(&self) -> &'static str {
        "WithFallback"
    }
}

// ── Cached ────────────────────────────────────────────────────────────────────

/// Memoize [`Outcome::Halt`] outputs via the SOUL-backed cache.
///
/// Constructed via [`StrategyExt::cached`]. Requires `--features soul-cache`.
#[cfg(feature = "soul-cache")]
pub struct Cached<S>
where
    S: Strategy,
    S::State: crate::agent::cache::CacheKey + serde::Serialize + Clone,
    S::Output: serde::Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
{
    inner: S,
    cache: crate::agent::cache::SoulCache<S::State, S::Output>,
}

#[cfg(feature = "soul-cache")]
#[async_trait]
impl<S> Strategy for Cached<S>
where
    S: Strategy,
    S::State: crate::agent::cache::CacheKey + serde::Serialize + Clone,
    S::Output: serde::Serialize + serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
{
    type State = S::State;
    type Output = S::Output;

    async fn step(
        &self,
        state: Self::State,
        ctx: &StepContext,
    ) -> Result<Outcome<Self::State, Self::Output>, LoopError> {
        // L1/L2 cache check before stepping (avoids redundant work).
        if let Some(cached_out) = self.cache.get(&state).await {
            return Ok(Outcome::Halt(cached_out));
        }
        let outcome = self.inner.step(state.clone(), ctx).await?;
        // Persist only on Halt — Continue/Pause reflect transient state
        // and must never be cached (WHY: caching them would break cross-call
        // invariants; a cached Continue would skip the actual step body).
        if let Outcome::Halt(ref out) = outcome {
            self.cache.put(&state, out.clone()).await;
        }
        Ok(outcome)
    }

    fn name(&self) -> &'static str {
        "Cached"
    }
}

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
#[allow(
    clippy::unwrap_used,
    clippy::items_after_statements,
    clippy::uninlined_format_args
)]
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

    // ── WithFallback semantics (G-COMPOSE-03) ────────────────────────────────

    /// Strategy that always emits `Pause`.
    struct PauseAlways;

    #[async_trait::async_trait]
    impl Strategy for PauseAlways {
        type State = u32;
        type Output = u32;

        async fn step(&self, s: u32, _: &StepContext) -> Result<Outcome<u32, u32>, LoopError> {
            Ok(Outcome::Pause(
                s,
                crate::agent::loops::runner::HitlRequest {
                    question: "pause".into(),
                    options: vec![],
                    header: "pause".into(),
                },
            ))
        }

        fn name(&self) -> &'static str {
            "PauseAlways"
        }
    }

    /// Strategy that always returns `LoopError::StepFailed`.
    struct ErrorAlways;

    #[async_trait::async_trait]
    impl Strategy for ErrorAlways {
        type State = u32;
        type Output = u32;

        async fn step(&self, _: u32, _: &StepContext) -> Result<Outcome<u32, u32>, LoopError> {
            Err(LoopError::StepFailed("always fails".into()))
        }

        fn name(&self) -> &'static str {
            "ErrorAlways"
        }
    }

    #[tokio::test]
    async fn with_fallback_invoked_on_pause() {
        // G-COMPOSE-03: Pause from primary → fallback is invoked.
        let primary = PauseAlways;
        let fallback: Const<u32, u32> = Const::new(99u32, "fallback");
        let combined = primary.with_fallback(fallback);

        let ctx = StepContext {
            turn: 1,
            chain: ChainContext::default(),
            session_id: None,
        };
        let result = combined.step(0u32, &ctx).await.unwrap();
        assert!(matches!(result, Outcome::Halt(99)));
    }

    #[tokio::test]
    async fn with_fallback_invoked_on_err() {
        // G-COMPOSE-03: Step error from primary → fallback is invoked.
        let primary = ErrorAlways;
        let fallback: Const<u32, u32> = Const::new(42u32, "fallback");
        let combined = primary.with_fallback(fallback);

        let ctx = StepContext {
            turn: 1,
            chain: ChainContext::default(),
            session_id: None,
        };
        let result = combined.step(0u32, &ctx).await.unwrap();
        assert!(matches!(result, Outcome::Halt(42)));
    }

    #[tokio::test]
    async fn with_fallback_not_invoked_on_continue() {
        // G-COMPOSE-03: Continue from primary passes through; fallback NOT called.
        struct ContinueOnce(std::sync::atomic::AtomicU32);
        #[async_trait::async_trait]
        impl Strategy for ContinueOnce {
            type State = u32;
            type Output = u32;
            async fn step(&self, s: u32, _: &StepContext) -> Result<Outcome<u32, u32>, LoopError> {
                let n = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == 0 {
                    Ok(Outcome::Continue(s + 1))
                } else {
                    Ok(Outcome::Halt(s))
                }
            }
            fn name(&self) -> &'static str {
                "ContinueOnce"
            }
        }
        struct PanicIfCalled;
        #[async_trait::async_trait]
        impl Strategy for PanicIfCalled {
            type State = u32;
            type Output = u32;
            async fn step(&self, _: u32, _: &StepContext) -> Result<Outcome<u32, u32>, LoopError> {
                Err(LoopError::StepFailed(
                    "fallback must not be called on Continue".into(),
                ))
            }
            fn name(&self) -> &'static str {
                "PanicIfCalled"
            }
        }

        let primary = ContinueOnce(std::sync::atomic::AtomicU32::new(0));
        let combined = primary.with_fallback(PanicIfCalled);
        let ctx = StepContext {
            turn: 1,
            chain: ChainContext::default(),
            session_id: None,
        };
        // First step → Continue(1) — fallback NOT triggered.
        let result = combined.step(0u32, &ctx).await.unwrap();
        assert!(matches!(result, Outcome::Continue(1)));
    }

    #[tokio::test]
    async fn with_fallback_not_invoked_on_halt() {
        // G-COMPOSE-03: Halt from primary passes through; fallback NOT called.
        let primary: Const<u32, u32> = Const::new(7u32, "primary");
        struct PanicIfCalled;
        #[async_trait::async_trait]
        impl Strategy for PanicIfCalled {
            type State = u32;
            type Output = u32;
            async fn step(&self, _: u32, _: &StepContext) -> Result<Outcome<u32, u32>, LoopError> {
                Err(LoopError::StepFailed(
                    "fallback must not be called on Halt".into(),
                ))
            }
            fn name(&self) -> &'static str {
                "PanicIfCalled"
            }
        }
        let combined = primary.with_fallback(PanicIfCalled);
        let ctx = StepContext {
            turn: 1,
            chain: ChainContext::default(),
            session_id: None,
        };
        let result = combined.step(0u32, &ctx).await.unwrap();
        assert!(matches!(result, Outcome::Halt(7)));
    }

    // ── Backcompat guard (G-COMPOSE-05) ──────────────────────────────────────

    #[test]
    fn backcompat_existing_exports_compile() {
        // Verify Then, Parallel, Race are still constructible without the ext
        // trait; consumers that use the builder API remain unaffected.
        let _: Then<Const<u32, u32>, Const<u32, u32>> =
            Then::new(Const::new(1u32, "a"), Const::new(2u32, "b"));
        let _: Parallel<Const<u32, u32>, Const<String, String>> =
            Parallel::new(Const::new(1u32, "l"), Const::new("x".into(), "r"));
        let _: Race<Const<u32, u32>, Const<u32, u32>> =
            Race::new(Const::new(1u32, "l"), Const::new(2u32, "r"));
    }

    // ── Cached halt-only test (G-COMPOSE-04, requires soul-cache feature) ────

    #[cfg(feature = "soul-cache")]
    #[tokio::test]
    async fn cached_halt_only_persist() {
        use crate::agent::cache::{HelixSnapshotId, NullSoulCacheStore, SoulCache};
        use std::sync::Arc;

        let store = Arc::new(NullSoulCacheStore);
        let snap = HelixSnapshotId::from_timestamp_millis(0);
        let cache: SoulCache<u32, u32> = SoulCache::new("test-halt", store, snap, 100);

        // Strategy that increments a counter on each call so we can detect
        // whether it was bypassed on the second invocation.
        struct CountingHalt(std::sync::Arc<std::sync::atomic::AtomicU32>);
        #[async_trait::async_trait]
        impl Strategy for CountingHalt {
            type State = u32;
            type Output = u32;
            async fn step(&self, s: u32, _: &StepContext) -> Result<Outcome<u32, u32>, LoopError> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(Outcome::Halt(s * 2))
            }
            fn name(&self) -> &'static str {
                "CountingHalt"
            }
        }

        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let strategy = CountingHalt(std::sync::Arc::clone(&counter)).cached(cache.clone());
        let ctx = StepContext {
            turn: 1,
            chain: ChainContext::default(),
            session_id: None,
        };

        // First call: cache miss → strategy runs → cache populated.
        let r1 = strategy.step(5u32, &ctx).await.unwrap();
        assert!(
            matches!(r1, Outcome::Halt(10)),
            "expected Halt(10), got {:?}",
            r1
        );
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "strategy must run on miss"
        );

        // Second call with same state: cache hit → strategy NOT run.
        let r2 = strategy.step(5u32, &ctx).await.unwrap();
        assert!(
            matches!(r2, Outcome::Halt(10)),
            "expected Halt(10) from cache"
        );
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "strategy must NOT run on cache hit"
        );

        // Different state: cache miss → strategy runs again.
        let r3 = strategy.step(3u32, &ctx).await.unwrap();
        assert!(matches!(r3, Outcome::Halt(6)));
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[cfg(feature = "soul-cache")]
    #[tokio::test]
    async fn cached_continue_not_persisted() {
        use crate::agent::cache::{HelixSnapshotId, NullSoulCacheStore, SoulCache};
        use std::sync::Arc;

        let store = Arc::new(NullSoulCacheStore);
        let snap = HelixSnapshotId::from_timestamp_millis(0);
        let cache: SoulCache<u32, u32> = SoulCache::new("test-continue", store, snap, 100);

        struct AlwaysContinue;
        #[async_trait::async_trait]
        impl Strategy for AlwaysContinue {
            type State = u32;
            type Output = u32;
            async fn step(&self, s: u32, _: &StepContext) -> Result<Outcome<u32, u32>, LoopError> {
                Ok(Outcome::Continue(s + 1))
            }
            fn name(&self) -> &'static str {
                "AlwaysContinue"
            }
        }

        let strategy = AlwaysContinue.cached(cache.clone());
        let ctx = StepContext {
            turn: 1,
            chain: ChainContext::default(),
            session_id: None,
        };
        let r = strategy.step(0u32, &ctx).await.unwrap();
        assert!(matches!(r, Outcome::Continue(1)));
        // Verify Continue was NOT cached — get(0) must return None.
        assert!(
            cache.get(&0u32).await.is_none(),
            "Continue must not be cached"
        );
    }

    // ── Property test — .then() equivalence (G-COMPOSE-01) ──────────────────

    /// Collect the entire outcome sequence from a strategy driven by `LoopRunner`.
    async fn collect_outcomes<S>(strategy: S, init: S::State) -> Vec<Outcome<S::State, S::Output>>
    where
        S: Strategy,
        S::State: Clone + std::fmt::Debug,
        S::Output: std::fmt::Debug,
    {
        let runner = LoopRunner::new(strategy, Budget::unlimited());
        let mut stream = runner.run(init, ChainContext::default(), None);
        let mut outcomes = Vec::new();
        while let Some(result) = stream.next().await {
            let step = result.unwrap();
            let halt = matches!(step.outcome, Outcome::Halt(_));
            outcomes.push(step.outcome);
            if halt {
                break;
            }
        }
        outcomes
    }

    /// Run a strategy to its first `Halt` synchronously inside a multi-threaded
    /// tokio runtime using `block_in_place` (avoids nested-runtime panic from
    /// `block_on` inside `#[tokio::test]`).
    fn run_to_halt_sync<S>(strategy: S, init: S::State) -> Option<S::Output>
    where
        S: Strategy,
        S::State: Clone + std::fmt::Debug,
        S::Output: std::fmt::Debug,
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let runner = LoopRunner::new(strategy, Budget::unlimited());
                let mut stream = runner.run(init, ChainContext::default(), None);
                while let Some(r) = stream.next().await {
                    let s = r.unwrap();
                    if let Outcome::Halt(v) = s.outcome {
                        return Some(v);
                    }
                }
                None
            })
        })
    }

    /// Verify `.then()` ext method is semantically equivalent to `Then::new()`.
    ///
    /// Runs 1000 property samples: for varying `u32` output values, both
    /// compositions must produce the same final `Halt` value. G-COMPOSE-01.
    ///
    /// Uses `block_in_place` (not `block_on`) to avoid nested-runtime panic
    /// inside the `#[tokio::test(flavor = "multi_thread")]` runtime.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn then_equivalence_proptest_1000_samples() {
        use proptest::prelude::*;
        use proptest::test_runner::{Config, TestRunner};

        // Cap at 1000 samples; each sample is a single Halt step — fast.
        let config = Config::with_cases(1000);
        let mut runner = TestRunner::new(config);
        runner
            .run(&(any::<u32>(), any::<u32>()), |(a_out, b_out)| {
                // Build two copies — one via Then::new, one via StrategyExt::then.
                // A::Output (u32) satisfies Into<B::State> (u32) via the blanket impl.
                let manual = Then::new(
                    Const::<u32, u32>::new(a_out, "A"),
                    Const::<u32, u32>::new(b_out, "B"),
                );
                let chained =
                    Const::<u32, u32>::new(a_out, "A").then(Const::<u32, u32>::new(b_out, "B"));

                let manual_out = run_to_halt_sync(manual, ThenState::First(0u32));
                let chained_out = run_to_halt_sync(chained, ThenState::First(0u32));
                prop_assert_eq!(manual_out, chained_out, "Then::new vs .then() must agree");
                Ok(())
            })
            .unwrap();
    }

    // ── Parallel actual concurrency (G-COMPOSE-02) ────────────────────────────

    /// Strategy that sleeps `delay_ms` before halting with `()`.
    struct SleepThenHalt {
        delay_ms: u64,
    }

    #[async_trait::async_trait]
    impl Strategy for SleepThenHalt {
        type State = ();
        type Output = ();

        async fn step(&self, _: (), _: &StepContext) -> Result<Outcome<(), ()>, LoopError> {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
            Ok(Outcome::Halt(()))
        }

        fn name(&self) -> &'static str {
            "SleepThenHalt"
        }
    }

    #[tokio::test]
    async fn parallel_actual_concurrency_wall_clock_lt_sum() {
        // G-COMPOSE-02: two 100ms sleeps in parallel must complete in < 150ms.
        // Sequential would be ~200ms; concurrent should be ~100ms.
        let s1 = SleepThenHalt { delay_ms: 100 };
        let s2 = SleepThenHalt { delay_ms: 100 };
        let combined = s1.parallel(s2);

        let start = std::time::Instant::now();
        let outcomes = collect_outcomes(combined, ((), ())).await;
        let elapsed_ms = start.elapsed().as_millis();

        assert!(
            outcomes
                .iter()
                .any(|o| matches!(o, Outcome::Halt(((), ())))),
            "parallel must produce Halt"
        );
        assert!(
            elapsed_ms < 150,
            "parallel wall-clock {} ms must be < 150 ms (sum would be ~200ms)",
            elapsed_ms
        );
    }
}
