#![cfg(feature = "loops-core")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! End-to-end integration tests for the Phase 4 strategy loop additions.
//!
//! Verifies: `StrategyRegistry` lookup, `Outcome::Pause` HITL suspension,
//! `LoopRunner` stream termination on pause, and full run-to-halt for
//! non-pausing strategies.

use futures_util::StreamExt as _;
use lightarchitects::agent::{
    ChainContext,
    loops::{
        Budget, LoopRunner, LoopState, MetaSkill, Outcome, StrategyRegistry, scrum::ScrumMode,
    },
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn initial_state(ctx: &str) -> LoopState {
    LoopState::new(ctx)
}

// ── registry lookup ───────────────────────────────────────────────────────────

#[test]
fn registry_resolves_all_four_ids() {
    for id in ["build", "secure", "scrum", "enrich"] {
        assert!(
            StrategyRegistry::lookup(id).is_some(),
            "registry must resolve '{id}'"
        );
    }
}

#[test]
fn registry_unknown_id_returns_none() {
    assert!(StrategyRegistry::lookup("chatroom").is_none());
}

#[test]
fn meta_skill_from_id_round_trips() {
    for id in ["build", "secure", "scrum", "enrich"] {
        let skill = MetaSkill::from_id(id).unwrap();
        assert_eq!(skill.strategy_id(), id);
    }
}

// ── BuildStrategy: pauses on phase 0, then runs to halt ───────────────────────

#[tokio::test]
async fn build_strategy_pauses_then_resumes_to_halt() {
    let strategy = StrategyRegistry::lookup("build").unwrap();
    let mut stream = LoopRunner::new(strategy, Budget::unlimited()).run(
        initial_state("build ctx"),
        ChainContext::default(),
        None,
    );

    // Step 1: should pause for architecture approval.
    let step1 = stream.next().await.unwrap().unwrap();
    assert!(
        matches!(step1.outcome, Outcome::Pause(_, _)),
        "build step 1 must pause for arch approval"
    );

    // Stream terminates after Pause — extract resumable state.
    let Outcome::Pause(resumed_state, hitl_req) = step1.outcome else {
        unreachable!()
    };
    assert!(
        !hitl_req.options.is_empty(),
        "HitlRequest must have options"
    );

    // Resume from state advanced past phase 0.
    let strategy2 = StrategyRegistry::lookup("build").unwrap();
    let mut stream2 = LoopRunner::new(strategy2, Budget::unlimited()).run(
        resumed_state,
        ChainContext::default(),
        None,
    );

    // Step 2: Continue (phase 1 → 2).
    let step2 = stream2.next().await.unwrap().unwrap();
    assert!(matches!(step2.outcome, Outcome::Continue(_)));

    // Step 3: Halt.
    let step3 = stream2.next().await.unwrap().unwrap();
    assert!(
        matches!(step3.outcome, Outcome::Halt(_)),
        "build must halt after verify phase"
    );

    // Stream ends after Halt.
    assert!(stream2.next().await.is_none());
}

// ── SecureStrategy: pauses on phase 0 ────────────────────────────────────────

#[tokio::test]
async fn secure_strategy_pauses_for_scope_approval() {
    let strategy = StrategyRegistry::lookup("secure").unwrap();
    let mut stream = LoopRunner::new(strategy, Budget::unlimited()).run(
        initial_state("gateway API"),
        ChainContext::default(),
        None,
    );

    let step = stream.next().await.unwrap().unwrap();
    assert!(
        matches!(step.outcome, Outcome::Pause(_, _)),
        "secure step 1 must pause for scope approval"
    );
    // Stream must end after pause.
    assert!(stream.next().await.is_none());
}

// ── ScrumStrategy: runs all 3 rounds without pause ───────────────────────────

#[tokio::test]
async fn scrum_review_runs_to_halt_without_pause() {
    let strategy = StrategyRegistry::scrum(ScrumMode::Review);
    let mut stream = LoopRunner::new(strategy, Budget::unlimited()).run(
        initial_state("plan review"),
        ChainContext::default(),
        None,
    );

    let mut steps = 0u32;
    let mut halted = false;
    while let Some(result) = stream.next().await {
        let step = result.unwrap();
        steps += 1;
        assert!(
            !matches!(step.outcome, Outcome::Pause(_, _)),
            "scrum review must never pause"
        );
        if matches!(step.outcome, Outcome::Halt(_)) {
            halted = true;
        }
    }
    assert!(halted, "scrum review must halt");
    assert_eq!(steps, 4, "3 Continue steps + 1 Halt step = 4 total");
}

// ── EnrichStrategy: fully autonomous, no pause ───────────────────────────────

#[tokio::test]
async fn enrich_strategy_runs_to_halt_without_pause() {
    let strategy = StrategyRegistry::lookup("enrich").unwrap();
    let mut stream = LoopRunner::new(strategy, Budget::unlimited()).run(
        initial_state("session memory"),
        ChainContext::default(),
        None,
    );

    let mut halted = false;
    while let Some(result) = stream.next().await {
        let step = result.unwrap();
        assert!(
            !matches!(step.outcome, Outcome::Pause(_, _)),
            "enrich must never pause"
        );
        if matches!(step.outcome, Outcome::Halt(_)) {
            halted = true;
        }
    }
    assert!(halted);
}

// ── Budget enforcement ────────────────────────────────────────────────────────

#[tokio::test]
async fn budget_zero_steps_terminates_immediately() {
    let strategy = StrategyRegistry::lookup("enrich").unwrap();
    let budget = Budget::new(0, 0.0);
    let mut stream =
        LoopRunner::new(strategy, budget).run(initial_state("ctx"), ChainContext::default(), None);

    let step = stream.next().await.unwrap();
    assert!(
        step.is_err(),
        "zero-step budget should return an error immediately"
    );
}
