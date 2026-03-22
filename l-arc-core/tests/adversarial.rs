//! Adversarial integration tests for `l-arc-core`.
//!
//! These tests verify the SDK's resilience to malformed, oversized, and
//! protocol-violating inputs at a level above individual framing functions.
//! They complement the unit-level framing tests in `src/transport.rs`.
//!
//! # What is tested here
//!
//! - **Binary path shell-injection safety**: `StdioTransport::connect` uses
//!   `tokio::process::Command::new(path)` which calls `execve(2)` directly,
//!   never passing the path through a shell.  A path containing shell
//!   metacharacters (`;`, `&&`, `|`) is treated as a literal filename —
//!   the OS returns `ENOENT` and the SDK surfaces `TransportError::ProcessSpawn`.
//!
//! - **Protocol constant verification**: Sanity-checks that the hard limits
//!   (`MAX_RESPONSE_BYTES`, `MAX_CONTENT_LENGTH_HEADERS`) match their
//!   documented values.
//!
//! - **Deserialization boundary audit**: Annotates all `serde_json::from_str` /
//!   `from_value` call sites reached from public API so that fuzzing targets are
//!   self-documenting.

use std::time::Duration;

use l_arc_core::constants::{MAX_CONTENT_LENGTH_HEADERS, MAX_RESPONSE_BYTES};
use l_arc_core::error::{SdkError, TransportError};
use l_arc_core::sibling::SiblingId;
use l_arc_core::transport::StdioTransport;

// ── Binary path injection safety ─────────────────────────────────────────────

/// `StdioTransport::connect` must not execute shell metacharacters embedded in
/// a binary path.
///
/// If the SDK used `sh -c <path>` internally, a path like
/// `/nonexistent; echo PWNED` would execute the `echo` command.  Because we
/// use `tokio::process::Command::new(path)` → `execve(2)`, the entire string
/// is a literal path argument.  The OS returns `ENOENT`, which surfaces as
/// `TransportError::ProcessSpawn`.
///
/// This test DOES NOT spawn a child process — it verifies that the spawn
/// attempt fails at the OS level for a non-existent path.
#[tokio::test]
async fn binary_path_shell_metacharacters_cannot_execute() {
    // Path with shell injection payload. If execve'd literally, the OS
    // looks for a file literally named "/nonexistent; echo PWNED" — not found.
    let malicious = std::path::Path::new("/nonexistent-binary; echo PWNED");
    let result = StdioTransport::connect(SiblingId::Soul, malicious, Duration::from_secs(1)).await;
    assert!(
        matches!(
            result,
            Err(SdkError::Transport(TransportError::ProcessSpawn { .. }))
        ),
        "shell-metacharacter path must fail at ProcessSpawn, not execute"
    );
}

/// A binary path with null bytes must also fail at `ProcessSpawn` — the OS
/// rejects paths containing NUL (paths are NUL-terminated C strings).
#[tokio::test]
async fn binary_path_with_null_byte_fails_cleanly() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    // Paths containing NUL cannot be opened by the OS.
    // tokio::process::Command will return an error before execve is attempted.
    let path_str = "/tmp/soul\0evil";
    // SAFETY: We create a Path from raw bytes to embed the NUL byte.
    // This path is rejected by the OS before any execution occurs.
    let os_str = OsStr::from_bytes(path_str.as_bytes());
    let path = std::path::Path::new(os_str);

    let result = StdioTransport::connect(SiblingId::Soul, path, Duration::from_secs(1)).await;
    assert!(
        matches!(
            result,
            Err(SdkError::Transport(TransportError::ProcessSpawn { .. }))
        ),
        "NUL-byte path must fail at ProcessSpawn"
    );
}

// ── Protocol constant verification ────────────────────────────────────────────

/// `MAX_RESPONSE_BYTES` must be exactly 10 MiB.
///
/// Any change to this constant is a breaking API/security change and should be
/// intentional.
#[test]
fn max_response_bytes_is_ten_mib() {
    const EXPECTED: usize = 10 * 1024 * 1024;
    assert_eq!(
        MAX_RESPONSE_BYTES, EXPECTED,
        "MAX_RESPONSE_BYTES changed — verify threat model D1/D2 is still satisfied"
    );
}

/// `MAX_CONTENT_LENGTH_HEADERS` must be 32.
///
/// Real MCP servers (SERAPH) emit 1–2 headers before the blank line.  32 is
/// generous headroom.  Any increase should be accompanied by a threat-model
/// review (D3).
#[test]
fn max_content_length_headers_is_32() {
    assert_eq!(
        MAX_CONTENT_LENGTH_HEADERS, 32,
        "MAX_CONTENT_LENGTH_HEADERS changed — verify threat model D3 is still satisfied"
    );
}

// ── Deserialization boundary audit ────────────────────────────────────────────
//
// All `serde_json::from_str` / `from_value` call sites in `l-arc-core` that
// are reachable from public API are listed here for fuzzing-target visibility.
// Each site is already guarded by a typed error mapping — this module serves
// as a living inventory.
//
// Call site 1: `transport::read_response`
//   serde_json::from_str::<JsonRpcResponse>(json.trim())
//   Guard: maps Err → ProtocolError::MalformedJson
//
// Call site 2: `transport::read_newline_frame`
//   String::from_utf8(buf) — guards against non-UTF8 before JSON parse
//   Guard: maps Err → ProtocolError::MalformedJson
//
// Call site 3: `transport::read_content_length_frame`
//   String::from_utf8(buf) — same guard
//   Guard: maps Err → ProtocolError::MalformedJson
//
// Call site 4: `client::McpClient::list_tools`
//   serde_json::from_value::<ToolsListResult>(value)
//   Guard: maps Err → SdkError::Serialization
//
// Call site 5: `jsonrpc::JsonRpcResponse::into_result`
//   Not a parse — extracts already-decoded `serde_json::Value` fields.
//   No additional deserialization boundary.
//
// Fuzz priority: Call sites 1, 2, 3 are highest risk (external binary input).
// Call sites 4, 5 receive already-validated `serde_json::Value` — lower risk.
//
// Recommended fuzzing approach: cargo-fuzz / libFuzzer targeting
// `read_newline_frame` and `read_content_length_frame` with arbitrary byte slices.
// These functions are now generic over `AsyncBufRead + Unpin`, so a fuzz harness
// can use `tokio::io::BufReader<&[u8]>` without spawning any process.

/// Dummy test to keep this module non-empty from the test runner's perspective.
/// The deserialization audit above is the primary deliverable of this module.
#[test]
fn deserialization_boundary_audit_complete() {
    // 5 call sites identified, all guarded with typed errors.
    // Fuzz targets: call sites 1, 2, 3 (external binary input).
    // See module-level doc comment for full inventory.
}
