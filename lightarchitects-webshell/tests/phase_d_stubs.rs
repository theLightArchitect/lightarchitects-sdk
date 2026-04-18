//! Phase D stub-route smoke tests.
//!
//! These tests exist to defend the *contract* the Mockcli frontend expects
//! on first render: every screen makes a handful of `/api/*` fetches, and
//! all of them MUST respond with either well-formed JSON or a deliberate
//! error. Silent 404s would show up as console errors in the browser.
//!
//! We don't test the real shape of the data — that's what the stubs are
//! for. We test:
//!
//! 1. Auth is enforced on every route (401 without Bearer).
//! 2. Reads return 200 + valid JSON.
//! 3. Writes return 501 + a structured `{error, reason}` body.
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
async fn workspaces_stub_returns_empty_array() {
    let (status, body) = get("/api/workspaces", true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, serde_json::json!([]));
}

#[tokio::test]
async fn siblings_stub_returns_seven_entries() {
    let (status, body) = get("/api/siblings", true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 7);
}

#[tokio::test]
async fn sitrep_stub_reports_all_pillars_green() {
    let (status, body) = get("/api/sitrep", true).await;
    assert_eq!(status, StatusCode::OK);
    for p in ["arch", "sec", "qual", "perf", "test", "doc", "ops"] {
        assert_eq!(body["pillars"][p]["state"], "green");
    }
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
async fn write_stubs_return_501_not_implemented() {
    let b = Uuid::new_v4();
    let writes = [
        format!("/api/builds/{b}/pillars/arch"),
        format!("/api/builds/{b}/artifacts"),
        format!("/api/builds/{b}/copilot"),
        format!("/api/builds/{b}/dispatch"),
    ];
    for w in &writes {
        assert_eq!(
            post(w).await,
            StatusCode::NOT_IMPLEMENTED,
            "write {w} should be 501"
        );
    }
}
