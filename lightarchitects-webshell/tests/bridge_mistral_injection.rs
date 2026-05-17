//! Integration test — `MISTRAL_API_KEY` injection into vibe subprocess environment.
//!
//! Verifies the security-critical invariant: after `env_clear()` + whitelist + vibe
//! conditional injection, the key is present in the child environment and absent from
//! non-vibe environments.
//!
//! Unit tests in `copilot::tests` cover the `resolve_mistral_api_key()` → `SecretString`
//! conversion path. These tests focus on the subprocess injection layer.

#![allow(clippy::unwrap_used, clippy::expect_used, unsafe_code)]

use secrecy::{ExposeSecret, SecretString};

/// Verify that `SecretString::expose_secret()` → `cmd.env()` round-trip
/// produces the correct key in the subprocess environment.
///
/// Simulates the exact `spawn_bridge` env-building sequence:
/// `env_clear` → whitelist → vibe conditional inject → subprocess reads key.
/// Constructs `SecretString` directly to avoid `unsafe` env mutation races with
/// the concurrent `non_vibe_env_does_not_inherit_mistral_api_key` test.
#[tokio::test]
async fn vibe_bridge_injects_mistral_api_key_after_env_clear() {
    let key = SecretString::new("sk-integration-test-key-9876".to_owned().into());

    let mut cmd = tokio::process::Command::new("sh");
    // Mirror spawn_bridge: clear env, apply whitelist, inject key.
    cmd.env_clear();
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    for var in ["HOME", "USER", "SHELL"] {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }
    // Vibe-specific injection — the exact pattern in bridge.rs.
    cmd.env("MISTRAL_API_KEY", key.expose_secret());

    let output = cmd
        .arg("-c")
        .arg("printf '%s' \"$MISTRAL_API_KEY\"")
        .output()
        .await
        .expect("sh must spawn");

    let injected = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        injected.as_ref(),
        "sk-integration-test-key-9876",
        "MISTRAL_API_KEY must survive env_clear + re-inject into subprocess env"
    );
}

/// Verify non-vibe sessions do NOT receive `MISTRAL_API_KEY` via the whitelist.
///
/// The LA_* forwarding loop in bridge.rs passes all `LA_`/`LIGHTARCHITECTS_` vars but
/// not `MISTRAL_API_KEY`. This test confirms the key is stripped by `env_clear` and
/// not re-introduced by the whitelist for non-vibe sessions.
#[tokio::test]
async fn non_vibe_env_does_not_inherit_mistral_api_key() {
    unsafe { std::env::set_var("MISTRAL_API_KEY", "sk-should-not-appear") };

    // Build command without the vibe injection block (simulates non-vibe session).
    let mut cmd = tokio::process::Command::new("sh");
    cmd.env_clear();
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    // Whitelist only — no MISTRAL_API_KEY injection.
    for var in ["HOME", "USER", "SHELL", "RUST_LOG"] {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }
    for (key, val) in std::env::vars() {
        if key.starts_with("LA_") || key.starts_with("LIGHTARCHITECTS_") {
            cmd.env(key, val);
        }
    }

    let output = cmd
        .arg("-c")
        .arg("printf '%s' \"${MISTRAL_API_KEY:-absent}\"")
        .output()
        .await
        .expect("sh must spawn");

    unsafe { std::env::remove_var("MISTRAL_API_KEY") };

    let result = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        result.as_ref(),
        "absent",
        "MISTRAL_API_KEY must be absent from non-vibe subprocess environment"
    );
}
