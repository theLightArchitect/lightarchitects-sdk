//! Authorization test suite — Canon XXVII §50.
//!
//! Verifies that every protected route enforces the HMAC bearer token
//! correctly: 401 on missing/wrong credentials, 200 on valid credentials.
//! Uses `tower::ServiceExt::oneshot` — no TCP socket is bound.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use lightarchitects_webshell::{
    config::{Cli, Config},
    server::{AppState, build_app},
};
use tower::ServiceExt;

const TOKEN: &str = "test-token-auth-suite";

fn make_app() -> axum::Router {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    build_app(AppState::for_test(
        cfg,
        lightarchitects_webshell::container::DockerCapability::Unavailable,
    ))
}

// --- /api/auth-check ---------------------------------------------------------

#[tokio::test]
async fn auth_check_missing_header_is_401() {
    let resp = make_app()
        .oneshot(Request::get("/api/auth-check").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_check_wrong_token_is_401() {
    let resp = make_app()
        .oneshot(
            Request::get("/api/auth-check")
                .header("authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_check_correct_token_is_200() {
    let resp = make_app()
        .oneshot(
            Request::get("/api/auth-check")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn auth_check_lowercase_bearer_scheme_is_200() {
    // Bearer token validation must be case-insensitive on the scheme keyword.
    let resp = make_app()
        .oneshot(
            Request::get("/api/auth-check")
                .header("authorization", format!("bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// --- /api/events -------------------------------------------------------------

#[tokio::test]
async fn events_missing_auth_is_401() {
    let resp = make_app()
        .oneshot(Request::get("/api/events").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn events_wrong_token_is_401() {
    let resp = make_app()
        .oneshot(
            Request::get("/api/events")
                .header("authorization", "Bearer attacker-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn events_correct_token_opens_stream() {
    // We only assert 200 — reading the full SSE stream would block indefinitely.
    let resp = make_app()
        .oneshot(
            Request::get("/api/events")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
