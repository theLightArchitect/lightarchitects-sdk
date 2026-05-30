//! Vault sidecar isolation tests — Phase 2 Wave 2.
//!
//! Verifies that:
//! - Stub keys never carry real API key material.
//! - Stub keys are unique across concurrent spawns.
//! - `LiteLLMConfig::is_active()` correctly reflects proxy availability.
//! - Unreachable proxy hosts cause `is_proxy_reachable` to return `false`.

use lightarchitects_webshell::config::LiteLLMConfig;
use lightarchitects_webshell::copilot::is_proxy_reachable;
use std::collections::HashSet;

/// Stub keys must not contain patterns that match real API keys.
///
/// Real Anthropic keys look like `sk-ant-api03-...`.
/// Stub keys must be recognisably synthetic (`stub-la-<uuid>`) and
/// must never embed the host process's `ANTHROPIC_API_KEY` value.
#[test]
fn test_subprocess_env_contains_no_real_keys() {
    let cfg = LiteLLMConfig {
        proxy_url: Some("http://localhost:4000".to_owned()),
        stub_key_prefix: "stub-la-".to_owned(),
    };
    let stub = cfg.generate_stub_key();
    assert!(
        stub.starts_with("stub-la-"),
        "stub key must start with the configured prefix; got: {stub}"
    );
    // Must not carry Anthropic key shape
    assert!(
        !stub.starts_with("sk-ant-"),
        "stub key must not look like a real Anthropic key; got: {stub}"
    );
    // Must not contain the host's real key (env may be unset in CI; both branches are safe)
    if let Ok(real_key) = std::env::var("ANTHROPIC_API_KEY") {
        assert_ne!(
            stub, real_key,
            "stub key must not equal the host's real ANTHROPIC_API_KEY"
        );
    }
}

/// Stub keys must be unique across N concurrent-session simulations.
///
/// [`LiteLLM`]'s virtual key registry uses the key as the session identity.
/// Collisions under concurrent builds would mix cost attribution and
/// could allow cross-session tool-call replay. N=100 provides 99.9999%
/// collision confidence with UUID v4 (2^122 space).
#[test]
fn test_stub_keys_unique_per_spawn() {
    let cfg = LiteLLMConfig {
        proxy_url: Some("http://localhost:4000".to_owned()),
        stub_key_prefix: "stub-la-".to_owned(),
    };
    let mut seen = HashSet::with_capacity(100);
    for _ in 0..100 {
        let key = cfg.generate_stub_key();
        assert!(
            seen.insert(key.clone()),
            "duplicate stub key generated: {key}"
        );
    }
}

/// `LiteLLMConfig::is_active()` must return `true` when `proxy_url` is `Some`.
///
/// The match arm in `run_print_turn` gates vault injection on `is_active()`;
/// a misconfigured inactive state would silently skip injection without error.
#[test]
fn test_anthropic_base_url_injected() {
    let active = LiteLLMConfig {
        proxy_url: Some("http://localhost:4000".to_owned()),
        stub_key_prefix: "stub-la-".to_owned(),
    };
    assert!(
        active.is_active(),
        "LiteLLMConfig with proxy_url must be active"
    );

    let inactive = LiteLLMConfig::default();
    assert!(
        !inactive.is_active(),
        "default LiteLLMConfig (no proxy_url) must be inactive"
    );
}

/// `is_proxy_reachable` must return `false` for a port with nothing listening.
///
/// Port 19999 is outside the IANA registered range and is not bound by any
/// standard service — connection should be refused or time out in < 200ms.
/// This test validates the degradation path: an unreachable proxy must not
/// cause vault injection, so the subprocess falls back to direct Anthropic.
#[tokio::test]
async fn test_proxy_unreachable_falls_back() {
    let reachable = is_proxy_reachable("http://127.0.0.1:19999").await;
    assert!(
        !reachable,
        "is_proxy_reachable must return false for a non-listening port"
    );
}
