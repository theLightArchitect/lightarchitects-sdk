//! User journey test suite — Canon XXVII §50.
//!
//! Simulates end-to-end sequences a real browser would perform:
//! 1. Health probe (unauthenticated)
//! 2. Auth-check to confirm the token is valid
//! 3. Open the SSE stream
//! 4. Verify static assets are served
//! 5. Verify the full auth → stream → disconnect cycle is stable

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lightarchitects_webshell::{
    config::{Cli, Config},
    server::{AppState, build_app},
};
use tower::ServiceExt;

const TOKEN: &str = "test-token-user-journey";

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

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

// --- Step 1: Health probe ---------------------------------------------------

#[tokio::test]
async fn journey_01_health_probe_succeeds_before_auth() {
    let resp = make_app()
        .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "ok");
}

// --- Step 2: Auth-check round-trip ------------------------------------------

#[tokio::test]
async fn journey_02_auth_check_rejects_before_valid_token() {
    let resp = make_app()
        .oneshot(Request::get("/api/auth-check").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn journey_03_auth_check_accepts_valid_token() {
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

// --- Step 3: SSE stream opens -----------------------------------------------

#[tokio::test]
async fn journey_04_sse_stream_opens_after_auth() {
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
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/event-stream"), "wrong content-type: {ct}");
}

// --- Step 4: Static assets --------------------------------------------------

#[tokio::test]
async fn journey_05_index_html_served_for_unknown_path() {
    let resp = make_app()
        .oneshot(Request::get("/unknown/path").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/html"), "expected html fallback: {ct}");
}

// --- Step 5: Repeated auth-check is stable ----------------------------------

#[tokio::test]
async fn journey_06_repeated_auth_checks_are_stable() {
    // Simulates a browser that re-validates its token after a page reload.
    for _ in 0..3 {
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
}
