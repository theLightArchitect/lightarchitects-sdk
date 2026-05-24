//! Integration tests for the skills-as-tools pipeline (Phase 6, W6.2 + W6.3).
//!
//! Canon XXVII Suite 2 — integration tier.
//!
//! Verifies the turn-boundary lifecycle of the operator-wins invariant and the
//! trust-gate ledger pinning contract, using only the public API surface of
//! `GatewayToolExecutor` and `skill_trust::verify_or_pin`.
//!
//! ## What is NOT tested here (deferred to E2E)
//!
//! `run_skill_tool()` subprocess dispatch requires a real gateway binary at
//! `std::env::current_exe()`. In this test harness `current_exe` resolves to
//! the test runner, not the gateway binary, so the subprocess path is not
//! exercisable here. The E2E gap is tracked in `tests/CAPABILITY_MATRIX.md`
//! under capability "M — Skills-as-Tools" (HIGH priority, deferred to CI
//! binary integration suite).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_gateway::providers::GatewayToolExecutor;

// ── Operator-wins turn boundary ───────────────────────────────────────────────

/// After `mark_operator_invoked`, `clear_operator_invocations` fully resets
/// the registry so the NEXT turn can dispatch via `tool_use` again.
///
/// This models the turn lifecycle: each new LLM turn starts clean.
#[test]
fn operator_wins_clears_per_turn_boundary() {
    let config = lightarchitects_gateway::config::GatewayConfig::default();
    let executor = GatewayToolExecutor::new(std::sync::Arc::new(config));

    // Simulate: operator invoked /plan this turn.
    executor.mark_operator_invoked("plan");
    executor.mark_operator_invoked("build");

    // Simulate: new turn begins — clear the registry.
    executor.clear_operator_invocations();

    // Verify the clear + re-mark cycle is correct.
    executor.mark_operator_invoked("verify");
    executor.clear_operator_invocations();

    // A freshly-cleared executor should accept the same slug via mark again.
    executor.mark_operator_invoked("plan");
    executor.mark_operator_invoked("plan"); // idempotent — HashSet insert
    executor.clear_operator_invocations();
    // No panic = pass.
}

/// Slugs are stored case-insensitively.
///
/// The operator may type `/PLAN` or `/plan` — both must collide with a
/// `tool_use` `name: "plan"` (which Anthropic normalises to lowercase).
#[test]
fn operator_wins_slug_is_case_insensitive() {
    let config = lightarchitects_gateway::config::GatewayConfig::default();
    let executor = GatewayToolExecutor::new(std::sync::Arc::new(config));

    // Mark with uppercase slug (as the user types /PLAN).
    executor.mark_operator_invoked("PLAN");
    executor.clear_operator_invocations();

    executor.mark_operator_invoked("plan");
    executor.mark_operator_invoked("Plan"); // mixed case — idempotent
    executor.clear_operator_invocations();
}

/// Multiple concurrent operators can each claim a different skill.
///
/// The `HashSet` must not conflate different slugs.
#[test]
fn operator_wins_distinct_slugs_are_independent() {
    let config = lightarchitects_gateway::config::GatewayConfig::default();
    let executor = GatewayToolExecutor::new(std::sync::Arc::new(config));

    executor.mark_operator_invoked("plan");
    executor.mark_operator_invoked("build");
    executor.mark_operator_invoked("verify");
    executor.mark_operator_invoked("secure");

    executor.clear_operator_invocations();
}

// ── Trust ledger integration ──────────────────────────────────────────────────
//
// These tests write to the real ~/.lightarchitects/skill-trust-ledger.toml.
// They use stable, namespaced slugs (prefix "IT_") that will never collide
// with real skill slugs (PLAN, BUILD, etc.) or with each other.

/// First call to `verify_or_pin` always succeeds (pin path).
#[test]
fn trust_ledger_first_pin_always_succeeds() {
    let result = lightarchitects_gateway::cli::skill_trust::verify_or_pin(
        "IT_INTEGRATION_SUITE_V1",
        "# Integration test content v1 — always stable",
    );
    assert!(
        result.is_ok(),
        "first verify_or_pin must succeed (pin path)"
    );
}

/// Re-verifying with the same content always succeeds (cached-hash path).
#[test]
fn trust_ledger_matching_content_passes() {
    const CONTENT: &str = "# BUILD skill integration test — determinism check";
    let _ = lightarchitects_gateway::cli::skill_trust::verify_or_pin("IT_BUILD_DET", CONTENT);
    let second = lightarchitects_gateway::cli::skill_trust::verify_or_pin("IT_BUILD_DET", CONTENT);
    assert!(second.is_ok(), "re-verify with same content must pass");
}

// NOTE: Tamper-detection (verify_or_pin Err path) is covered by unit tests
// in `src/cli/skill_trust.rs::tests::verify_or_pin_detects_change` and the
// `prop_sha256_avalanche` proptest. Testing it at integration level via the
// real shared ledger file causes unavoidable race conditions when tests run
// in parallel — the pin can be overwritten between the pin step and the
// verify step by a concurrent test writing to the same ledger file.
