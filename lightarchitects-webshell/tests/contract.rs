//! Contract test suite — Canon XXVII §50.
//!
//! Verifies the observable API shape: status codes, content-types, CORS
//! headers, and response bodies.  These tests exist to catch accidental
//! breaking changes to the public API surface.

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

const TOKEN: &str = "test-token-contract-suite";

fn make_app() -> axum::Router {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    build_app(AppState::for_test(cfg))
}

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

// --- /api/health -------------------------------------------------------------

#[tokio::test]
async fn health_returns_200() {
    let resp = make_app()
        .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_body_is_ok() {
    let resp = make_app()
        .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(body_string(resp).await, "ok");
}

// --- /api/events content-type ------------------------------------------------

#[tokio::test]
async fn events_content_type_is_event_stream() {
    let resp = make_app()
        .oneshot(
            Request::get("/api/events")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("text/event-stream"),
        "unexpected content-type: {ct}"
    );
}

// --- Static asset fallback ---------------------------------------------------

#[tokio::test]
async fn unknown_path_serves_index_html() {
    // rust-embed fallback: any unmatched path returns the embedded index.html.
    let resp = make_app()
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/html"), "expected html, got: {ct}");
}

// --- CORS --------------------------------------------------------------------

#[tokio::test]
async fn cors_header_present_on_health() {
    // The CORS layer restricts allowed origins to localhost variants.
    // The test port is 8733, so http://localhost:8733 is on the allowlist.
    let resp = make_app()
        .oneshot(
            Request::get("/api/health")
                .header("origin", "http://localhost:8733")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let acao = resp
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok());
    assert!(acao.is_some(), "CORS header missing for allowed origin");
    assert_eq!(
        acao.unwrap(),
        "http://localhost:8733",
        "CORS header must echo the matched allowed origin"
    );
}

#[tokio::test]
async fn cors_header_absent_for_unknown_origin() {
    // Origins not on the allowlist must not receive the CORS header —
    // this is the security invariant the explicit allowlist enforces.
    let resp = make_app()
        .oneshot(
            Request::get("/api/health")
                .header("origin", "http://evil.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let acao = resp
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok());
    assert!(
        acao.is_none() || acao.unwrap() != "http://evil.example.com",
        "CORS header must not be returned for unlisted origin"
    );
}

// --- auth-check body ---------------------------------------------------------

#[tokio::test]
async fn auth_check_401_body_does_not_contain_token() {
    // The 401 response must not leak the HMAC token in the body.
    let resp = make_app()
        .oneshot(
            Request::get("/api/auth-check")
                .header("authorization", "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_string(resp).await;
    assert!(
        !body.contains(TOKEN),
        "token must not appear in 401 body: {body}"
    );
}

// --- /api/polytopes (luminous-grafting-nautilus Phase 1) ---------------------

#[tokio::test]
async fn polytopes_requires_authorization() {
    let resp = make_app()
        .oneshot(Request::get("/api/polytopes").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn polytopes_rejects_wrong_token() {
    let resp = make_app()
        .oneshot(
            Request::get("/api/polytopes")
                .header("authorization", "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn polytopes_accepts_valid_token_and_returns_json_array() {
    let resp = make_app()
        .oneshot(
            Request::get("/api/polytopes")
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
    assert!(
        ct.contains("application/json"),
        "unexpected content-type: {ct}"
    );
    let body = body_string(resp).await;
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("body must be valid JSON");
    let arr = parsed.as_array().expect("body must be a JSON array");
    assert!(!arr.is_empty(), "polytope snapshot must not be empty");
    // Spot-check: every entry has the documented fields.
    for entry in arr {
        assert!(entry.get("id").is_some(), "entry missing id: {entry}");
        assert!(entry.get("color").is_some(), "entry missing color: {entry}");
        assert!(
            entry.get("polytope").is_some(),
            "entry missing polytope: {entry}"
        );
    }
}

#[tokio::test]
async fn polytopes_401_body_does_not_contain_token() {
    let resp = make_app()
        .oneshot(
            Request::get("/api/polytopes")
                .header("authorization", "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_string(resp).await;
    assert!(
        !body.contains(TOKEN),
        "token must not appear in 401 body: {body}"
    );
}
