//! Phase 9.8–9.10 real-data route smoke tests.
//!
//! Supersedes the prior "stub" tests — handlers in `src/real_data.rs` now
//! read from live filesystem sources. These tests defend the *contract* the
//! Mockcli frontend relies on at first render:
//!
//! 1. Auth is enforced on every route (401 without Bearer).
//! 2. Reads return 200 + valid JSON (content may be empty when sources are absent).
//! 3. Writes that still aren't wired to a backend return 501; writes that
//!    enqueue into the conductor return 202.
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

const TOKEN: &str = "phase-d-test-token";

fn make_state() -> AppState {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    AppState::for_test(cfg)
}

fn make_app() -> axum::Router {
    build_app(make_state())
}

/// Build an app that has a single registered `BuildSession`. Returns the
/// app + the session's UUID so callers can hit build-scoped routes that gate
/// on `state.builds.get(id)` (Phase 15+ — see `real_data::trigger_pillar`).
/// Used by `write_contracts_have_expected_semantics` to test the success
/// path of the pillar/dispatch enqueue routes.
fn make_app_with_registered_build() -> (axum::Router, Uuid) {
    let state = make_state();
    let session = Arc::new(BuildSession::new(
        PathBuf::from("/tmp"),
        AgentSession::Lightarchitects(ClaudeBackend::Anthropic),
    ));
    let build_id = session.build_id;
    state.builds.insert(session);
    (build_app(state), build_id)
}

async fn get(path: &str, with_auth: bool) -> (StatusCode, serde_json::Value) {
    let mut req = Request::get(path);
    if with_auth {
        req = req.header("authorization", format!("Bearer {TOKEN}"));
    }
    let resp = make_app()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let body = to_bytes(resp.into_body(), 1024 * 64).await.unwrap();
    let json =
        serde_json::from_slice::<serde_json::Value>(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// (post() helper removed when write_contracts_have_expected_semantics
// migrated to post_on() with pre-registered builds — #84.)

// ── Auth enforcement ─────────────────────────────────────────────────────────

#[tokio::test]
async fn all_stub_reads_require_bearer() {
    let paths = [
        "/api/workspaces",
        "/api/workspaces/w1",
        "/api/meta-skills",
        "/api/siblings",
        "/api/sitrep",
        "/api/conductor/status",
        "/api/arena/status",
    ];
    for p in paths {
        let (status, _) = get(p, false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "route {p} missing auth");
    }
}

// ── Read stubs ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn workspaces_returns_array() {
    let (status, body) = get("/api/workspaces", true).await;
    assert_eq!(status, StatusCode::OK);
    // Phase 9.8: real scan of ~/Projects/ — length is host-dependent.
    assert!(body.is_array(), "workspaces must be an array");
}

#[tokio::test]
async fn siblings_stub_returns_squad_entries() {
    let (status, body) = get("/api/siblings", true).await;
    assert_eq!(status, StatusCode::OK);
    // SquadRegistry defaults: corso, eva, soul, quantum, seraph, ayin (6 entries).
    // "claude" was removed — it has no LA binary and belongs to the agent layer, not the squad registry.
    assert_eq!(body.as_array().unwrap().len(), 6);
}

#[tokio::test]
async fn sitrep_reports_all_seven_pillars() {
    let (status, body) = get("/api/sitrep", true).await;
    assert_eq!(status, StatusCode::OK);
    // Phase 9.8: SITREP derives pillar colour from live sibling state — colour
    // is host-dependent but the pillar *keys* must always exist so the UI
    // renders a seven-dot grid without `undefined` gaps.
    for p in ["arch", "sec", "qual", "perf", "test", "doc", "ops"] {
        assert!(
            body["pillars"][p]["state"].is_string(),
            "pillar {p} must expose a string `state`"
        );
    }
    assert!(body["status"].is_string());
}

#[tokio::test]
async fn build_scoped_reads_stub_well_formed() {
    let b = Uuid::new_v4();
    let (s1, _) = get(&format!("/api/builds/{b}/findings"), true).await;
    let (s2, _) = get(&format!("/api/builds/{b}/notes"), true).await;
    let (s3, _) = get(&format!("/api/builds/{b}/artifacts"), true).await;
    let (s4, _) = get(&format!("/api/builds/{b}/gates/arch"), true).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(s3, StatusCode::OK);
    assert_eq!(s4, StatusCode::OK);
}

// ── Write stubs ──────────────────────────────────────────────────────────────

/// POST against an app that has the given build pre-registered. Necessary
/// because Phase 15+ `trigger_pillar` / dispatch handlers gate on
/// `state.builds.get(id)` and 404 on unknown builds.
async fn post_on(app: axum::Router, path: &str) -> StatusCode {
    let resp = app
        .oneshot(
            Request::post(path)
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    resp.status()
}

#[tokio::test]
async fn write_contracts_have_expected_semantics() {
    // Originally written for Phase 9.10 stubs where any UUID got 202 with an
    // empty body. Phase 15+ promoted these to real execution paths — pillar
    // and dispatch now require a registered build, and dispatch validates
    // its JSON body (sibling + prompt fields). The contract assertions below
    // reflect the *current* semantics; see #84 for the migration record.
    let (app1, b) = make_app_with_registered_build();
    assert_eq!(
        post_on(app1, &format!("/api/builds/{b}/pillars/arch")).await,
        StatusCode::ACCEPTED,
        "pillar enqueue should be 202 for a registered build"
    );

    // Dispatch with an empty body fails JSON validation → 422. Asserting 422
    // (not 202) keeps this a fast contract test — driving the success path
    // would spawn a copilot subprocess, which is slow + flaky in CI. The
    // 422 still proves the route is wired and body parsing is enforced.
    let (app2, b) = make_app_with_registered_build();
    assert_eq!(
        post_on(app2, &format!("/api/builds/{b}/dispatch")).await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "dispatch with empty body should 422 (route wired, body required)"
    );

    // Artifact upload is still Phase 10 work — 501 regardless of build state.
    let (app3, b) = make_app_with_registered_build();
    assert_eq!(
        post_on(app3, &format!("/api/builds/{b}/artifacts")).await,
        StatusCode::NOT_IMPLEMENTED,
        "artifact upload should be 501"
    );
}

#[tokio::test]
async fn copilot_returns_404_for_unknown_build() {
    let b = Uuid::new_v4();
    let resp = make_app()
        .oneshot(
            Request::post(format!("/api/builds/{b}/copilot"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"message":"hello"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
