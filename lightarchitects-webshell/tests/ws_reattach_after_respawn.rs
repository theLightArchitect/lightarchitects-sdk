//! `ws_reattach_after_respawn` — Phase 5 integration tests for POST /api/pty/respawn.
//!
//! Covers:
//!   G1 — unauthenticated request → 401 (`AuthGuard` gate).
//!   G2 — unknown agent kind → 422 (serde JSON rejection by Axum).
//!   G3 — missing credential → 412 (cred-before-kill gate).
//!   G-dedup — concurrent respawn → 409 (`Respawning` state guard).
//!   P50 — median round-trip for the 401 fast path < 500 ms.
//!
//! Uses `tower::ServiceExt::oneshot` for shape tests (no TCP socket) and
//! `TcpListener::bind("127.0.0.1:0")` for latency sampling.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf, time::Instant};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use lightarchitects_webshell::{
    auth::credential::CredentialState,
    config::{Cli, Config},
    container::DockerCapability,
    server::{AppState, build_app, pty_respawn::PtyState},
};
use serde_json::json;
use tokio::net::TcpListener;
use tower::ServiceExt;

const TOKEN: &str = "phase-5-respawn-test-token";

fn make_config() -> Config {
    let cli = Cli {
        port: 0,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap()
}

fn make_app() -> axum::Router {
    build_app(AppState::for_test(
        make_config(),
        DockerCapability::Unavailable,
    ))
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

// ── G1: Unauthenticated → 401 ─────────────────────────────────────────────────

#[tokio::test]
async fn respawn_no_auth_returns_401() {
    let resp = make_app()
        .oneshot(
            Request::post("/api/pty/respawn")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "agent": "lightarchitects" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn respawn_wrong_token_returns_401() {
    let resp = make_app()
        .oneshot(
            Request::post("/api/pty/respawn")
                .header("Authorization", bearer("wrong-token"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "agent": "lightarchitects" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── G2: Unknown agent kind → 422 ─────────────────────────────────────────────

#[tokio::test]
async fn respawn_unknown_agent_returns_422() {
    let resp = make_app()
        .oneshot(
            Request::post("/api/pty/respawn")
                .header("Authorization", bearer(TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "agent": "totally_unknown_agent" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum returns 422 Unprocessable Entity for serde deserialization failures.
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn respawn_missing_agent_field_returns_422() {
    let resp = make_app()
        .oneshot(
            Request::post("/api/pty/respawn")
                .header("Authorization", bearer(TOKEN))
                .header("Content-Type", "application/json")
                // send old (incorrect) field name to confirm the rename matters
                .body(Body::from(
                    serde_json::to_vec(&json!({ "agent_kind": "lightarchitects" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ── G3: Missing credential → 412 ─────────────────────────────────────────────

#[tokio::test]
async fn respawn_valid_auth_no_credential_returns_412() {
    // AppState::for_test has an empty credential_store, so every valid agent kind
    // will trigger the G3 cred-before-kill guard (412 Precondition Failed).
    let resp = make_app()
        .oneshot(
            Request::post("/api/pty/respawn")
                .header("Authorization", bearer(TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "agent": "lightarchitects" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
}

#[tokio::test]
async fn respawn_codex_no_credential_returns_412() {
    let resp = make_app()
        .oneshot(
            Request::post("/api/pty/respawn")
                .header("Authorization", bearer(TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "agent": "codex" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
}

// ── G-dedup: concurrent respawn → 409 ────────────────────────────────────────

/// Pre-set `PtyState::Respawning` and send an authenticated request that passes
/// G1, G3, and G2 — confirming the 409 guard fires before any kill/spawn work.
#[tokio::test]
async fn respawn_while_respawning_returns_409() {
    let state = AppState::for_test(make_config(), DockerCapability::Unavailable);

    // Pre-inject an anthropic credential so G3 (412) does not fire first.
    state
        .credential_store
        .insert("anthropic".to_owned(), CredentialState::Connected);

    // Mark state as already Respawning to simulate a concurrent in-flight request.
    *state.pty_state.write().await = PtyState::Respawning;

    let resp = build_app(state)
        .oneshot(
            Request::post("/api/pty/respawn")
                .header("Authorization", bearer(TOKEN))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "agent": "lightarchitects" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

// ── P50 latency: 401 fast path < 500 ms ──────────────────────────────────────

/// Spawns a real HTTP server on a random port and fires 10 unauthenticated
/// POST /api/pty/respawn requests, measuring round-trip duration for each.
///
/// Assertion: P50 (median) < 500 ms.
///
/// The 401 path returns before any PTY or credential work, making it the
/// fastest stable path for measuring server overhead.
#[tokio::test]
async fn respawn_401_path_p50_under_500ms() {
    let state = AppState::for_test(make_config(), DockerCapability::Unavailable);
    let app = build_app(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let base = format!("http://{addr}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap();

    let mut durations_ms: Vec<u128> = Vec::with_capacity(10);

    for _ in 0..10 {
        let t0 = Instant::now();
        let resp = client
            .post(format!("{base}/api/pty/respawn"))
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&json!({ "agent": "lightarchitects" })).unwrap())
            .send()
            .await
            .unwrap();
        let elapsed_ms = t0.elapsed().as_millis();
        assert_eq!(resp.status(), 401u16);
        durations_ms.push(elapsed_ms);
    }

    durations_ms.sort_unstable();
    // Median of 10 samples = midpoint of the 5th and 6th values (0-indexed: [4] and [5]).
    let p50 = u128::midpoint(durations_ms[4], durations_ms[5]);
    assert!(
        p50 < 500,
        "P50 latency ({p50} ms) exceeds 500 ms budget — server overhead regression"
    );
}
