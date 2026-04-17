//! Integration tests for [`KeyValidator`] — cache logic, grace period, and HTTP validation.
//!
//! HTTP endpoint calls are intercepted by a [`mockito::Server`] started per-test.
//! The `api_base_url` in [`AuthConfig`] is overridden to point at the mock server,
//! so no real network traffic leaves the machine.
//!
//! Cache files are written as raw JSON (rather than constructing [`KeyCache`] directly)
//! to verify the full serde round-trip path the validator uses at runtime.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_auth::{AuthConfig, AuthError, AuthTier, KeyValidator};
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

/// Write a valid, unexpired cache entry for `key` directly to `config.cache_file_path`.
fn write_valid_cache(config: &AuthConfig, key: &str) {
    let hash = KeyValidator::hash_key(key);
    let cache = json!({
        "key_hash": hash,
        "tier": "free",
        "user_id": "user_test_123",
        "validated_at": "2026-01-01T00:00:00Z",
        "expires_at": "9999-12-31T23:59:59Z",   // far future — never expires in tests
        "grace_resets": 0
    });
    std::fs::write(&config.cache_file_path, cache.to_string()).expect("write cache");
}

/// Write an expired cache entry (expires_at in the past).
fn write_expired_cache(config: &AuthConfig, key: &str, grace_resets: u8) {
    let hash = KeyValidator::hash_key(key);
    let cache = json!({
        "key_hash": hash,
        "tier": "free",
        "user_id": "user_test_123",
        "validated_at": "2020-01-01T00:00:00Z",
        "expires_at": "2020-01-02T00:00:00Z",   // past — expired
        "grace_resets": grace_resets
    });
    std::fs::write(&config.cache_file_path, cache.to_string()).expect("write expired cache");
}

// ── Hash tests (synchronous, no I/O) ─────────────────────────────────────────

#[test]
fn hash_key_is_deterministic() {
    let h1 = KeyValidator::hash_key("my-api-key");
    let h2 = KeyValidator::hash_key("my-api-key");
    assert_eq!(h1, h2);
}

#[test]
fn hash_key_different_inputs_produce_different_hashes() {
    let h1 = KeyValidator::hash_key("key-a");
    let h2 = KeyValidator::hash_key("key-b");
    assert_ne!(h1, h2);
}

#[test]
fn hash_key_output_is_hex_sha256() {
    let hash = KeyValidator::hash_key("test");
    // SHA-256 output is 32 bytes → 64 hex chars.
    assert_eq!(hash.len(), 64, "SHA-256 hex must be 64 chars");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "must be hex");
}

// ── Cache-hit tests (no network call) ────────────────────────────────────────

#[tokio::test]
async fn valid_unexpired_cache_returns_valid_without_network() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    write_valid_cache(&config, "cached-key");

    // Point api_base_url at an unreachable address — any real HTTP call would fail.
    let config = AuthConfig {
        api_base_url: "http://127.0.0.1:1".to_string(),
        ..config
    };
    let validator = KeyValidator::new(config);

    let (tier, cache) = validator
        .validate("cached-key")
        .await
        .expect("should use cache");
    assert_eq!(tier, AuthTier::Valid);
    assert_eq!(cache.user_id, "user_test_123");
}

#[test]
fn clear_cache_removes_file() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    write_valid_cache(&config, "key");
    assert!(config.cache_file_path.exists());

    let validator = KeyValidator::new(config.clone());
    validator.clear_cache().expect("clear");

    assert!(
        !config.cache_file_path.exists(),
        "cache file should be deleted"
    );
}

#[test]
fn clear_cache_is_idempotent_when_absent() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    let validator = KeyValidator::new(config);
    // No cache file was ever written — should not error.
    validator.clear_cache().expect("clear on absent file");
}

// ── Network validation tests ──────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires network socket (mockito) — run with --ignored flag"]
async fn valid_key_response_writes_cache_and_returns_valid() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/api/validate-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"valid":true,"tier":"free","user_id":"user_from_server"}"#)
        .create_async()
        .await;

    let dir = TempDir::new().expect("tempdir");
    let config = AuthConfig {
        api_base_url: server.url(),
        ..isolated_config(&dir)
    };
    let validator = KeyValidator::new(config.clone());

    let (tier, cache) = validator
        .validate("live-key")
        .await
        .expect("should succeed");
    assert_eq!(tier, AuthTier::Valid);
    assert_eq!(cache.user_id, "user_from_server");
    assert!(
        config.cache_file_path.exists(),
        "validator must write cache on successful validation"
    );
}

#[tokio::test]
#[ignore = "requires network socket (mockito) — run with --ignored flag"]
async fn invalid_key_response_returns_validation_failed() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/api/validate-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"valid":false,"error":"Key not found or expired"}"#)
        .create_async()
        .await;

    let dir = TempDir::new().expect("tempdir");
    let config = AuthConfig {
        api_base_url: server.url(),
        ..isolated_config(&dir)
    };
    let validator = KeyValidator::new(config);

    let err = validator.validate("bad-key").await.expect_err("must fail");
    assert!(
        matches!(err, AuthError::ValidationFailed(_)),
        "expected ValidationFailed, got {err:?}"
    );
}

#[tokio::test]
#[ignore = "requires network socket (mockito) — run with --ignored flag"]
async fn expired_cache_re_validates_against_endpoint() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("POST", "/api/validate-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"valid":true,"tier":"pro","user_id":"refreshed_user"}"#)
        .create_async()
        .await;

    let dir = TempDir::new().expect("tempdir");
    let config = AuthConfig {
        api_base_url: server.url(),
        ..isolated_config(&dir)
    };
    // Write an expired cache — should trigger re-validation.
    write_expired_cache(&config, "expiring-key", 0);

    let validator = KeyValidator::new(config);
    let (tier, cache) = validator
        .validate("expiring-key")
        .await
        .expect("re-validate");
    assert_eq!(tier, AuthTier::Valid);
    assert_eq!(cache.user_id, "refreshed_user");
}

// ── Grace period tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn endpoint_unreachable_with_valid_cache_grants_grace_period() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    // Write an expired cache (grace_resets=0, max_grace_resets defaults to 3).
    write_expired_cache(&config, "grace-key", 0);

    // Unreachable endpoint → reqwest returns Err immediately (connection refused).
    let config = AuthConfig {
        api_base_url: "http://127.0.0.1:1".to_string(),
        ..config
    };
    let validator = KeyValidator::new(config);

    let (tier, _cache) = validator
        .validate("grace-key")
        .await
        .expect("grace should be granted");
    assert!(
        matches!(
            tier,
            AuthTier::GracePeriod {
                resets_remaining: 2
            }
        ),
        "expected GracePeriod(2), got {tier:?}"
    );
}

#[tokio::test]
async fn grace_period_tracks_resets_remaining() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    // Already consumed 1 reset.
    write_expired_cache(&config, "grace-key-2", 1);

    let config = AuthConfig {
        api_base_url: "http://127.0.0.1:1".to_string(),
        ..config
    };
    let validator = KeyValidator::new(config);

    let (tier, _) = validator.validate("grace-key-2").await.expect("grace");
    assert!(
        matches!(
            tier,
            AuthTier::GracePeriod {
                resets_remaining: 1
            }
        ),
        "expected 1 reset remaining, got {tier:?}"
    );
}

#[tokio::test]
async fn grace_period_exhausted_returns_error() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    // grace_resets == max_grace_resets (default 3) → exhausted.
    write_expired_cache(&config, "exhausted-key", 3);

    let config = AuthConfig {
        api_base_url: "http://127.0.0.1:1".to_string(),
        ..config
    };
    let validator = KeyValidator::new(config);

    let err = validator
        .validate("exhausted-key")
        .await
        .expect_err("must be exhausted");
    assert!(
        matches!(err, AuthError::GraceExhausted { max: 3 }),
        "expected GraceExhausted, got {err:?}"
    );
}

#[tokio::test]
async fn no_cache_with_endpoint_down_returns_original_http_error() {
    let dir = TempDir::new().expect("tempdir");
    // No cache file written — grace period cannot be granted.
    let config = AuthConfig {
        api_base_url: "http://127.0.0.1:1".to_string(),
        ..isolated_config(&dir)
    };
    let validator = KeyValidator::new(config);

    let err = validator
        .validate("no-cache-key")
        .await
        .expect_err("must fail");
    // Without a cache to fall back to, the original Http error is returned.
    assert!(
        matches!(err, AuthError::Http(_)),
        "expected Http error, got {err:?}"
    );
}
