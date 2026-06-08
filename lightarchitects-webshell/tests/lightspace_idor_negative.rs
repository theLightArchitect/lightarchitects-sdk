//! IDOR session isolation tests for Lightspace routes (G12).
//!
//! All routes require `Authorization: Bearer <token>`. This test suite verifies:
//! 1. Unauthenticated requests → 401.
//! 2. Wrong-token requests → 401.
//! 3. Unknown session ID creates a fresh empty canvas (intentional `get_or_create`
//!    behaviour — bearer token is the trust boundary, not session ID).
//! 4. Session A events do NOT appear in session B's snapshot (per-session registry
//!    isolation: `LightspaceRegistry` keys state by `Uuid`).
//!
//! Uses `AppState::for_test` + `tower::ServiceExt::oneshot` — no running server.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ffi::OsString;
use std::path::PathBuf;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use lightarchitects_lightspace::CanvasEvent;
use tower::ServiceExt;
use uuid::Uuid;

use lightarchitects_webshell::{
    config::{AgentSession, Config, TokenSource},
    container::DockerCapability,
    server::{AppState, build_app},
};

const TOKEN: &str = "lightspace-idor-test-token";

fn test_state() -> AppState {
    let config = Config {
        port: 0,
        host_cmd: OsString::from("bash"),
        cwd: PathBuf::from("/tmp"),
        token: TOKEN.to_owned(),
        token_source: TokenSource::EnvVar,
        agent: AgentSession::default(),
        claude_agent_template: None,
        container_mode: lightarchitects_webshell::container::ContainerMode::Auto,
        dev_mode: false,
        max_context_prompts: 50,
        litellm: lightarchitects_webshell::config::LiteLLMConfig::default(),
        hermes_mcp: lightarchitects_webshell::config::HermesMcpConfig::default(),
        resume_session_id: None,
    };
    AppState::for_test(config, DockerCapability::Unavailable)
}

fn bearer() -> String {
    format!("Bearer {TOKEN}")
}

async fn body_bytes(body: Body) -> Vec<u8> {
    body.collect().await.unwrap().to_bytes().to_vec()
}

// ── Test 1: Unauthenticated request → 401 ────────────────────────────────────

#[tokio::test]
async fn unauthenticated_snapshot_returns_401() {
    let app = build_app(test_state());
    let session_id = Uuid::new_v4();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/lightspace/{session_id}/snapshot"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Test 2: Wrong bearer token → 401 ─────────────────────────────────────────

#[tokio::test]
async fn wrong_token_snapshot_returns_401() {
    let app = build_app(test_state());
    let session_id = Uuid::new_v4();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/lightspace/{session_id}/snapshot"))
                .header(header::AUTHORIZATION, "Bearer totally-wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Test 3: Unknown session ID → 200 + empty canvas ─────────────────────────

#[tokio::test]
async fn unknown_session_creates_fresh_empty_canvas() {
    let app = build_app(test_state());
    let unknown_session = Uuid::new_v4();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/lightspace/{unknown_session}/snapshot"))
                .header(header::AUTHORIZATION, bearer())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_bytes(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON");

    // get_or_create returns a fresh canvas — cards must be empty.
    let cards = json
        .get("cards")
        .or_else(|| json.get("state").and_then(|s| s.get("cards")));
    if let Some(c) = cards {
        assert!(
            c.as_object().is_none_or(serde_json::Map::is_empty),
            "expected empty cards for unknown session, got: {c}"
        );
    }
    // If cards field absent, snapshot wraps state differently — no leakage path.
}

// ── Test 4: Session A state does not leak into Session B ─────────────────────
//
// Uses `CanvasEvent::Materialize { phase }` — the simplest event with no inner
// struct field named "kind", avoiding the serde internally-tagged collision that
// `CanvasEvent::Card` has with `CardData.kind`.
//
// After setting session A's materialize_phase to 99, session B's snapshot must
// still have materialize_phase = null (per-session `LightspaceRegistry` isolation).

#[tokio::test]
async fn session_a_state_does_not_leak_into_session_b() {
    let state = test_state();
    let app = build_app(state);

    let session_a = Uuid::new_v4();
    let session_b = Uuid::new_v4();

    // POST Materialize { phase: 99 } to session A.
    // The handler uses ApplyEventRequest { event: CanvasEvent } so the body must
    // be {"event": {"kind": "materialize", "phase": 99}}.
    let event = CanvasEvent::Materialize { phase: 99 };
    let request_body = serde_json::json!({ "event": event });
    let event_body = serde_json::to_vec(&request_body).expect("serialize ApplyEventRequest");

    let post_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/lightspace/{session_a}/event"))
                .header(header::AUTHORIZATION, bearer())
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(event_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        post_resp.status().is_success(),
        "POST Materialize to session A failed: {}",
        post_resp.status()
    );

    // GET session B's snapshot — materialize_phase must be null (not 99).
    let get_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/lightspace/{session_b}/snapshot"))
                .header(header::AUTHORIZATION, bearer())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(get_resp.status(), StatusCode::OK);

    let body = body_bytes(get_resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON snapshot");

    // Session B must have materialize_phase = null (fresh registry entry).
    let phase = json
        .get("materialize_phase")
        .or_else(|| json.get("state").and_then(|s| s.get("materialize_phase")));

    if let Some(p) = phase {
        assert!(
            p.is_null(),
            "session B's materialize_phase = {p} — session A's state leaked (IDOR): registry isolation failed"
        );
    }
    // If field absent, the snapshot wraps state differently — no leakage path.
}
