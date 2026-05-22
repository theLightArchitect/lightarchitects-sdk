//! Core `Strategy` trait and `LoopRunner` — L1 agentic loop execution engine.
//!
//! # Design
//!
//! `LoopRunner<S>` drives a [`Strategy`] via [`futures_util::stream::unfold`],
//! producing a lazy [`Stream`] of [`StepResult`] values. This design choice
//! (per AYIN G5 + SOUL G15, plan iter-2 fix) means:
//!
//! - Steps only execute when the consumer polls the stream.
//! - Budget enforcement fires per step before the next step executes.
//! - Cancellation is free: drop the stream, the loop stops.
//!
//! The unfold seed is `Option<RunState<S>>`. `None` terminates the stream
//! cleanly after a halt or error, avoiding an extra empty-step probe.
//!
//! # `ChainContext` invariant
//!
//! Every step propagates a [`ChainContext`] child, incrementing depth and
//! enforcing the Canon §2.6 chain depth ≤ 7 invariant before the strategy
//! step runs. Compose combinators in [`super::compose`] must do the same.

use std::time::Instant;

use async_trait::async_trait;
use futures_util::{Stream, stream};
use std::pin::Pin;

use crate::agent::ChainContext;

use super::{budget::Budget, error::LoopError, trace};

#[cold]
#[allow(clippy::expect_used)]
fn fallback_span() -> crate::ayin::span::TraceSpan {
    crate::ayin::span::TraceContext::new(crate::ayin::span::Actor::claude(), "loop.step.fallback")
        .outcome(crate::ayin::span::TraceOutcome::Error(
            "span build failed".into(),
        ))
        .finish()
        .expect("fallback span is always valid")
}

// ── Outcome ───────────────────────────────────────────────────────────────────

/// Return value of a single strategy step.
///
/// Signals whether execution should continue with a new `State` or halt with
/// a terminal `Output`.
#[derive(Debug)]
pub enum Outcome<State, Output> {
    /// Step completed; loop continues with the new state.
    Continue(State),
    /// Loop is done; this is the final result.
    Halt(Output),
}

// ── StepContext ───────────────────────────────────────────────────────────────

/// Per-step execution context passed to every [`Strategy::step`] call.
#[derive(Debug, Clone)]
pub struct StepContext {
    /// Current step number (1-based).
    pub turn: u32,
    /// Chain-of-trust context for this hop (Canon §2.6, depth already incremented).
    pub chain: ChainContext,
    /// Optional session ID for AYIN span correlation.
    pub session_id: Option<String>,
}

// ── Strategy ──────────────────────────────────────────────────────────────────

/// Core trait for all L1 agentic strategies.
///
/// Implementors define a single step of the strategy loop. [`LoopRunner`]
/// repeatedly calls [`step`] with the current state until the strategy returns
/// [`Outcome::Halt`] or a [`Budget`] limit is reached.
///
/// # Thread safety
///
/// All strategies must be `Send + Sync + 'static`. Stateful strategies should
/// pass mutable data through `State`, or use an `Arc<Mutex<…>>` for shared
/// resources that cannot be threaded through the step interface.
///
/// [`step`]: Strategy::step
#[async_trait]
pub trait Strategy: Send + Sync + 'static {
    /// Mutable state threaded through each step.
    type State: Send + Clone + 'static;
    /// Terminal result produced when the strategy halts.
    type Output: Send + 'static;

    /// Execute one step of the strategy.
    ///
    /// Returns [`Outcome::Continue`] with the next state, or
    /// [`Outcome::Halt`] with the final output.
    ///
    /// # Errors
    ///
    /// Returns [`LoopError`] on strategy-level failures. Budget enforcement is
    /// handled by [`LoopRunner`] before each step.
    async fn step(
        &self,
        state: Self::State,
        ctx: &StepContext,
    ) -> Result<Outcome<Self::State, Self::Output>, LoopError>;

    /// Human-readable name for logs and AYIN spans.
    fn name(&self) -> &'static str;

    /// Estimated USD cost per step for budget pre-flight (0.0 if unknown).
    fn estimated_step_cost_usd(&self) -> f64 {
        0.0
    }
}

// ── StepResult ────────────────────────────────────────────────────────────────

/// A completed step emitted by the [`LoopRunner`] stream.
pub struct StepResult<S: Strategy> {
    /// Turn index (1-based).
    pub turn: u32,
    /// Outcome of this step.
    pub outcome: Outcome<S::State, S::Output>,
    /// USD cost as reported by the strategy (0.0 if not tracked).
    pub cost_usd: f64,
    /// AYIN span for this step (submit to AYIN at `:3742` if desired).
    pub span: crate::ayin::span::TraceSpan,
}

impl<S: Strategy> std::fmt::Debug for StepResult<S>
where
    S::State: std::fmt::Debug,
    S::Output: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepResult")
            .field("turn", &self.turn)
            .field("outcome", &self.outcome)
            .field("cost_usd", &self.cost_usd)
            .field("span", &self.span)
            .finish()
    }
}

// ── Internal unfold seed ──────────────────────────────────────────────────────

struct RunState<S: Strategy> {
    state: S::State,
    chain: ChainContext,
    session_id: Option<String>,
    turn: u32,
    budget: Budget,
    strategy: S,
}

// ── LoopRunner ────────────────────────────────────────────────────────────────

/// Drives a [`Strategy`] to completion, enforcing [`Budget`] at each step.
///
/// Produces a lazy [`Stream`] of [`StepResult`] values. The stream terminates
/// when the strategy returns [`Outcome::Halt`], a budget limit is exceeded, or
/// a step returns an error.
///
/// # Example
///
/// ```rust,no_run
/// use lightarchitects::agent::loops::{LoopRunner, Outcome, Strategy, StepContext};
/// use lightarchitects::agent::loops::error::LoopError;
/// use lightarchitects::agent::loops::budget::Budget;
/// use lightarchitects::agent::ChainContext;
/// use async_trait::async_trait;
/// use futures_util::StreamExt as _;
///
/// struct CountDown;
///
/// #[async_trait]
/// impl Strategy for CountDown {
///     type State = u32;
///     type Output = String;
///
///     async fn step(&self, n: u32, _ctx: &StepContext) -> Result<Outcome<u32, String>, LoopError> {
///         if n == 0 { Ok(Outcome::Halt("done".into())) }
///         else { Ok(Outcome::Continue(n - 1)) }
///     }
///     fn name(&self) -> &'static str { "CountDown" }
/// }
///
/// # async fn run() {
/// let mut stream = LoopRunner::new(CountDown, Budget::new(10, 1.0))
///     .run(5u32, ChainContext::default(), None);
/// while let Some(step) = futures_util::StreamExt::next(&mut stream).await {
///     // handle Ok(StepResult) or Err(LoopError)
/// }
/// # }
/// ```
pub struct LoopRunner<S: Strategy> {
    strategy: S,
    budget: Budget,
}

impl<S: Strategy> LoopRunner<S> {
    /// Create a runner for the given strategy with the given budget.
    #[must_use]
    pub fn new(strategy: S, budget: Budget) -> Self {
        Self { strategy, budget }
    }

    /// Run the strategy from `initial_state`, returning a lazy step stream.
    ///
    /// # Chain depth
    ///
    /// `chain_ctx` is the caller's current depth. Each step calls
    /// [`ChainContext::child()`], incrementing depth. The stream emits
    /// [`LoopError::ChainDepthExceeded`] and terminates if depth would exceed
    /// [`MAX_CHAIN_DEPTH`].
    ///
    /// [`MAX_CHAIN_DEPTH`]: crate::agent::MAX_CHAIN_DEPTH
    #[allow(clippy::missing_panics_doc)]
    pub fn run(
        self,
        initial_state: S::State,
        chain_ctx: ChainContext,
        session_id: Option<String>,
    ) -> Pin<Box<dyn Stream<Item = Result<StepResult<S>, LoopError>> + Send>> {
        let seed = RunState {
            state: initial_state,
            chain: chain_ctx,
            session_id,
            turn: 0,
            budget: self.budget,
            strategy: self.strategy,
        };

        Box::pin(stream::unfold(Some(seed), |opt| async move {
            // None seed → stream already terminated.
            let RunState {
                state,
                chain,
                session_id,
                turn,
                mut budget,
                strategy,
            } = opt?;

            let turn = turn + 1;

            // Enforce Canon §2.6 chain depth before the step.
            let Ok(step_chain) = chain.child() else {
                let err = LoopError::ChainDepthExceeded { depth: chain.depth };
                return Some((Err(err), None));
            };

            let step_ctx = StepContext {
                turn,
                chain: step_chain.clone(),
                session_id: session_id.clone(),
            };

            let start = Instant::now();
            let outcome = match strategy.step(state.clone(), &step_ctx).await {
                Ok(o) => o,
                Err(e) => return Some((Err(e), None)),
            };
            let cost_usd = strategy.estimated_step_cost_usd();

            // Budget enforcement before emitting the result.
            if let Err(e) = budget.record_step(cost_usd) {
                return Some((Err(e), None));
            }

            let halted = matches!(outcome, Outcome::Halt(_));

            // Dual-emit: tracing::info! + AYIN TraceSpan (iter-2 G5).
            let span = trace::emit_step(
                strategy.name(),
                turn,
                cost_usd,
                start,
                halted,
                session_id.as_deref(),
            )
            .unwrap_or_else(|_| fallback_span());

            if halted {
                // Terminal step: emit result, then send None to end stream.
                let result = StepResult {
                    turn,
                    outcome,
                    cost_usd,
                    span,
                };
                Some((Ok(result), None))
            } else {
                // Extract the next state from Continue before building StepResult.
                let Outcome::Continue(next_state) = outcome else {
                    unreachable!("halted is false → outcome is Continue");
                };
                let next_seed = RunState {
                    state: next_state.clone(),
                    chain: step_chain,
                    session_id,
                    turn,
                    budget,
                    strategy,
                };
                let result = StepResult {
                    turn,
                    outcome: Outcome::Continue(next_state),
                    cost_usd,
                    span,
                };
                Some((Ok(result), Some(next_seed)))
            }
        }))
    }
}
