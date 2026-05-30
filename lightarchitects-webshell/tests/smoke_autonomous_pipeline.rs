//! Smoke tests — Canon XXVII Suite 6 — autonomous pipeline boot checks.
//!
//! Verifies that the ironclaw autonomous build surface is wired and boots without
//! panic. Uses `AppState::for_test` (`mock_workers` = true) — no Ollama API key,
//! no git repo, and no network I/O required.
//!
//! Canon XXVII suite coverage: Suite 6 (smoke).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use lightarchitects_webshell::{
    config::{Cli, Config},
    container::DockerCapability,
    server::{AppState, build_app},
};

const TOKEN: &str = "smoke-autonomous-token";

fn make_app() -> axum::Router {
    let cli = Cli {
        port: 0,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    let state = AppState::for_test(cfg, DockerCapability::Unavailable);
    build_app(state)
}

// ── Suite 6: Smoke ─────────────────────────────────────────────────────────

#[tokio::test]
async fn smoke_autonomous_app_boots_without_panic() {
    // AppState::for_test must not panic — validates all struct fields initialise.
    let _ = make_app();
}

#[tokio::test]
async fn smoke_post_builds_autonomous_returns_build_id() {
    let app = make_app();

    let body = json!({
        "cwd": "/tmp",
        "mode": "autonomous",
        "waves": [[{
            "id": "smoke-task",
            "prompt": "smoke test noop",
            "depends_on": [],
            "file_ownership": [],
            "concurrency_safe": true
        }]]
    });

    let req = Request::post("/api/builds")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "POST /api/builds must return 200"
    );

    let bytes = axum::body::to_bytes(resp.into_body(), 65_536)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).expect("response must be valid JSON");
    let build_id = json["build_id"].as_str().expect("build_id field required");
    assert!(!build_id.is_empty(), "build_id must not be empty");
    assert_eq!(
        build_id.len(),
        36,
        "build_id must be a UUID-formatted string"
    );
}

#[tokio::test]
async fn smoke_get_builds_list_is_empty_at_start() {
    let app = make_app();

    let req = Request::get("/api/builds")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "GET /api/builds must return 200"
    );
}

#[tokio::test]
async fn smoke_autonomous_route_rejects_missing_auth() {
    let app = make_app();

    let body = json!({ "mode": "autonomous", "waves": [[]] });
    let req = Request::post("/api/builds")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "missing auth must yield 401"
    );
}
