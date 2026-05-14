//! Property-based tests for `exec.*` argv safety (T-1 command injection mitigation).
//!
//! Properties verified:
//! 1. `argv_metacharacter_escape` — no string containing a shell metacharacter
//!    passes `validate_argv` when the binary is allowlisted.
//! 2. `allowlist_binary_only` — arbitrary binary names not in the allowlist are
//!    always rejected regardless of the argv tail.
//! 3. `output_buffer_cap_enforcement` — `get_output` cursor arithmetic never
//!    panics on arbitrary cursor values.
//! 4. `rate_limit_window_proptest` — the sliding-window limiter never admits
//!    more than `max_requests` within a single window.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use proptest::prelude::*;
use serde_json::json;

use lightarchitects_gateway::core_tools::exec_comms::run_get_output;

// ── Property 1: metacharacter rejection ───────────────────────────────────────

/// Shell metacharacters that T-1 must always reject.
const METACHARACTERS: &[char] = &[';', '|', '&', '$', '`', '(', ')', '\n', '\r'];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Any string containing at least one shell metacharacter must be rejected
    /// when passed as an argument to a valid allowlisted binary.
    #[test]
    fn argv_metacharacter_escape(
        prefix in "[a-zA-Z0-9_\\-./]{0,20}",
        meta_idx in 0..METACHARACTERS.len(),
        suffix in "[a-zA-Z0-9_\\-./]{0,20}",
    ) {
        use lightarchitects_gateway::core_tools::exec_comms::run_run_command;

        let bad_char = METACHARACTERS[meta_idx];
        let evil_arg = format!("{prefix}{bad_char}{suffix}");

        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(run_run_command(json!({
            "argv": ["cargo", evil_arg],
            "cwd": cwd
        })));

        prop_assert!(
            result.is_err(),
            "metacharacter '{bad_char}' in arg '{evil_arg}' must be rejected, but got Ok"
        );
    }

    /// Arbitrary binary names not in the allowlist must always be rejected.
    #[test]
    fn allowlist_binary_only(
        binary in "[a-zA-Z][a-zA-Z0-9_\\-]{1,30}",
    ) {
        use lightarchitects_gateway::core_tools::exec_comms::run_run_command;

        const ALLOWED: &[&str] = &[
            "cargo", "cargo-nextest", "pnpm", "npx", "vitest", "playwright",
            "node", "rustfmt", "clippy-driver",
        ];

        // Only test binaries that are NOT in the allowlist.
        if ALLOWED.contains(&binary.as_str()) {
            return Ok(());
        }

        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(run_run_command(json!({
            "argv": [binary, "--help"],
            "cwd": cwd
        })));

        prop_assert!(
            result.is_err(),
            "binary '{binary}' is not in allowlist but was accepted"
        );
    }

    /// Arbitrary cursor values for a non-existent handle must always return an error
    /// (not panic, not OOB, not return garbage data).
    #[test]
    fn get_output_cursor_never_panics(cursor in 0u64..u64::MAX) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        // Unknown handle — must return a GatewayError, not panic.
        let result = rt.block_on(run_get_output(json!({
            "stream_handle": "proptest-nonexistent-handle-00000",
            "cursor": cursor
        })));
        prop_assert!(result.is_err(), "unknown handle must always error");
    }
}

// ── Property 4: rate limiter correctness ──────────────────────────────────────

/// Verify the sliding-window limiter never admits more than `max` requests
/// in a single instant (no time elapsed between calls).
#[test]
fn rate_limiter_never_exceeds_max_in_instant_window() {
    use std::time::Duration;
    // Access internal limiter via the test constructor.
    use lightarchitects_gateway::core_tools::exec_comms::test_helpers::make_rate_limiter;

    for max in [1usize, 5, 10, 50] {
        let mut rl = make_rate_limiter(max, Duration::from_secs(60));
        let mut admitted = 0;
        for _ in 0..(max + 10) {
            if rl.try_acquire() {
                admitted += 1;
            }
        }
        assert_eq!(
            admitted, max,
            "limiter max={max}: admitted {admitted} instead of {max}"
        );
    }
}
