//! Integration tests for Phase 4 — `ClaudeBackend::LiteLlm` provider consolidation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown
)]

use lightarchitects_webshell::config::{ClaudeBackend, ClaudeBackendKind, LiteLlmBackendConfig};

// ── ClaudeBackend::LiteLlm variant ───────────────────────────────────────────

#[test]
fn test_litellm_backend_kind_is_litellm() {
    let backend = ClaudeBackend::LiteLlm(LiteLlmBackendConfig {
        model: "anthropic/claude-sonnet-4-5".to_owned(),
    });
    assert_eq!(backend.kind(), ClaudeBackendKind::LiteLlm);
}

#[test]
fn test_litellm_backend_config_model_round_trips() {
    let model = "anthropic/claude-opus-4-7";
    let backend = ClaudeBackend::LiteLlm(LiteLlmBackendConfig {
        model: model.to_owned(),
    });
    let ClaudeBackend::LiteLlm(cfg) = backend else {
        panic!("expected LiteLlm variant");
    };
    assert_eq!(cfg.model, model);
}

/// The `LiteLlm` variant must serialize/deserialize round-trip correctly via
/// `serde_json` using the `#[serde(tag = "kind")]` discriminant.
#[test]
fn test_litellm_backend_serde_round_trip() {
    let original = ClaudeBackend::LiteLlm(LiteLlmBackendConfig {
        model: "anthropic/claude-sonnet-4-5".to_owned(),
    });
    let json = serde_json::to_string(&original).expect("serialize must succeed");
    assert!(
        json.contains(r#""kind":"lite_llm""#),
        "serialized JSON must contain snake_case kind discriminant; got: {json}"
    );
    let round_tripped: ClaudeBackend =
        serde_json::from_str(&json).expect("deserialize must succeed");
    let ClaudeBackend::LiteLlm(cfg) = round_tripped else {
        panic!("round-tripped value must be LiteLlm variant");
    };
    assert_eq!(cfg.model, "anthropic/claude-sonnet-4-5");
}

/// The deprecated `Ollama` variant must still deserialize correctly (backwards compat).
#[test]
fn test_ollama_backend_still_deserializes() {
    let json = r#"{"kind":"ollama","base_url":"http://localhost:11434","model":"llama3","auth_token":"ollama"}"#;
    let backend: ClaudeBackend = serde_json::from_str(json).expect("Ollama must still parse");
    assert_eq!(backend.kind(), ClaudeBackendKind::Ollama);
}
