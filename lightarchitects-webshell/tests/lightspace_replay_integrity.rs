//! Lightspace HMAC chain integrity tests.
//!
//! Verifies that:
//! 1. Appending 3 events and calling `verify_chain` returns `Ok(true)`.
//! 2. Corrupting one byte in the second log line causes `verify_chain` to
//!    return `Ok(false)` or `Err` — detecting the tamper.
//!
//! Uses `HOME` redirection to a `TempDir` for full filesystem isolation:
//! `path_safety::session_dir` reads `$HOME`, so pointing it at a temp
//! directory keeps test artefacts out of `~/.lightarchitects/lightspace/`.
//!
//! Tests are serialized via `HOME_LOCK` to prevent concurrent HOME mutation
//! across threads in the same test binary.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use chrono::Utc;
use lightarchitects_webshell::events::{
    envelope::{Severity, WebEventV2},
    types::WebEvent,
};
use lightarchitects_webshell::lightspace::{
    hmac_seed::new_seed,
    persist::{append, verify_chain},
};
use std::fs;
use std::sync::Mutex;
use tempfile::TempDir;
use uuid::Uuid;

/// Serialize all HOME-mutating tests in this binary.
///
/// WHY: `std::env::set_var` is unsafe in multi-threaded contexts. Holding this
/// lock for the entire `with_temp_home` window bounds the HOME-visible region
/// to a single thread at a time, preventing race conditions between concurrent
/// test threads within this binary.
static HOME_LOCK: Mutex<()> = Mutex::new(());

/// Redirect `HOME` to `tmp` for the duration of `f`, then restore it.
///
/// Holds `HOME_LOCK` for the entire window. `set_var` is marked unsafe in
/// Rust 2024 for multi-threaded programs; the lock above ensures only one
/// thread mutates HOME at a time in this test binary.
///
/// `#[allow(unsafe_code)]` WHY: `std::env::set_var` is unsafe in Rust 2024.
/// The `HOME_LOCK` above serializes access, providing the single-writer
/// guarantee that makes this safe within this test binary.
#[allow(unsafe_code)]
fn with_temp_home<F: FnOnce()>(tmp: &TempDir, f: F) {
    let _guard = HOME_LOCK.lock().expect("HOME_LOCK poisoned");
    let original = std::env::var("HOME").unwrap_or_default();
    // SAFETY: HOME_LOCK is held; no other thread reads or writes HOME concurrently.
    unsafe {
        std::env::set_var("HOME", tmp.path());
    }
    f();
    // SAFETY: restoring HOME — same lock guarantee.
    unsafe {
        std::env::set_var("HOME", original);
    }
}

/// Build a minimal `WebEventV2` for test purposes.
fn make_event(label: &str) -> WebEventV2 {
    WebEventV2 {
        topic: format!("v1.lightspace.integrity.{label}"),
        timestamp: Utc::now(),
        agent_id: "integrity-test".to_owned(),
        build_id: None,
        severity: Severity::Info,
        inner: WebEvent::GatewayNotify {
            payload: serde_json::json!({ "label": label }),
        },
    }
}

/// Appending 3 events produces a valid HMAC chain.
#[test]
fn three_appends_verify_clean() {
    let tmp = TempDir::new().expect("tempdir");
    let session_id = Uuid::new_v4();
    let seed = new_seed();

    with_temp_home(&tmp, || {
        let mut prev = [0u8; 32];

        prev = append(session_id, &seed, 0, &prev, &make_event("alpha")).expect("append 0 failed");
        prev = append(session_id, &seed, 1, &prev, &make_event("beta")).expect("append 1 failed");
        let _ = append(session_id, &seed, 2, &prev, &make_event("gamma")).expect("append 2 failed");

        let ok = verify_chain(session_id, &seed).expect("verify_chain error");
        assert!(ok, "chain should be valid after 3 clean appends");
    });
}

/// Corrupting one byte in the second log line causes chain verification to fail.
#[test]
fn corrupted_second_line_fails_verify() {
    let tmp = TempDir::new().expect("tempdir");
    let session_id = Uuid::new_v4();
    let seed = new_seed();

    with_temp_home(&tmp, || {
        let mut prev = [0u8; 32];

        prev = append(session_id, &seed, 0, &prev, &make_event("alpha")).expect("append 0 failed");
        prev = append(session_id, &seed, 1, &prev, &make_event("beta")).expect("append 1 failed");
        let _ = append(session_id, &seed, 2, &prev, &make_event("gamma")).expect("append 2 failed");

        // Locate the log file and corrupt one byte in the second NDJSON line.
        // WHY: use the tempdir-relative path directly (not session_dir) so we
        // don't need to call session_dir again while HOME is still set.
        let log_path = tmp
            .path()
            .join(".lightarchitects")
            .join("lightspace")
            .join(session_id.to_string())
            .join("events.ndjson");

        let raw = fs::read(&log_path).expect("read log");
        let content = String::from_utf8(raw).expect("utf8 log");

        let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
        assert!(
            lines.len() >= 2,
            "expected at least 2 log lines, got {}",
            lines.len()
        );

        // Replace the first lowercase hex digit ('a'–'f') on line[1] with 'X'.
        // WHY: 'X' is invalid hex — `hex_decode` in verify_chain will return Err,
        // confirming tamper detection regardless of HMAC comparison order.
        let corrupted = lines[1].replacen(
            |c: char| c.is_ascii_lowercase() && "abcdef".contains(c),
            "X",
            1,
        );
        lines[1] = corrupted;

        let tampered = lines.join("\n") + "\n";
        fs::write(&log_path, tampered.as_bytes()).expect("write tampered log");

        // verify_chain must detect the tamper: either Ok(false) or Err.
        let result = verify_chain(session_id, &seed);
        // Ok(false) = HMAC mismatch; Err = hex decode failure — both confirm detection.
        if let Ok(true) = result {
            panic!(
                "verify_chain returned Ok(true) after tampering — chain did not detect corruption"
            );
        }
    });
}

/// An empty log (no events appended) verifies as Ok(true).
#[test]
fn empty_log_verifies_clean() {
    let tmp = TempDir::new().expect("tempdir");
    let session_id = Uuid::new_v4();
    let seed = new_seed();

    with_temp_home(&tmp, || {
        // No appends — log file does not exist yet.
        let ok = verify_chain(session_id, &seed).expect("verify_chain error on empty");
        assert!(ok, "empty (non-existent) log should verify as true");
    });
}

/// A single append followed by immediate verify returns Ok(true).
#[test]
fn single_append_verifies_clean() {
    let tmp = TempDir::new().expect("tempdir");
    let session_id = Uuid::new_v4();
    let seed = new_seed();

    with_temp_home(&tmp, || {
        let prev = [0u8; 32];
        let _ = append(session_id, &seed, 0, &prev, &make_event("solo")).expect("append failed");

        let ok = verify_chain(session_id, &seed).expect("verify_chain error");
        assert!(ok, "single-append chain should verify clean");
    });
}
