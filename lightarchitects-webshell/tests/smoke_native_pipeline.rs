//! Smoke tests — Canon XXVII Suite 6.
//!
//! Verifies that the native SSE pipeline routes are wired, the `AppState` boots
//! cleanly, and `POST /api/builds/:id/copilot` returns `200 text/event-stream`
//! when a registered `LightarchitectsNative` session is present.
//!
//! These tests do NOT exercise the live Ollama Cloud connection; the SSE response
//! headers are set before the spawned turn task makes any network call, so the
//! 200 + `text/event-stream` assertion is deterministic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf, sync::Arc};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use lightarchitects_webshell::{
    config::{AgentSession, Cli, Config, LightarchitectsNativeConfig},
    container::DockerCapability,
    server::{AppState, build_app},
    session::BuildSession,
};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = "smoke-native-token";

/// Create an app with one pre-registered LA-native build session.
/// Returns `(router, build_id)`.
fn make_native_app() -> (axum::Router, Uuid) {
    let cli = Cli {
        port: 8735,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    let state = AppState::for_test(cfg, DockerCapability::Unavailable);

    let session = Arc::new(BuildSession::new(
        PathBuf::from("/tmp"),
        AgentSession::LightarchitectsNative(LightarchitectsNativeConfig::default()),
    ));
    let build_id = session.build_id;
    state.builds.insert(Arc::clone(&session));

    (build_app(state), build_id)
}

// ── Suite 6: Smoke ─────────────────────────────────────────────────────────

#[tokio::test]
async fn smoke_native_app_boots_without_panic() {
    let _ = make_native_app();
}

#[tokio::test]
async fn smoke_native_copilot_returns_200_sse_content_type() {
    let (app, build_id) = make_native_app();
    let body = serde_json::json!({"message": "ping", "recent_events": []});

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{build_id}/copilot"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "native copilot must return 200 SSE"
    );
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("text/event-stream"),
        "native copilot response must be SSE; got content-type: {ct}"
    );
}

#[tokio::test]
async fn smoke_native_interrupt_wired_for_registered_session() {
    let (app, build_id) = make_native_app();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{build_id}/copilot/interrupt"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // 204 No Content — interrupt is a fire-and-forget signal with no body
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "interrupt must return 204 for a registered session"
    );
}

#[tokio::test]
async fn smoke_native_copilot_requires_auth() {
    let id = Uuid::new_v4();
    let body = serde_json::json!({"message": "hi", "recent_events": []});

    let resp = make_native_app()
        .0
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/builds/{id}/copilot"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "copilot route must be authenticated (not 404 = missing route, not 500 = crash)"
    );
}
