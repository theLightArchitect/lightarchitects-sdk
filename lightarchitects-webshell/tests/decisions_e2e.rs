//! Integration + regression tests for the Phase 6 decisions endpoint and
//! mode field — Canon XXVII Suites 2 (integration) and 5 (regression).
//!
//! Covers:
//! - `GET /api/builds/:id/decisions` — auth, 404, 200 + empty array
//! - `POST /api/builds` mode field — autonomous echo, absent → interactive,
//!   unknown value → interactive (regression pin)
//! - `GET /api/builds/:id` — stored mode survives the registry round-trip
//!
//! Uses `tower::ServiceExt::oneshot` — no TCP bind, no flaky network.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf, sync::Arc};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use lightarchitects_webshell::{
    config::{AgentSession, ClaudeBackend, Cli, Config},
    server::{AppState, build_app},
    session::BuildSession,
};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = "decisions-e2e-test-token";

fn make_state() -> AppState {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    AppState::for_test(
        cfg,
        lightarchitects_webshell::container::DockerCapability::Unavailable,
    )
}

fn make_app() -> axum::Router {
    build_app(make_state())
}

/// App with a pre-registered `BuildSession` at the default mode ("interactive").
fn make_app_with_build() -> (axum::Router, Uuid) {
    let state = make_state();
    let session = Arc::new(BuildSession::new(
        PathBuf::from("/tmp"),
        AgentSession::Lightarchitects(ClaudeBackend::Anthropic),
    ));
    let build_id = session.build_id;
    state.builds.insert(session);
    (build_app(state), build_id)
}

/// App with a pre-registered `BuildSession` with `mode` set to `"autonomous"`.
/// Simulates the registry state after `create_build_handler` processes an
/// autonomous POST body (validates the S1-F2 session.rs persistence fix).
fn make_app_with_autonomous_build() -> (axum::Router, Uuid) {
    let state = make_state();
    let mut session = BuildSession::new(
        PathBuf::from("/tmp"),
        AgentSession::Lightarchitects(ClaudeBackend::Anthropic),
    );
    "autonomous".clone_into(&mut session.mode);
    let build_id = session.build_id;
    state.builds.insert(Arc::new(session));
    (build_app(state), build_id)
}

async fn json_body(resp: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(resp.into_body(), 1024 * 64).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

// ── Suite 2: Integration — GET /api/builds/:id/decisions ─────────────────────

#[tokio::test]
async fn decisions_requires_bearer_auth() {
    let b = Uuid::new_v4();
    let resp = make_app()
        .oneshot(
            Request::get(format!("/api/builds/{b}/decisions"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn decisions_returns_404_for_unknown_build() {
    let b = Uuid::new_v4();
    let resp = make_app()
        .oneshot(
            Request::get(format!("/api/builds/{b}/decisions"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn decisions_returns_empty_array_for_known_build() {
    let (app, build_id) = make_app_with_build();
    let resp = app
        .oneshot(
            Request::get(format!("/api/builds/{build_id}/decisions"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert!(body.is_array(), "decisions must return a JSON array");
    assert_eq!(
        body.as_array().unwrap().len(),
        0,
        "stub must return empty array before decisions.md is populated in Phase 7"
    );
}

// ── Suite 2: Integration — mode field in POST /api/builds ────────────────────

async fn post_build(body_json: &str) -> (StatusCode, serde_json::Value) {
    let resp = make_app()
        .oneshot(
            Request::post("/api/builds")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(body_json.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let body = json_body(resp).await;
    (status, body)
}

#[tokio::test]
async fn create_build_mode_autonomous_echoes_autonomous() {
    let (status, body) = post_build(r#"{"cwd":"/tmp","mode":"autonomous"}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["mode"].as_str(),
        Some("autonomous"),
        "BuildResponse must echo the resolved mode"
    );
}

#[tokio::test]
async fn create_build_mode_absent_defaults_to_interactive() {
    let (status, body) = post_build(r#"{"cwd":"/tmp"}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["mode"].as_str(),
        Some("interactive"),
        "absent mode field must default to interactive"
    );
}

// ── Suite 5: Regression — mode normalisation ──────────────────────────────────

#[tokio::test]
async fn create_build_unknown_mode_normalises_to_interactive() {
    // Any unrecognised mode string must collapse to "interactive" — never
    // leak the raw value into BuildResponse (regression pin for the match
    // normalisation in create_build_handler).
    let (status, body) = post_build(r#"{"cwd":"/tmp","mode":"turbo-supremo"}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["mode"].as_str(),
        Some("interactive"),
        "unrecognised mode must normalise to interactive, not leak raw value"
    );
}

// ── Suite 2: Integration — build_details_handler echoes stored mode ───────────

#[tokio::test]
async fn build_details_echoes_stored_mode() {
    // Validates the S1-F2 fix: build_details_handler must read mode from the
    // registry, not hardcode "interactive". Uses a pre-registered session with
    // mode="autonomous" to isolate the handler from create_build_handler.
    let (app, build_id) = make_app_with_autonomous_build();
    let resp = app
        .oneshot(
            Request::get(format!("/api/builds/{build_id}"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(
        body["mode"].as_str(),
        Some("autonomous"),
        "build_details_handler must echo stored mode, not hardcode 'interactive'"
    );
}
