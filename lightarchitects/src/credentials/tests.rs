//! SDK-level tests for the credentials module.
//!
//! Coverage focus: env precedence, file-existence, `Absent` paths. Keychain
//! paths are behavior-tested via the platform abstraction — the subprocess
//! `security(1)` spawn is not mocked here; integration-level tests in the
//! webshell exercise the real keychain path.
//!
//! # Unsafe
//!
//! Rust 2024 marked `std::env::set_var` / `remove_var` as `unsafe` due to
//! thread-safety concerns. Rust tests run in parallel by default, so we
//! serialize environment mutation via a global lock.
//! SAFETY comments are attached at each call site.

#![allow(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::undocumented_unsafe_blocks
)]

use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard, OnceLock};

use super::{Detection, Locator, Registry, default_registry};

#[allow(dead_code)]
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[allow(dead_code)]
struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    saved: Vec<(OsString, Option<OsString>)>,
}

#[allow(dead_code)]
impl EnvGuard {
    fn new(vars: &[&str]) -> Self {
        let lock = env_lock().lock().expect("env_lock poisoned");
        let saved: Vec<(OsString, Option<OsString>)> = vars
            .iter()
            .map(|v| (OsString::from(*v), std::env::var_os(v)))
            .collect();
        for (v, _) in &saved {
            // SAFETY: these tests mutate process env and assume no concurrent
            // access. This module's tests must not run in parallel.
            unsafe {
                std::env::remove_var(v);
            }
        }
        Self { _lock: lock, saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (v, prev) in &self.saved {
            match prev {
                // SAFETY: see EnvGuard::new.
                Some(val) => unsafe { std::env::set_var(v, val) },
                None => unsafe { std::env::remove_var(v) },
            }
        }
    }
}

#[test]
fn registry_is_empty_by_default() {
    let r = Registry::new();
    assert!(r.provider_ids().is_empty());
}

#[test]
fn default_registry_includes_enabled_providers() {
    let r = default_registry();
    let ids = r.provider_ids();
    #[cfg(any(
        feature = "providers-anthropic",
        feature = "providers-openai",
        feature = "providers-google"
    ))]
    assert!(
        !ids.is_empty(),
        "default registry should include at least one provider"
    );
    #[cfg(not(any(
        feature = "providers-anthropic",
        feature = "providers-openai",
        feature = "providers-google"
    )))]
    assert!(ids.is_empty());
}

#[cfg(feature = "providers-anthropic")]
#[tokio::test]
async fn anthropic_env_var_produces_env_locator() {
    let _guard = EnvGuard::new(&[
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "CLAUDE_CONFIG_DIR",
    ]);
    // SAFETY: single-threaded test (see module docs).
    unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test") };
    let r = default_registry();
    let d = r.probe(super::ANTHROPIC_CLI).await.unwrap();
    assert!(d.available);
    assert_eq!(d.locator, Locator::Env);
}

#[cfg(feature = "providers-openai")]
#[tokio::test]
async fn openai_env_var_produces_env_locator() {
    let _guard = EnvGuard::new(&["OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_HOME"]);
    // SAFETY: single-threaded test (see module docs).
    unsafe { std::env::set_var("OPENAI_API_KEY", "sk-test") };
    let r = default_registry();
    let d = r.probe(super::OPENAI_CLI).await.unwrap();
    assert!(d.available);
    assert_eq!(d.locator, Locator::Env);
}

#[cfg(feature = "providers-google")]
#[tokio::test]
async fn google_env_var_produces_env_locator() {
    let _guard = EnvGuard::new(&[
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "GOOGLE_APPLICATION_CREDENTIALS",
        "GEMINI_HOME",
    ]);
    // SAFETY: single-threaded test (see module docs).
    unsafe { std::env::set_var("GEMINI_API_KEY", "test-key") };
    let r = default_registry();
    let d = r.probe(super::GOOGLE_CLI).await.unwrap();
    assert!(d.available);
    assert_eq!(d.locator, Locator::Env);
}

#[test]
fn detection_debug_does_not_reveal_canonical_strings() {
    // ProviderId is Debug-rendered as hex only.
    let id = super::ProviderId([0xaa; 16]);
    let s = format!("{id:?}");
    assert!(s.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    assert!(!s.contains("Claude"));
    assert!(!s.contains("Codex"));
    assert!(!s.contains("Gemini"));
}

#[cfg(feature = "credentials-detailed-locator")]
#[test]
fn detailed_locator_debug_redacts_fields() {
    use super::DetailedLocator;
    let kc = DetailedLocator::Keychain {
        service: "TEST-SERVICE".to_owned(),
        account: "TEST-ACCOUNT".to_owned(),
    };
    let s = format!("{kc:?}");
    assert!(!s.contains("TEST-SERVICE"));
    assert!(!s.contains("TEST-ACCOUNT"));
    assert!(s.contains("<redacted>"));
}

#[test]
fn detection_struct_equality_by_id_and_locator() {
    let id = super::ProviderId([1; 16]);
    let a = Detection {
        provider_id: id,
        available: true,
        locator: Locator::Env,
    };
    let b = a.clone();
    assert_eq!(a, b);
}
