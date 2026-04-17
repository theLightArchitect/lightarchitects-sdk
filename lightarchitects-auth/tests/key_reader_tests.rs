//! Integration tests for [`KeyReader`] — API key loading and persistence.
//!
//! Every test uses an isolated [`TempDir`] so file operations never touch
//! `~/lightarchitects/soul/config/`. Env-var tests use [`EnvGuard`] to restore the previous
//! `LA_API_KEY` value on drop regardless of test outcome.

#![allow(clippy::unwrap_used, clippy::expect_used)]
// set_var / remove_var are `unsafe` in edition 2024; no safe alternative exists for
// env-var mutation in tests. The guard pattern below is single-threaded and sound.
#![allow(unsafe_code)]

use lightarchitects_auth::{AuthConfig, AuthError, KeyReader};
use std::sync::Mutex;
use tempfile::TempDir;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Serialize all env-var-touching tests within this binary.
///
/// `cargo test` runs tests in parallel by default. `LA_API_KEY` is process-wide
/// state; concurrent mutation races. Holding this lock pins execution to one
/// env-var test at a time without adding an external dependency.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Build an [`AuthConfig`] rooted inside `dir` so nothing touches `~/.soul`.
fn isolated_config(dir: &TempDir) -> AuthConfig {
    AuthConfig {
        key_file_path: dir.path().join("la-api-key"),
        cache_file_path: dir.path().join("la-key-cache.json"),
        revoked_file_path: dir.path().join("la-revoked"),
        ..AuthConfig::default()
    }
}

/// RAII pair: acquires [`ENV_LOCK`], then sets/removes `LA_API_KEY`.
/// Restores the original value on drop regardless of test outcome.
struct EnvGuard {
    prev: Option<String>,
    // Lock held for the duration of the test — dropped after `prev` is restored.
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn set(val: &str) -> Self {
        let lock = ENV_LOCK.lock().expect("env lock");
        let prev = std::env::var("LA_API_KEY").ok();
        unsafe { std::env::set_var("LA_API_KEY", val) };
        Self { prev, _lock: lock }
    }

    fn remove() -> Self {
        let lock = ENV_LOCK.lock().expect("env lock");
        let prev = std::env::var("LA_API_KEY").ok();
        unsafe { std::env::remove_var("LA_API_KEY") };
        Self { prev, _lock: lock }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.prev {
                Some(v) => std::env::set_var("LA_API_KEY", v),
                None => std::env::remove_var("LA_API_KEY"),
            }
        }
    }
}

// ── Read tests ────────────────────────────────────────────────────────────────

#[test]
fn env_var_is_read_when_set() {
    let _guard = EnvGuard::set("la-test-key-from-env");
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);

    let key = KeyReader::read(&config).expect("read from env");
    assert_eq!(key, "la-test-key-from-env");
}

#[test]
fn file_is_read_when_env_var_absent() {
    let _guard = EnvGuard::remove();
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    KeyReader::save(&config, "la-key-from-file").expect("save");

    let key = KeyReader::read(&config).expect("read from file");
    assert_eq!(key, "la-key-from-file");
}

#[test]
fn env_var_takes_priority_over_file() {
    let _guard = EnvGuard::set("env-wins");
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    KeyReader::save(&config, "file-loses").expect("save");

    let key = KeyReader::read(&config).expect("env wins");
    assert_eq!(key, "env-wins");
}

#[test]
fn empty_env_var_falls_through_to_file() {
    // Empty string → treated as absent → falls back to file.
    let _guard = EnvGuard::set("");
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    KeyReader::save(&config, "fallback-key").expect("save");

    let key = KeyReader::read(&config).expect("fallback to file");
    assert_eq!(key, "fallback-key");
}

#[test]
fn missing_both_returns_no_key_found() {
    let _guard = EnvGuard::remove();
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);

    let err = KeyReader::read(&config).expect_err("must fail");
    assert!(
        matches!(err, AuthError::NoKeyFound { .. }),
        "expected NoKeyFound, got {err:?}"
    );
}

#[test]
fn empty_key_file_returns_no_key_found() {
    let _guard = EnvGuard::remove();
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    std::fs::write(&config.key_file_path, "   \n  ").expect("write empty file");

    let err = KeyReader::read(&config).expect_err("empty file must fail");
    assert!(matches!(err, AuthError::NoKeyFound { .. }));
}

#[test]
fn key_file_whitespace_is_trimmed_on_read() {
    let _guard = EnvGuard::remove();
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    // Manually write with surrounding whitespace — save() would trim it anyway.
    std::fs::write(&config.key_file_path, "  trimmed-key\n").expect("write");

    let key = KeyReader::read(&config).expect("read trimmed");
    assert_eq!(key, "trimmed-key");
}

// ── Save tests ────────────────────────────────────────────────────────────────

#[test]
fn save_creates_key_file() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);

    KeyReader::save(&config, "new-key").expect("save");

    assert!(
        config.key_file_path.exists(),
        "key file should exist after save"
    );
}

#[test]
fn save_and_read_roundtrip() {
    let _guard = EnvGuard::remove();
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);

    KeyReader::save(&config, "round-trip-key").expect("save");
    let key = KeyReader::read(&config).expect("read");
    assert_eq!(key, "round-trip-key");
}

#[test]
fn save_overwrites_existing_key() {
    let _guard = EnvGuard::remove();
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);

    KeyReader::save(&config, "old-key").expect("first save");
    KeyReader::save(&config, "new-key").expect("second save");

    let key = KeyReader::read(&config).expect("read after overwrite");
    assert_eq!(key, "new-key");
}

#[test]
fn save_creates_parent_directories() {
    let dir = TempDir::new().expect("tempdir");
    let mut config = isolated_config(&dir);
    // Nest the key file two levels deeper — parent dirs don't exist yet.
    config.key_file_path = dir.path().join("deep").join("nested").join("la-api-key");

    KeyReader::save(&config, "deep-key").expect("save in nested dir");
    assert!(config.key_file_path.exists());
}

// ── Remove tests ──────────────────────────────────────────────────────────────

#[test]
fn remove_deletes_key_file() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);
    KeyReader::save(&config, "to-delete").expect("save");
    assert!(config.key_file_path.exists());

    KeyReader::remove(&config).expect("remove");

    assert!(!config.key_file_path.exists(), "key file should be gone");
}

#[test]
fn remove_is_idempotent_when_file_absent() {
    let dir = TempDir::new().expect("tempdir");
    let config = isolated_config(&dir);

    // File was never created — should not error.
    KeyReader::remove(&config).expect("remove on absent file");
}
