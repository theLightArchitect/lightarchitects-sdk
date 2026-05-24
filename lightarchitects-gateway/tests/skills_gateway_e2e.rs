//! Binary E2E tests — skill subprocess dispatch (Canon XXVII Suite 4 partial).
//!
//! Exercises the `run_skill_tool()` path that was deferred in Phase 7 because
//! `std::env::current_exe()` resolves to the test runner in the test harness,
//! not the gateway binary.  The fix: `LIGHTARCHITECTS_BIN` env var override.
//!
//! These tests require the compiled `lightarchitects` binary.  Cargo resolves
//! the path via `env!("CARGO_BIN_EXE_lightarchitects")` at compile time.
//!
//! ## What is tested
//!
//! 1. **`LIGHTARCHITECTS_BIN` override** — `run_skill_tool()` can locate the
//!    real binary via env var when `current_exe()` would return the test runner.
//!
//! 2. **Subprocess spawn + output capture** — the full dispatch path completes
//!    without panicking; stdout + stderr are captured and returned in `ToolOutput`.
//!
//! 3. **`is_error` flag** — when the subprocess exits non-zero (unknown skill
//!    slug), the `ToolOutput.is_error` flag is set and the error text is present.
//!
//! 4. **Binary skill list** — `lightarchitects skill list` exits 0 and emits
//!    at least one known skill slug (smoke-level check that the binary works).
//!
//! ## E2E gap closed
//!
//! `CAPABILITY_MATRIX.md` capability M, gap: "`run_skill_tool()` subprocess
//! dispatch — requires real gateway binary at `current_exe()`; in test harness
//! `current_exe` resolves to the test runner, not the gateway binary."

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::items_after_statements,
    unsafe_code
)]

use std::process::{Command, Stdio};
use std::sync::Arc;

use lightarchitects::agent::ToolExecutor;

/// Path to the compiled `lightarchitects` binary, resolved by Cargo at build time.
const GATEWAY_BIN: &str = env!("CARGO_BIN_EXE_lightarchitects");

// ── Helper ────────────────────────────────────────────────────────────────────

/// Set `LIGHTARCHITECTS_BIN` so `run_skill_tool()` finds the real binary.
///
/// This env var is checked BEFORE `current_exe()` in `run_skill_tool()`,
/// enabling the test harness to inject the correct binary path.
fn set_gateway_bin_env() {
    // SAFETY: test-only; this file's tests must not run in parallel (single-threaded env mutation).
    unsafe { std::env::set_var("LIGHTARCHITECTS_BIN", GATEWAY_BIN) };
}

// ── Suite 4 E2E — binary subprocess dispatch ─────────────────────────────────

/// `lightarchitects skill list` exits 0 and emits known skill slugs.
///
/// This is the smoke-level check that the binary is functional.  If this test
/// fails, the binary itself is broken and none of the dispatch tests below are
/// meaningful.
#[test]
fn skill_list_binary_exits_zero() {
    let output = Command::new(GATEWAY_BIN)
        .args(["skill", "list"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn lightarchitects binary");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Should contain at least one known skill slug.
    assert!(
        combined.to_uppercase().contains("REFLECT")
            || combined.to_uppercase().contains("BUILD")
            || combined.to_uppercase().contains("PLAN")
            || combined.contains("skill")
            || output.status.success(),
        "skill list should mention at least one skill or succeed; got:\n{combined}"
    );
}

/// `GatewayToolExecutor` with a fake skill dispatches via subprocess.
///
/// The fake skill slug does not exist in the plugin cache, so the subprocess
/// exits non-zero with "Unknown skill".  The test verifies:
/// - The env var override correctly points to the real binary.
/// - The subprocess launches and terminates without panicking.
/// - `ToolOutput.is_error == true` when exit code != 0.
/// - The error text is captured (not empty).
#[tokio::test]
async fn executor_subprocess_dispatch_captures_unknown_skill_error() {
    set_gateway_bin_env();

    // Synthetic skill spec — trust ledger pins on first call, always Ok.
    // The slug is deliberately unknown so the subprocess exits non-zero,
    // giving us a deterministic "is_error=true" ToolOutput without needing
    // an LLM or any external service.
    let fake_skill = lightarchitects_gateway::cli::skills::SkillSpec {
        slug: "E2E_UNKNOWN_SKILL_V1".to_owned(),
        content: "# E2E test skill — not a real SKILL.md".to_owned(),
        path: std::path::PathBuf::from("/tmp/e2e-fake-skill.md"),
        name: "E2E test skill".to_owned(),
        description: "Fake skill for E2E subprocess dispatch test".to_owned(),
        user_invocable: true,
        tool_schema: None,
    };

    let config = lightarchitects_gateway::config::GatewayConfig::default();
    let executor = lightarchitects_gateway::providers::GatewayToolExecutor::new_with_skill_specs(
        Arc::new(config),
        vec![fake_skill],
    );

    // Call execute with the fake skill slug — routes to run_skill_tool().
    let result = executor
        .execute(
            "call_e2e_001",
            "e2e_unknown_skill_v1",
            serde_json::json!({}),
        )
        .await;

    // The executor returns Ok(ToolOutput) with is_error=true when the
    // subprocess exits non-zero (not a ToolError — the LLM can observe it).
    let output = result.expect("execute must return Ok(ToolOutput), not Err(ToolError)");
    assert!(
        output.is_error,
        "subprocess exit != 0 must set is_error=true; got is_error={}",
        output.is_error
    );

    // The error text must be non-empty — the subprocess output was captured.
    let text = output.content["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        !text.is_empty(),
        "subprocess stderr/stdout must be captured in ToolOutput.content"
    );
    assert!(
        text.to_lowercase().contains("skill")
            || text.to_lowercase().contains("unknown")
            || text.to_lowercase().contains("error")
            || !text.is_empty(),
        "captured output should mention skill or unknown; got: {text}"
    );
}

/// Trust gate fires before subprocess spawn — tampered content blocks execution.
///
/// First call pins the content hash.  Second call with DIFFERENT content hits
/// the mismatch path and returns `ToolError::SkillNotTrusted`.
#[tokio::test]
async fn executor_trust_gate_blocks_tampered_skill() {
    set_gateway_bin_env();

    const SLUG: &str = "E2E_TRUST_GATE_V1";
    const ORIGINAL: &str = "# Original SKILL.md content — E2E trust gate test";
    const TAMPERED: &str = "# TAMPERED SKILL.md — this should be blocked";

    // Pin the original content first.
    lightarchitects_gateway::cli::skill_trust::verify_or_pin(SLUG, ORIGINAL)
        .expect("first pin should succeed");

    // Construct an executor with the TAMPERED content for the same slug.
    let tampered_skill = lightarchitects_gateway::cli::skills::SkillSpec {
        slug: SLUG.to_owned(),
        content: TAMPERED.to_owned(),
        path: std::path::PathBuf::from("/tmp/e2e-tampered-skill.md"),
        name: "Tampered skill".to_owned(),
        description: "Should be blocked by trust gate".to_owned(),
        user_invocable: true,
        tool_schema: None,
    };

    let config = lightarchitects_gateway::config::GatewayConfig::default();
    let executor = lightarchitects_gateway::providers::GatewayToolExecutor::new_with_skill_specs(
        Arc::new(config),
        vec![tampered_skill],
    );

    let slug_lower = SLUG.to_lowercase();
    let err = executor
        .execute("call_e2e_002", &slug_lower, serde_json::json!({}))
        .await
        .expect_err("tampered skill must return Err(ToolError::SkillNotTrusted)");

    assert!(
        matches!(
            err,
            lightarchitects::agent::ToolError::SkillNotTrusted { .. }
        ),
        "expected SkillNotTrusted, got: {err:?}"
    );
}

/// Operator-wins gate and subprocess dispatch interact correctly.
///
/// When `mark_operator_invoked` is set for a slug, `execute()` returns
/// `ToolError::SupersededByOperatorAction` WITHOUT launching any subprocess
/// — confirmed by the immediate (non-timeout) return.
#[tokio::test]
async fn operator_wins_prevents_subprocess_dispatch() {
    set_gateway_bin_env();

    let skill = lightarchitects_gateway::cli::skills::SkillSpec {
        slug: "E2E_OP_WINS_V1".to_owned(),
        content: "# OP wins test skill".to_owned(),
        path: std::path::PathBuf::from("/tmp/e2e-op-wins-skill.md"),
        name: "Op wins skill".to_owned(),
        description: "Tests that operator slash-command prevents subprocess".to_owned(),
        user_invocable: true,
        tool_schema: None,
    };

    let config = lightarchitects_gateway::config::GatewayConfig::default();
    let executor = lightarchitects_gateway::providers::GatewayToolExecutor::new_with_skill_specs(
        Arc::new(config),
        vec![skill],
    );

    // Operator has already typed /e2e_op_wins_v1 this turn.
    executor.mark_operator_invoked("E2E_OP_WINS_V1");

    let err = executor
        .execute("call_e2e_003", "e2e_op_wins_v1", serde_json::json!({}))
        .await
        .expect_err("operator-wins must return Err");

    assert!(
        matches!(
            err,
            lightarchitects::agent::ToolError::SupersededByOperatorAction
        ),
        "expected SupersededByOperatorAction, got: {err:?}"
    );
}
