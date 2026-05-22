//! Phase 5 E2E — §2.38 Provider Credential Substrate route registration.
//!
//! OA guarantee: all 8 credential routes are reachable through the webshell
//! HTTP API without a terminal (P1 mechanical — `terminal_window_open_count === 0`).
//!
//! Strategy: unauthenticated requests must return 401 (route exists, `AuthGuard`
//! fires), NOT 404 (route missing). A 404 would mean the route was never
//! registered in `build_app()`.
//!
//! Routes covered (§2.38 table):
//!   GET  /api/auth/credential/{provider}/status
//!   POST /api/auth/credential/{provider}/init
//!   GET  /api/auth/credential/{provider}/callback  (Google OAuth)
//!   DELETE /api/auth/credential/{provider}
//!   POST /api/auth/credential/{provider}/key        (`ApiKey` providers)
//!   POST /api/auth/credential/github/device
//!   POST /api/auth/credential/github/poll
//!   POST /api/auth/credential/ollama/connect

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf};

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use lightarchitects_webshell::{
    config::{Cli, Config},
    server::{AppState, build_app},
};
use tower::ServiceExt;

const TOKEN: &str = "test-token-credential-e2e";

fn make_app() -> axum::Router {
    let cli = Cli {
        port: 8734,
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

/// Verify that `method + path` returns 401, not 404.
/// 401 = route registered + `AuthGuard` fired.
/// 404 = route not registered (test failure).
async fn assert_registered_not_404(method: Method, path: &str) {
    let req = Request::builder()
        .method(method.clone())
        .uri(path)
        .body(Body::empty())
        .unwrap();
    let resp = make_app().oneshot(req).await.unwrap();
    assert_ne!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "{method} {path} returned 404 — route not registered in build_app()"
    );
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "{method} {path} expected 401 (AuthGuard), got {}",
        resp.status()
    );
}

// ── §2.38 route registration checks ──────────────────────────────────────────

/// GET /api/auth/credential/{provider}/status — all 6 providers.
#[tokio::test]
async fn credential_status_google_registered() {
    assert_registered_not_404(Method::GET, "/api/auth/credential/google/status").await;
}

#[tokio::test]
async fn credential_status_github_registered() {
    assert_registered_not_404(Method::GET, "/api/auth/credential/github/status").await;
}

#[tokio::test]
async fn credential_status_anthropic_registered() {
    assert_registered_not_404(Method::GET, "/api/auth/credential/anthropic/status").await;
}

#[tokio::test]
async fn credential_status_openai_registered() {
    assert_registered_not_404(Method::GET, "/api/auth/credential/openai/status").await;
}

#[tokio::test]
async fn credential_status_mistral_registered() {
    assert_registered_not_404(Method::GET, "/api/auth/credential/mistral/status").await;
}

#[tokio::test]
async fn credential_status_ollama_registered() {
    assert_registered_not_404(Method::GET, "/api/auth/credential/ollama/status").await;
}

/// POST /api/auth/credential/{provider}/init — OAuth redirect initiation.
#[tokio::test]
async fn credential_init_google_registered() {
    assert_registered_not_404(Method::POST, "/api/auth/credential/google/init").await;
}

/// GET /api/auth/credential/{provider}/callback — OAuth callback (Google).
/// The callback is a public redirect target (no auth token from Google's redirect),
/// so it returns 200/400 on bad params rather than 401. Assert non-404 only.
#[tokio::test]
async fn credential_callback_google_registered() {
    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/auth/credential/google/callback")
        .body(Body::empty())
        .unwrap();
    let resp = make_app().oneshot(req).await.unwrap();
    assert_ne!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "GET /api/auth/credential/google/callback returned 404 — route not registered"
    );
}

/// DELETE /api/auth/credential/{provider} — revoke credential.
#[tokio::test]
async fn credential_revoke_google_registered() {
    assert_registered_not_404(Method::DELETE, "/api/auth/credential/google").await;
}

#[tokio::test]
async fn credential_revoke_anthropic_registered() {
    assert_registered_not_404(Method::DELETE, "/api/auth/credential/anthropic").await;
}

/// POST /api/auth/credential/{provider}/key — `ApiKey` store.
#[tokio::test]
async fn credential_key_anthropic_registered() {
    assert_registered_not_404(Method::POST, "/api/auth/credential/anthropic/key").await;
}

#[tokio::test]
async fn credential_key_openai_registered() {
    assert_registered_not_404(Method::POST, "/api/auth/credential/openai/key").await;
}

#[tokio::test]
async fn credential_key_mistral_registered() {
    assert_registered_not_404(Method::POST, "/api/auth/credential/mistral/key").await;
}

/// POST /api/auth/credential/github/device — RFC 8628 device code initiation.
#[tokio::test]
async fn credential_github_device_registered() {
    assert_registered_not_404(Method::POST, "/api/auth/credential/github/device").await;
}

/// POST /api/auth/credential/github/poll — RFC 8628 device token poll.
#[tokio::test]
async fn credential_github_poll_registered() {
    assert_registered_not_404(Method::POST, "/api/auth/credential/github/poll").await;
}

/// POST /api/auth/credential/ollama/connect — CLI subprocess connect.
#[tokio::test]
async fn credential_ollama_connect_registered() {
    assert_registered_not_404(Method::POST, "/api/auth/credential/ollama/connect").await;
}
