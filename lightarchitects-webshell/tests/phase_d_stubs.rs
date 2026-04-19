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

use std::{ffi::OsString, path::PathBuf};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use lightarchitects_webshell::{
    config::{Cli, Config},
    server::{AppState, build_app},
};
use tower::ServiceExt;
use uuid::Uuid;

const TOKEN: &str = "phase-d-test-token";

fn make_app() -> axum::Router {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    build_app(AppState::for_test(cfg))
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

async fn post(path: &str) -> StatusCode {
    let resp = make_app()
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
async fn siblings_stub_returns_seven_entries() {
    let (status, body) = get("/api/siblings", true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 7);
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

#[tokio::test]
async fn write_contracts_have_expected_semantics() {
    let b = Uuid::new_v4();
    // Phase 9.10: pillars + dispatch enqueue to the conductor → 202 Accepted.
    assert_eq!(
        post(&format!("/api/builds/{b}/pillars/arch")).await,
        StatusCode::ACCEPTED,
        "pillar enqueue should be 202"
    );
    assert_eq!(
        post(&format!("/api/builds/{b}/dispatch")).await,
        StatusCode::ACCEPTED,
        "sibling dispatch enqueue should be 202"
    );
    // Artifact upload is still Phase 10 work.
    assert_eq!(
        post(&format!("/api/builds/{b}/artifacts")).await,
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
