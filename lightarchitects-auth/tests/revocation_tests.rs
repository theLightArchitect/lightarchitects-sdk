//! Integration tests for [`RevocationWatcher`] — revocation list management and polling.
//!
//! The `/api/revocations` endpoint is mocked per-test using [`mockito::Server`].
//! The revocation list file is isolated to a [`TempDir`].

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_auth::{AuthConfig, RevocationWatcher};
use serde_json::json;
use tempfile::TempDir;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn isolated_config(dir: &TempDir) -> AuthConfig {
    AuthConfig {
        key_file_path: dir.path().join("la-api-key"),
        cache_file_path: dir.path().join("la-key-cache.json"),
        revoked_file_path: dir.path().join("la-revoked"),
        ..AuthConfig::default()
    }
}

// ── is_revoked tests (local file, no network) ─────────────────────────────────

#[test]
fn is_revoked_returns_false_when_no_revocation_file() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    let watcher = RevocationWatcher::new(config);

    assert!(!watcher.is_revoked("la-12345678"));
}

#[test]
fn is_revoked_returns_false_for_unknown_prefix() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    // Write a revocation file with a different prefix.
    std::fs::write(&config.revoked_file_path, "la-99999999\n").expect("write");

    let watcher = RevocationWatcher::new(config);
    assert!(!watcher.is_revoked("la-12345678"));
}

#[test]
fn is_revoked_returns_true_for_known_prefix() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    std::fs::write(&config.revoked_file_path, "la-12345678\nla-87654321\n").expect("write");

    let watcher = RevocationWatcher::new(config);
    assert!(watcher.is_revoked("la-12345678"));
    assert!(watcher.is_revoked("la-87654321"));
}

#[test]
fn revocation_file_ignores_blank_lines() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    std::fs::write(&config.revoked_file_path, "\nla-aabbccdd\n\n").expect("write");

    let watcher = RevocationWatcher::new(config);
    assert!(watcher.is_revoked("la-aabbccdd"));
}

// ── poll tests (network, mockito) ─────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network socket (mockito) — run with --ignored flag"]
async fn poll_adds_new_revocations_to_empty_list() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/revocations")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({"revoked": ["la-aabbccdd", "la-11223344"]}).to_string())
        .create_async()
        .await;

    let dir = TempDir::new().expect("tempdir");
    let config = AuthConfig {
        api_base_url: server.url(),
        ..isolated_config(&dir)
    };
    let watcher = RevocationWatcher::new(config);

    let new_count = watcher.poll().await.expect("poll");
    assert_eq!(new_count, 2);
    assert!(watcher.is_revoked("la-aabbccdd"));
    assert!(watcher.is_revoked("la-11223344"));
}

#[tokio::test]
#[ignore = "requires network socket (mockito) — run with --ignored flag"]
async fn poll_merges_with_existing_revocation_list() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/revocations")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({"revoked": ["la-new00000"]}).to_string())
        .create_async()
        .await;

    let dir = TempDir::new().expect("tempdir");
    let config = AuthConfig {
        api_base_url: server.url(),
        ..isolated_config(&dir)
    };
    // Pre-populate with an existing revoked prefix.
    std::fs::write(&config.revoked_file_path, "la-existing1\n").expect("write");

    let watcher = RevocationWatcher::new(config);
    watcher.poll().await.expect("poll");

    // Both old and new entries must survive the merge.
    assert!(watcher.is_revoked("la-existing1"), "old entry must be kept");
    assert!(watcher.is_revoked("la-new00000"), "new entry must be added");
}

#[tokio::test]
#[ignore = "requires network socket (mockito) — run with --ignored flag"]
async fn poll_returns_zero_on_non_success_response() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/revocations")
        .with_status(503)
        .create_async()
        .await;

    let dir = TempDir::new().expect("tempdir");
    let config = AuthConfig {
        api_base_url: server.url(),
        ..isolated_config(&dir)
    };
    let watcher = RevocationWatcher::new(config);

    let count = watcher.poll().await.expect("503 is graceful");
    assert_eq!(count, 0, "no entries on server error");
}

#[tokio::test]
#[ignore = "requires network socket (mockito) — run with --ignored flag"]
async fn poll_returns_zero_when_revoked_list_is_empty() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/revocations")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"revoked":[]}"#)
        .create_async()
        .await;

    let dir = TempDir::new().expect("tempdir");
    let config = AuthConfig {
        api_base_url: server.url(),
        ..isolated_config(&dir)
    };
    let watcher = RevocationWatcher::new(config);

    let count = watcher.poll().await.expect("empty list");
    assert_eq!(count, 0);
}

// ── clear tests ───────────────────────────────────────────────────────────────

#[test]
fn clear_removes_revocation_file() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    std::fs::write(&config.revoked_file_path, "la-12345678\n").expect("write");
    assert!(config.revoked_file_path.exists());

    let watcher = RevocationWatcher::new(config.clone());
    watcher.clear().expect("clear");

    assert!(
        !config.revoked_file_path.exists(),
        "revocation file should be deleted"
    );
    assert!(
        !watcher.is_revoked("la-12345678"),
        "cleared prefix must not be revoked"
    );
}

#[test]
fn clear_is_idempotent_when_file_absent() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    let watcher = RevocationWatcher::new(config);
    // No file created — should not error.
    watcher.clear().expect("clear on absent file");
}
