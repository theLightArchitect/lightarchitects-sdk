//! `GET /api/roadmap` integration tests — webshell-roadmap-rendering Phase 3.
//!
//! Covers the three response branches of `roadmap_handler`:
//!
//! | Condition                    | Expected response                         |
//! |------------------------------|-------------------------------------------|
//! | File present, non-empty      | 200 `text/html; charset=utf-8` with body  |
//! | File absent / empty          | 200 `text/plain` with empty body          |
//! | Unauthenticated              | 401 (AuthGuard rejects)                   |

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf};

use axum::body::to_bytes;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use lightarchitects_webshell::{
    config::{Cli, Config},
    server::{AppState, build_app},
};
use tower::ServiceExt;

const TOKEN: &str = "test-token-roadmap-route";

fn make_app_with_path(html_path: PathBuf) -> axum::Router {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    let mut state = AppState::for_test(
        cfg,
        lightarchitects_webshell::container::DockerCapability::Unavailable,
    );
    state.roadmap_html_path = html_path;
    build_app(state)
}

fn authed_get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// File present and non-empty → 200 text/html with the artifact body.
#[tokio::test]
async fn roadmap_returns_html_when_file_present() {
    let dir = std::env::temp_dir().join("la-roadmap-test-present");
    let path = dir.join("roadmap.html");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&path, "<h1>Test Roadmap</h1>").unwrap();

    let app = make_app_with_path(path.clone());
    let res = app.oneshot(authed_get("/api/roadmap")).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let ct = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/html"), "expected text/html, got: {ct}");

    let body = to_bytes(res.into_body(), 1024 * 64).await.unwrap();
    assert!(std::str::from_utf8(&body).unwrap().contains("Test Roadmap"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// File absent → 200 with empty body (frontend shows empty state).
#[tokio::test]
async fn roadmap_returns_empty_when_file_absent() {
    let path = std::env::temp_dir().join("la-roadmap-test-absent-nonexistent.html");
    // Ensure it does not exist.
    let _ = std::fs::remove_file(&path);

    let app = make_app_with_path(path);
    let res = app.oneshot(authed_get("/api/roadmap")).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), 1024).await.unwrap();
    assert!(body.is_empty(), "expected empty body for absent file");
}

/// No auth header → 401.
#[tokio::test]
async fn roadmap_requires_authentication() {
    let path = std::env::temp_dir().join("la-roadmap-test-unauthed.html");
    let app = make_app_with_path(path);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/roadmap")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
