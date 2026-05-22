//! Chain-depth invariant tests for composed L1 strategies (Canon §2.6).
//!
//! Topology under test:  `A.then(B).parallel(C)`
//!
//! Depth budget at depth-0 call:
//!
//! ```text
//! depth 0  caller passes ChainContext { depth: 0 }
//! depth 1  LoopRunner::run calls chain.child() before each step
//! depth 2  Parallel::step calls chain.child() for each branch
//! depth 3  Then::step calls chain.child()
//! depth 4  leaf Strategy::step
//! ```
//!
//! `MAX_CHAIN_DEPTH` = 7, so this topology has 3 points of headroom for
//! `ConversationSession` (Phase 3) and `WorkerPool` (Phase 5) layers.

#![cfg(feature = "loops-core")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::doc_markdown)]

use futures_util::StreamExt as _;
use lightarchitects::agent::{
    ChainContext, MAX_CHAIN_DEPTH,
    loops::{
        Budget, LoopError, LoopRunner, Outcome, StepContext, Strategy,
        compose::{StrategyExt, ThenState},
    },
};

// ── Leaf strategies ───────────────────────────────────────────────────────────

/// Records the chain depth it was called with on the first step.
struct DepthRecorder {
    name: &'static str,
}

#[async_trait::async_trait]
impl Strategy for DepthRecorder {
    type State = u8; // carries the recorded max depth
    type Output = u8;

    async fn step(&self, state: u8, ctx: &StepContext) -> Result<Outcome<u8, u8>, LoopError> {
        let depth = ctx.chain.depth;
        let max_so_far = state.max(depth);
        Ok(Outcome::Halt(max_so_far))
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// `A.then(B)` at depth 0 must stay within `MAX_CHAIN_DEPTH`.
///
/// Depth trace (from depth 0):
/// - `LoopRunner` +1 → 1
/// - Then +1 → 2
/// - leaf A sees depth 2 (First state)
/// - Then +1 → 2 again (same combinator, second pass)
/// - leaf B sees depth 2 (Second state)
#[tokio::test]
async fn then_topology_stays_within_max_depth() {
    let a = DepthRecorder { name: "A" };
    let b = DepthRecorder { name: "B" };
    // u8 → u8 trivially satisfies A::Output: Into<B::State>
    let composed = a.then(b);
    let runner = LoopRunner::new(composed, Budget::unlimited());
    let mut stream = runner.run(ThenState::First(0u8), ChainContext::default(), None);

    let mut max_depth_seen = 0u8;
    while let Some(result) = stream.next().await {
        assert!(
            result.is_ok(),
            "unexpected error: {:?}",
            result.unwrap_err()
        );
        if let Outcome::Halt(d) = result.unwrap().outcome {
            max_depth_seen = max_depth_seen.max(d);
        }
    }
    assert!(
        max_depth_seen <= MAX_CHAIN_DEPTH,
        "max depth {max_depth_seen} exceeded MAX_CHAIN_DEPTH {MAX_CHAIN_DEPTH}"
    );
}

/// `A.parallel(B)` at depth 0 — each branch gets its own child, so both see
/// depth 2 but they are independent (not stacked).
#[tokio::test]
async fn parallel_topology_stays_within_max_depth() {
    let a = DepthRecorder { name: "Left" };
    let b = DepthRecorder { name: "Right" };
    let composed = a.parallel(b);
    let runner = LoopRunner::new(composed, Budget::unlimited());
    let mut stream = runner.run((0u8, 0u8), ChainContext::default(), None);

    while let Some(result) = stream.next().await {
        assert!(
            result.is_ok(),
            "unexpected error: {:?}",
            result.unwrap_err()
        );
        if let Outcome::Halt((dl, dr)) = result.unwrap().outcome {
            assert!(
                dl <= MAX_CHAIN_DEPTH,
                "left depth {dl} exceeded MAX_CHAIN_DEPTH {MAX_CHAIN_DEPTH}"
            );
            assert!(
                dr <= MAX_CHAIN_DEPTH,
                "right depth {dr} exceeded MAX_CHAIN_DEPTH {MAX_CHAIN_DEPTH}"
            );
        }
    }
}

/// `(A.then(B)).parallel(C.then(D))` at depth 0 — both branches are
/// symmetric `Then` combinators that halt in the same 2 steps.
///
/// Depth trace per step (from depth 0):
/// - Runner +1 → 1; Parallel +1 → 2; Then +1 → 3; leaf sees 3
///
/// MAX_CHAIN_DEPTH = 7, so 3 is well within budget.
#[tokio::test]
async fn then_parallel_topology_stays_within_max_depth() {
    let a = DepthRecorder { name: "A" };
    let b = DepthRecorder { name: "B" };
    let c = DepthRecorder { name: "C" };
    let d = DepthRecorder { name: "D" };
    let then_ab = a.then(b);
    let then_cd = c.then(d);
    // Both branches are Then combinators → both take 2 steps → symmetric halt.
    let composed = then_ab.parallel(then_cd);

    let runner = LoopRunner::new(composed, Budget::unlimited());
    let init = (ThenState::First(0u8), ThenState::First(0u8));
    let mut stream = runner.run(init, ChainContext::default(), None);

    let mut halted = false;
    while let Some(result) = stream.next().await {
        assert!(
            result.is_ok(),
            "unexpected error: {:?}",
            result.unwrap_err()
        );
        if let Outcome::Halt((left_max, right_max)) = result.unwrap().outcome {
            assert!(
                left_max <= MAX_CHAIN_DEPTH,
                "left (A.then(B)) max depth {left_max} exceeded {MAX_CHAIN_DEPTH}"
            );
            assert!(
                right_max <= MAX_CHAIN_DEPTH,
                "right (C.then(D)) max depth {right_max} exceeded {MAX_CHAIN_DEPTH}"
            );
            halted = true;
        }
    }
    assert!(halted, "expected stream to halt cleanly");
}

/// Starting near the limit: depth 4 completes `A.then(B)` (2 runner steps,
/// each costing +2 hops via runner+Then) without exceeding MAX_CHAIN_DEPTH = 7.
///
/// Depth trace from depth 4:
/// - Step 1: runner 4→5, Then 5→6, leaf A at 6 ✓
/// - Step 2: runner 5→6, Then 6→7, leaf B at 7 ✓ (≤ MAX_CHAIN_DEPTH)
#[tokio::test]
async fn near_limit_depth_completes_cleanly() {
    let a = DepthRecorder { name: "A" };
    let b = DepthRecorder { name: "B" };
    let composed = a.then(b);
    let runner = LoopRunner::new(composed, Budget::unlimited());
    let start_ctx = ChainContext {
        depth: 4,
        origin: None,
        aud: None,
    };

    let mut stream = runner.run(ThenState::First(0u8), start_ctx, None);
    let mut completed = false;
    while let Some(result) = stream.next().await {
        assert!(
            result.is_ok(),
            "unexpected depth error at depth 4: {:?}",
            result.unwrap_err()
        );
        if let Outcome::Halt(_) = result.unwrap().outcome {
            completed = true;
        }
    }
    assert!(completed, "expected stream to halt cleanly");
}

/// Starting at MAX_CHAIN_DEPTH - 1 = 6 with `A.then(B)` requires 3 child()
/// calls (runner +1, Then +1, leaf call) — this exceeds MAX_CHAIN_DEPTH.
/// The runner must emit `LoopError::ChainDepthExceeded` before the second step.
#[tokio::test]
async fn over_limit_depth_emits_chain_depth_exceeded() {
    let a = DepthRecorder { name: "A" };
    let b = DepthRecorder { name: "B" };
    let composed = a.then(b);
    let runner = LoopRunner::new(composed, Budget::unlimited());
    // depth 6: runner +1 → 7, Then +1 → 8 > MAX_CHAIN_DEPTH → error on first step
    let start_ctx = ChainContext {
        depth: 6,
        origin: None,
        aud: None,
    };

    let mut stream = runner.run(ThenState::First(0u8), start_ctx, None);
    let mut saw_depth_error = false;
    while let Some(result) = stream.next().await {
        match result {
            Err(LoopError::ChainDepthExceeded { .. }) => {
                saw_depth_error = true;
                break;
            }
            Ok(_) => {}
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }
    assert!(
        saw_depth_error,
        "expected ChainDepthExceeded but stream completed without error"
    );
}
