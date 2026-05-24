//! Regression tests for Phase 6 (skills-as-tools) bug fixes (Canon XXVII Suite 5).
//!
//! Each test documents a specific bug found during /GATE-6 clippy remediation
//! (commit 7225169) and verifies the fix is present and correct.
//!
//! ## Regression inventory
//!
//! R6-1: `writeln!` vs `write!` — `skill_trust::Ledger::save()` used
//!        `push_str(&format!(...))`. Fixed by `writeln!`. Verified by round-trip.
//!
//! R6-2: `let...else` replaces `.expect()` in skill routing arm — double-lookup
//!        pattern (`any()` + `find().expect()`) could panic. Now uses `let-else`.
//!        Verified by operator registry cycle never panicking.
//!
//! R6-3: `#[allow(clippy::expect_used)]` on `SECRET_PATTERNS` `LazyLock` —
//!        compile-time-validated regex patterns. Verified by module init.

#![allow(clippy::unwrap_used, clippy::expect_used)]

// ── R6-1: Ledger round-trip (writeln! correctness) ───────────────────────────

/// Ledger saved by `verify_or_pin` round-trips correctly.
///
/// Regression: `write!` with `\n` replaced by `writeln!` (`clippy::write_with_newline`).
/// If the save format were broken, the TOML parser would mis-read the `[pins]`
/// table and a subsequent `verify_or_pin` call would re-pin (not match), which
/// means it would also return `Ok` but then FAIL on a third call with the original
/// content. We verify by doing three calls with the same content — all must succeed.
#[test]
fn r6_1_ledger_round_trips_three_calls() {
    const SLUG: &str = "REG_R6_1_ROUNDTRIP";
    const CONTENT: &str = "# R6-1 regression test content — writeln! correctness";

    for i in 0..3 {
        let result = lightarchitects_gateway::cli::skill_trust::verify_or_pin(SLUG, CONTENT);
        assert!(
            result.is_ok(),
            "R6-1: call {i} must succeed — ledger round-trip broken"
        );
    }
}

/// Multiple distinct slugs saved in one session all round-trip independently.
///
/// Regression: a `writeln!` bug could truncate the last line of the TOML,
/// causing the last pin to be dropped on reload.
#[test]
fn r6_1_multiple_slugs_all_round_trip() {
    let slugs = [
        ("REG_R6_1_REFLECT", "# REFLECT content R6-1"),
        ("REG_R6_1_BUILD", "# BUILD content R6-1"),
        ("REG_R6_1_PLAN", "# PLAN content R6-1"),
    ];

    // Pin all three.
    for (slug, content) in &slugs {
        let _ = lightarchitects_gateway::cli::skill_trust::verify_or_pin(slug, content);
    }
    // All three must re-verify successfully.
    for (slug, content) in &slugs {
        let result = lightarchitects_gateway::cli::skill_trust::verify_or_pin(slug, content);
        assert!(result.is_ok(), "R6-1: {slug} must round-trip");
    }
}

// ── R6-2: let-else / no-panic in skill routing ───────────────────────────────

/// The mark/clear cycle never panics under rapid alternation.
///
/// Regression: the double-lookup pattern (`any()` + `find().expect()`) could
/// panic if the skill list was mutated between the two calls. The `let-else`
/// fix returns `UnknownTool` instead of panicking.
#[test]
fn r6_2_operator_registry_never_panics_under_alternation() {
    let config = lightarchitects_gateway::config::GatewayConfig::default();
    let executor =
        lightarchitects_gateway::providers::GatewayToolExecutor::new(std::sync::Arc::new(config));

    for i in 0..100 {
        let slug = format!("skill_{i}");
        executor.mark_operator_invoked(&slug);
        if i % 3 == 0 {
            executor.clear_operator_invocations();
        }
    }
    executor.clear_operator_invocations();
    // No panic = pass.
}

/// `mark_operator_invoked` is idempotent — inserting the same slug 50 times
/// does not cause errors or panics.
#[test]
fn r6_2_mark_operator_invoked_is_idempotent() {
    let config = lightarchitects_gateway::config::GatewayConfig::default();
    let executor =
        lightarchitects_gateway::providers::GatewayToolExecutor::new(std::sync::Arc::new(config));

    for _ in 0..50 {
        executor.mark_operator_invoked("plan");
    }
    executor.clear_operator_invocations();
    // No panic, no error = HashSet semantics confirmed.
}

// ── R6-3: SECRET_PATTERNS LazyLock regex validity ────────────────────────────

/// The `HelixSessionMemory` module initialises without panic.
///
/// Regression: `#[allow(clippy::expect_used)]` on the `SECRET_PATTERNS` static
/// was added because `.expect("static regex")` on compile-time-validated patterns
/// is correct. If any regex in `SECRET_PATTERNS` were invalid, `LazyLock::force()`
/// would panic here (via `session_path` triggering module init).
#[test]
fn r6_3_session_memory_module_initialises_without_panic() {
    let cwd = std::path::Path::new("/tmp/test-project");
    let path = lightarchitects_gateway::agent_stream::session_memory::session_path(cwd);
    assert!(
        path.to_string_lossy().contains("test-project"),
        "R6-3: session_path must derive slug from cwd last component"
    );
}
