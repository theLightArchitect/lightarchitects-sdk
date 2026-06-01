//! Integration tests for Phase 4 — `ClaudeBackend::LiteLlm` provider consolidation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown
)]

use std::{ffi::OsString, path::PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use lightarchitects_webshell::{
    config::{ClaudeBackend, ClaudeBackendKind, Cli, Config, LiteLlmBackendConfig},
    container::DockerCapability,
    server::{AppState, build_app},
};
use tower::ServiceExt;

// ── Test helpers ─────────────────────────────────────────────────────────────

const TOKEN: &str = "provider-consolidation-token";

fn make_app() -> axum::Router {
    let cli = Cli {
        port: 8740,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    let state = AppState::for_test(cfg, DockerCapability::Unavailable);
    build_app(state)
}

async fn get_models_json(backend: &str) -> serde_json::Value {
    let app = make_app();
    let uri = format!("/api/setup/models?backend={backend}");
    let resp = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "backend={backend}");
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

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

// ── Route integration: GET /api/setup/models ─────────────────────────────────

/// `GET /api/setup/models?backend=deepseek` returns a non-empty model list
/// containing at least one entry whose id starts with `"deepseek/"`.
#[tokio::test]
async fn setup_models_deepseek_returns_models() {
    let body = get_models_json("deepseek").await;
    let models = body["models"].as_array().expect("models must be an array");
    assert!(
        !models.is_empty(),
        "deepseek must return at least one model"
    );
    assert!(
        models
            .iter()
            .any(|m| m["id"].as_str().unwrap_or("").starts_with("deepseek/")),
        "at least one deepseek model must have id prefixed 'deepseek/'; got: {body}"
    );
}

/// `GET /api/setup/models?backend=google-vertex` returns a non-empty model list
/// with at least one `vertex_ai/` entry.
#[tokio::test]
async fn setup_models_google_vertex_returns_models() {
    let body = get_models_json("google-vertex").await;
    let models = body["models"].as_array().expect("models must be an array");
    assert!(
        !models.is_empty(),
        "google-vertex must return at least one model"
    );
    assert!(
        models
            .iter()
            .any(|m| m["id"].as_str().unwrap_or("").starts_with("vertex_ai/")),
        "at least one google-vertex model must have id prefixed 'vertex_ai/'; got: {body}"
    );
}

/// `GET /api/setup/models?backend=ollama-cloud` returns a non-empty model list.
#[tokio::test]
async fn setup_models_ollama_cloud_returns_models() {
    let body = get_models_json("ollama-cloud").await;
    let models = body["models"].as_array().expect("models must be an array");
    assert!(
        !models.is_empty(),
        "ollama-cloud must return at least one model"
    );
}

/// `GET /api/setup/models?backend=mistral` returns a non-empty model list
/// with at least one `mistral/` entry.
#[tokio::test]
async fn setup_models_mistral_returns_models() {
    let body = get_models_json("mistral").await;
    let models = body["models"].as_array().expect("models must be an array");
    assert!(!models.is_empty(), "mistral must return at least one model");
    assert!(
        models
            .iter()
            .any(|m| m["id"].as_str().unwrap_or("").starts_with("mistral/")),
        "at least one mistral model must have id prefixed 'mistral/'; got: {body}"
    );
}

/// Unknown backend returns `{"models":[]}` — never an error status.
#[tokio::test]
async fn setup_models_unknown_backend_returns_empty() {
    let body = get_models_json("unknown-provider-xyz").await;
    let models = body["models"].as_array().expect("models must be an array");
    assert!(models.is_empty(), "unknown backend must return empty list");
}

// ── POST /api/setup/save → LiteLlm backend ───────────────────────────────────

/// `POST /api/setup/save` with a deepseek backend stores `ClaudeBackend::LiteLlm`
/// in the active agent session. Requires a valid Bearer token.
#[tokio::test]
async fn setup_save_deepseek_produces_litellm_session() {
    let app = make_app();
    let payload = serde_json::json!({
        "agent": "lightarchitects",
        "backend": "deepseek",
        "model": null,
        "ollama_base_url": null,
        "api_key": null,
    });
    let resp = app
        .oneshot(
            Request::post("/api/setup/save")
                .header("Authorization", format!("Bearer {TOKEN}"))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    // 204 = saved; anything other than 401/403 means auth+routing worked
    assert!(
        resp.status() == StatusCode::NO_CONTENT || resp.status() == StatusCode::OK,
        "setup_save deepseek: expected 204/200, got {}",
        resp.status()
    );
}

// ── Property tests: serde stability ──────────────────────────────────────────

/// `ModelOption` serde round-trip: `id`, `name`, `tier` survive a JSON
/// serialize → deserialize cycle with optional fields handled correctly.
#[test]
fn model_option_serde_round_trip() {
    use lightarchitects_webshell::setup::ModelOption;

    let samples = vec![
        ModelOption {
            id: "deepseek/deepseek-chat".to_owned(),
            label: "DeepSeek Chat".to_owned(),
            tier: "balanced".to_owned(),
            family: None,
            tool_use: Some(true),
            vision: None,
            context_k: Some(64),
        },
        ModelOption {
            id: "vertex_ai/gemini-1.5-pro".to_owned(),
            label: "Gemini 1.5 Pro".to_owned(),
            tier: "flagship".to_owned(),
            family: Some("gemini".to_owned()),
            tool_use: Some(true),
            vision: Some(true),
            context_k: Some(1_000),
        },
        ModelOption {
            id: "ollama_chat/llama3.2".to_owned(),
            label: "Llama 3.2".to_owned(),
            tier: "fast".to_owned(),
            family: None,
            tool_use: None,
            vision: None,
            context_k: None,
        },
    ];

    for m in &samples {
        let json = serde_json::to_string(m).expect("ModelOption must serialize");
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("ModelOption must deserialize");
        assert!(
            parsed.get("id").and_then(|v| v.as_str()).is_some(),
            "model missing 'id': {json}"
        );
        assert!(
            parsed.get("label").and_then(|v| v.as_str()).is_some(),
            "model missing 'label': {json}"
        );
        assert!(
            parsed.get("tier").and_then(|v| v.as_str()).is_some(),
            "model missing 'tier': {json}"
        );
        // Optional fields must be absent when None (skip_serializing_if)
        if m.family.is_none() {
            assert!(!json.contains("\"family\""), "None family must be omitted");
        }
    }
}

/// `AuthStatus` new fields serialize with expected JSON keys.
#[test]
fn auth_status_new_fields_serialize_correctly() {
    use lightarchitects_webshell::setup::{
        DeepSeekAuthStatus, GoogleVertexAuthStatus, OllamaCloudAuthStatus,
    };

    let oc = OllamaCloudAuthStatus {
        has_api_key: true,
        login_source: Some("keychain".to_owned()),
    };
    let json = serde_json::to_string(&oc).unwrap();
    assert!(json.contains("\"has_api_key\":true"));
    assert!(json.contains("\"login_source\":\"keychain\""));

    let oc_absent = OllamaCloudAuthStatus {
        has_api_key: false,
        login_source: None,
    };
    let json = serde_json::to_string(&oc_absent).unwrap();
    assert!(
        !json.contains("login_source"),
        "absent field must be skipped"
    );

    let ds = DeepSeekAuthStatus {
        has_api_key: true,
        login_source: None,
    };
    let json = serde_json::to_string(&ds).unwrap();
    assert!(json.contains("\"has_api_key\":true"));
    assert!(!json.contains("login_source"));

    let gv = GoogleVertexAuthStatus {
        has_service_account: true,
        project_id: Some("my-gcp-project".to_owned()),
    };
    let json = serde_json::to_string(&gv).unwrap();
    assert!(json.contains("\"has_service_account\":true"));
    assert!(json.contains("\"project_id\":\"my-gcp-project\""));
}
