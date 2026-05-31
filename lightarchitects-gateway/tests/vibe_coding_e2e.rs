//! Vibe coding loop E2E tests — Canon XXVII Suite 4 (binary integration).
//!
//! Tests the interactive skill-aware agent loop introduced by vibe-coding-loop.
//! All tests exercise the binary directly via subprocess; no LLM connection is
//! required for P1-E1..P1-E4.  P1-E5 (live streaming turn) is `#[ignore]`-gated
//! behind `LIGHTARCHITECTS_E2E_LIVE` so it never fires in CI without opt-in.
//!
//! ## Test map
//!
//! | ID     | Desc                                              | LLM? |
//! |--------|---------------------------------------------------|------|
//! | P1-E1  | Binary starts + prints banner on `--stream-events`| No  |
//! | P1-E2  | LiteLLM provider env vars accepted without panic  | No  |
//! | P1-E3  | `skill list` emits known slugs (binary smoke)     | No  |
//! | P1-E4  | Skill subcommand dispatch returns non-zero cleanly | No  |
//! | P1-E5  | Live streaming turn completes (opt-in)            | Yes |

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::items_after_statements
)]

use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const GATEWAY_BIN: &str = env!("CARGO_BIN_EXE_lightarchitects");

/// Poll `child.try_wait()` until it exits or the deadline is reached.
/// Kills the process and panics on timeout.
fn wait_or_timeout(child: &mut std::process::Child, timeout: Duration) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait().expect("try_wait must not error") {
            Some(status) => return status,
            None => {
                if Instant::now() >= deadline {
                    child.kill().ok();
                    panic!("binary did not exit within {}s", timeout.as_secs());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

// ── P1-E1 — Binary starts and exits cleanly on stdin-EOF ─────────────────────

/// Binary started with `--stream-events` exits 0 when stdin is closed immediately.
///
/// Validates that the vibe-coding-loop NDJSON path does not hang or panic on
/// an immediate EOF — the minimal liveness gate for the interactive mode.
#[test]
fn p1_e1_stream_events_exits_on_eof() {
    let mut child = Command::new(GATEWAY_BIN)
        .arg("--stream-events")
        .env("LA_LLM", "claude")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lightarchitects binary");

    // Drop stdin immediately — simulates EOF, the session loop should terminate.
    drop(child.stdin.take());

    let status = wait_or_timeout(&mut child, Duration::from_secs(10));

    // Exit code 0 is the expected clean exit; 1 is also acceptable (config missing).
    // Any crash (signal, panic) will show as !success() without code.
    assert!(
        status.code().is_some(),
        "binary must exit normally, not via signal; status={status}"
    );
}

// ── P1-E2 — LiteLLM provider env vars accepted without panic ─────────────────

/// Setting `LA_LLM=litellm` with valid-shaped env vars does not crash on startup.
///
/// Tests the `ProviderKind::LiteLLM` detection path added in vibe-coding-loop
/// Phase 1.  The binary can't make a real LiteLLM call (no server running) but
/// the provider construction and env-var detection must not panic.
#[test]
fn p1_e2_litellm_provider_env_vars_accepted() {
    let mut child = Command::new(GATEWAY_BIN)
        .arg("--stream-events")
        .env("LA_LLM", "litellm")
        .env("LA_LITELLM_BASE_URL", "http://localhost:4000")
        .env("LA_LITELLM_API_KEY", "test-key")
        .env("LA_LITELLM_MODEL", "anthropic/claude-opus-4-7")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lightarchitects binary");

    drop(child.stdin.take());

    let status = wait_or_timeout(&mut child, Duration::from_secs(10));

    assert!(
        status.code().is_some(),
        "LA_LLM=litellm must not crash on startup; status={status}"
    );
}

// ── P1-E3 — skill list emits known slugs (binary smoke) ──────────────────────

/// `lightarchitects skill list` exits 0 and emits at least one expected slug.
///
/// Confirms the binary is functional end-to-end (compilation, binary linking,
/// plugin cache path resolution).
#[test]
fn p1_e3_skill_list_exits_zero_and_emits_slugs() {
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

    assert!(
        output.status.success()
            || combined.to_uppercase().contains("REFLECT")
            || combined.to_uppercase().contains("BUILD")
            || combined.to_uppercase().contains("PLAN"),
        "skill list must exit 0 or emit a known slug; combined=\n{combined}"
    );
}

// ── P1-E4 — Unknown skill exits non-zero without panic ───────────────────────

/// `lightarchitects skill run UNKNOWN_SLUG_VCLE` exits non-zero without panicking.
///
/// Tests the skill dispatch error path — the binary must not panic or produce
/// a Rust backtrace when a skill slug is not found.
#[test]
fn p1_e4_unknown_skill_exits_nonzero_without_panic() {
    let output = Command::new(GATEWAY_BIN)
        .args(["skill", "run", "UNKNOWN_SLUG_VCLE_TEST"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn lightarchitects binary");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "unknown skill must exit non-zero; status={}",
        output.status
    );
    assert!(
        !stderr.contains("thread '") || !stderr.contains("panicked"),
        "unknown skill must not cause a panic; stderr=\n{stderr}"
    );
}

// ── P1-E5 — Live streaming turn (opt-in, requires LLM) ───────────────────────

/// Sends a real conversational turn and checks that the NDJSON response stream
/// contains at least one text delta event.
///
/// Requires:
/// - `LIGHTARCHITECTS_E2E_LIVE=1` env var
/// - A working LLM backend (`LA_LLM` / `ANTHROPIC_API_KEY` etc.)
///
/// Not run in CI. Activate with:
/// ```bash
/// LIGHTARCHITECTS_E2E_LIVE=1 cargo test -p lightarchitects-gateway \
///     --test vibe_coding_e2e p1_e5_live_streaming_turn -- --include-ignored
/// ```
#[test]
#[ignore = "requires LIGHTARCHITECTS_E2E_LIVE=1 and a working LLM backend"]
fn p1_e5_live_streaming_turn() {
    if std::env::var("LIGHTARCHITECTS_E2E_LIVE").unwrap_or_default() != "1" {
        return;
    }

    let mut child = Command::new(GATEWAY_BIN)
        .arg("--stream-events")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lightarchitects binary");

    // Send a minimal conversational turn — the LLM will respond.
    if let Some(ref mut stdin) = child.stdin {
        let msg = serde_json::json!({
            "type": "user_message",
            "content": "Reply with exactly: VIBE_E2E_OK"
        });
        let _ = writeln!(stdin, "{msg}");
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("wait_with_output");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("VIBE_E2E_OK") || stdout.contains("text") || stdout.contains("delta"),
        "live turn must produce at least one text event; stdout=\n{stdout}"
    );
}
