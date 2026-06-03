//! Integration tests for `StrategyExt` LCEL-style combinators.
//!
//! Covers four scenarios exercising the public combinator API:
//!
//! 1. `chain_then_sequential_works` — `.then()` chains two strategies correctly.
//! 2. `parallel_halts_when_both_halt` — `.parallel()` halts only when both branches halt.
//! 3. `existing_compose_consumers_unchanged` — `Then`, `Parallel`, `Race` API back-compat.
//! 4. `with_fallback_invoked_on_pause_integration` — `.with_fallback()` end-to-end via runner.
//!
//! These tests drive strategies through `LoopRunner` (the full execution path),
//! not just the `Strategy::step` method directly.

#![cfg(feature = "loops-core")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use futures_util::StreamExt as _;
use lightarchitects::agent::loops::error::LoopError;
use lightarchitects::agent::loops::runner::HitlRequest;
use lightarchitects::agent::{
    ChainContext,
    loops::{
        Budget, LoopRunner, Outcome, StepContext, Strategy, StrategyExt, ThenState, WithFallback,
    },
};

// ── Shared stub strategies ────────────────────────────────────────────────────

/// Halts immediately with a constant value.
struct Halt<S, O> {
    output: O,
    _state: std::marker::PhantomData<S>,
}

impl<S: Send + Sync + Clone + 'static, O: Send + Sync + Clone + 'static> Halt<S, O> {
    fn new(output: O) -> Self {
        Self {
            output,
            _state: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<S, O> Strategy for Halt<S, O>
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
        "Halt"
    }
}

/// Always emits `Pause`, never halts.
struct PauseAlways;

#[async_trait::async_trait]
impl Strategy for PauseAlways {
    type State = u32;
    type Output = u32;

    async fn step(&self, s: u32, _: &StepContext) -> Result<Outcome<u32, u32>, LoopError> {
        Ok(Outcome::Pause(
            s,
            HitlRequest {
                question: "integration pause".into(),
                options: vec![],
                header: "pause".into(),
            },
        ))
    }

    fn name(&self) -> &'static str {
        "PauseAlways"
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Collect all outcomes from a runner stream until `Halt` or stream end.
async fn collect<S: Strategy>(
    runner: LoopRunner<S>,
    init: S::State,
) -> Vec<Outcome<S::State, S::Output>>
where
    S::State: Clone + std::fmt::Debug,
    S::Output: std::fmt::Debug,
{
    let mut stream = runner.run(init, ChainContext::default(), None);
    let mut outcomes = Vec::new();
    while let Some(r) = stream.next().await {
        let step = r.unwrap();
        let done = matches!(step.outcome, Outcome::Halt(_));
        outcomes.push(step.outcome);
        if done {
            break;
        }
    }
    outcomes
}

// ── Test 1: chain_then_sequential_works ──────────────────────────────────────

#[tokio::test]
async fn chain_then_sequential_works() {
    // A halts with u32 10; B receives 10 as state and halts with "10".
    struct StringifyStrategy;
    #[async_trait::async_trait]
    impl Strategy for StringifyStrategy {
        type State = u32;
        type Output = String;
        async fn step(&self, n: u32, _: &StepContext) -> Result<Outcome<u32, String>, LoopError> {
            Ok(Outcome::Halt(n.to_string()))
        }
        fn name(&self) -> &'static str {
            "Stringify"
        }
    }

    let composed = Halt::<u32, u32>::new(10u32).then(StringifyStrategy);
    let runner = LoopRunner::new(composed, Budget::unlimited());
    let outcomes = collect(runner, ThenState::First(0u32)).await;

    // Expect: Continue(Second(10)) → Halt("10")
    let halt_val = outcomes.iter().find_map(|o| {
        if let Outcome::Halt(s) = o {
            Some(s.clone())
        } else {
            None
        }
    });
    assert_eq!(
        halt_val.as_deref(),
        Some("10"),
        "chain must produce Halt(\"10\")"
    );
}

// ── Test 2: parallel_halts_when_both_halt ────────────────────────────────────

#[tokio::test]
async fn parallel_halts_when_both_halt() {
    let left = Halt::<u32, u32>::new(1u32);
    let right = Halt::<u64, u64>::new(2u64);
    let combined = left.parallel(right);
    let runner = LoopRunner::new(combined, Budget::unlimited());
    let outcomes = collect(runner, (0u32, 0u64)).await;

    let halt_val = outcomes.into_iter().find_map(|o| {
        if let Outcome::Halt(v) = o {
            Some(v)
        } else {
            None
        }
    });
    assert_eq!(
        halt_val,
        Some((1u32, 2u64)),
        "parallel must halt with (1, 2)"
    );
}

// ── Test 3: existing_compose_consumers_unchanged ──────────────────────────────

#[tokio::test]
async fn existing_compose_consumers_unchanged() {
    // Verify that builder-style construction still compiles and works correctly.
    use lightarchitects::agent::loops::{Parallel, Race, Then};

    // Then builder API.
    let via_builder: Then<Halt<u32, u32>, Halt<u32, u32>> =
        Then::new(Halt::new(5u32), Halt::new(7u32));
    let runner = LoopRunner::new(via_builder, Budget::unlimited());
    let outcomes = collect(runner, ThenState::First(0u32)).await;
    let halt_val = outcomes.into_iter().find_map(|o| {
        if let Outcome::Halt(v) = o {
            Some(v)
        } else {
            None
        }
    });
    assert_eq!(halt_val, Some(7u32));

    // Parallel builder API.
    let via_parallel: Parallel<Halt<u32, u32>, Halt<u32, u32>> =
        Parallel::new(Halt::new(3u32), Halt::new(4u32));
    let runner2 = LoopRunner::new(via_parallel, Budget::unlimited());
    let outcomes2 = collect(runner2, (0u32, 0u32)).await;
    let halt2 = outcomes2.into_iter().find_map(|o| {
        if let Outcome::Halt(v) = o {
            Some(v)
        } else {
            None
        }
    });
    assert_eq!(halt2, Some((3u32, 4u32)));

    // Race builder API (A wins when both halt same step).
    let via_race: Race<Halt<u32, u32>, Halt<u32, u32>> =
        Race::new(Halt::new(8u32), Halt::new(9u32));
    let runner3 = LoopRunner::new(via_race, Budget::unlimited());
    let outcomes3 = collect(runner3, (0u32, 0u32)).await;
    let halt3 = outcomes3.into_iter().find_map(|o| {
        if let Outcome::Halt(v) = o {
            Some(v)
        } else {
            None
        }
    });
    // Race: A wins when both halt simultaneously; A::Output into B::Output (u32 → u32).
    assert_eq!(halt3, Some(8u32));
}

// ── Test 4: with_fallback_invoked_on_pause_integration ───────────────────────

#[tokio::test]
async fn with_fallback_invoked_on_pause_integration() {
    // End-to-end: run PauseAlways through LoopRunner with a WithFallback wrapper.
    // Expect: Pause triggers fallback Halt(99), runner produces Halt(99).
    let fallback = Halt::<u32, u32>::new(99u32);
    let combined: WithFallback<PauseAlways, Halt<u32, u32>> = PauseAlways.with_fallback(fallback);
    let runner = LoopRunner::new(combined, Budget::unlimited());
    let outcomes = collect(runner, 0u32).await;

    let halt_val = outcomes.into_iter().find_map(|o| {
        if let Outcome::Halt(v) = o {
            Some(v)
        } else {
            None
        }
    });
    assert_eq!(
        halt_val,
        Some(99u32),
        "fallback Halt(99) must be produced via runner"
    );
}
