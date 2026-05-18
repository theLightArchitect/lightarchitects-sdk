//! Pre-flight checks for autonomous build runs.
//!
//! Invoked via `lightarchitects preflight <check>`. Phase 1 stubs return immediately;
//! real implementations land in Phase 3.

use crate::error::GatewayError;

/// Verify sufficient free disk space for autonomous build operations.
///
/// Real implementation (Phase 3): `df -k /` + `du` on target dirs + inode probe.
/// Requirement: ≥60 GB free (R5 empirical — 8 worktrees × 23 GB without shared
/// `CARGO_TARGET_DIR` = 184 GB; with shared dir = 24 GB).
///
/// # Errors
///
/// Returns `GatewayError` when disk space is insufficient or the check cannot run.
/// Phase 1 stub always returns `Ok(())`.
pub fn disk() -> Result<(), GatewayError> {
    println!("preflight disk: ok (stub — Phase 3 implementation)");
    Ok(())
}

/// Verify API key headroom and external service availability.
///
/// Real implementation (Phase 3): Anthropic ping + Ollama ping +
/// `x-ratelimit-remaining-requests` header check (must be > 7 × 3 = 21 for a full wave).
///
/// # Errors
///
/// Returns `GatewayError` when the API is unreachable or rate-limit headroom is too low.
/// Phase 1 stub always returns `Ok(())`.
pub fn api() -> Result<(), GatewayError> {
    println!("preflight api: ok (stub — Phase 3 implementation)");
    Ok(())
}

/// Verify canon documents load successfully and stay within the token budget.
///
/// Real implementation (Phase 3): load all 8 canon docs, tokenise, verify
/// combined count ≤ 80 K (Ironclaw §13 cap).
///
/// # Errors
///
/// Returns `GatewayError` when a canon document is missing or the token count exceeds 80 K.
/// Phase 1 stub always returns `Ok(())`.
pub fn canon() -> Result<(), GatewayError> {
    println!("preflight canon: ok (stub — Phase 3 implementation)");
    Ok(())
}
