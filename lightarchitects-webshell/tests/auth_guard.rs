//! `AuthGuard` extractor — direct unit-style integration tests.
//!
//! Exercises the `FromRequestParts` impl in isolation by constructing
//! [`Parts`] + [`AppState`] manually and invoking
//! `AuthGuard::from_request_parts`. Validates the truth table for the
//! Bearer-OR-cookie OR semantics:
//!
//! | Bearer | Cookie | Result |
//! |--------|--------|--------|
//! | none   | none   | 401    |
//! | valid  | (any)  | OK     |
//! | wrong  | none   | 401    |
//! | wrong  | valid  | OK     |
//! | none   | valid  | OK     |
//! | wrong  | wrong  | 401    |
//! | valid  | wrong  | OK     |
//!
//! Wire-level tests for handler integration live in `tests/authorization.rs`
//! (added in the migration step).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{ffi::OsString, path::PathBuf};

use axum::body::Body;
use axum::extract::FromRequestParts;
use axum::http::{Request, StatusCode};
use lightarchitects_webshell::{
    auth::AuthGuard,
    config::{Cli, Config},
    server::AppState,
};

const TOKEN: &str = "test-token-auth-guard";

fn make_state() -> AppState {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: Some(PathBuf::from("/tmp")),
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    AppState::for_test(
        cfg,
        lightarchitects_webshell::container::DockerCapability::Unavailable,
    )
}

/// Build a Parts with the supplied (optional) headers.
fn parts_with(authorization: Option<&str>, cookie: Option<&str>) -> axum::http::request::Parts {
    let mut req = Request::builder().method("GET").uri("/api/anything");
    if let Some(a) = authorization {
        req = req.header("authorization", a);
    }
    if let Some(c) = cookie {
        req = req.header("cookie", c);
    }
    req.body(Body::empty()).unwrap().into_parts().0
}

async fn invoke(
    authorization: Option<&str>,
    cookie: Option<&str>,
) -> Result<AuthGuard, StatusCode> {
    let state = make_state();
    let mut parts = parts_with(authorization, cookie);
    AuthGuard::from_request_parts(&mut parts, &state)
        .await
        .map_err(|resp| resp.status())
}

// ── Reject paths ────────────────────────────────────────────────────────────

#[tokio::test]
async fn auth_guard_rejects_no_credentials() {
    let result = invoke(None, None).await;
    assert_eq!(result.err(), Some(StatusCode::UNAUTHORIZED));
}

#[tokio::test]
async fn auth_guard_rejects_wrong_bearer_only() {
    let result = invoke(Some("Bearer wrong-token"), None).await;
    assert_eq!(result.err(), Some(StatusCode::UNAUTHORIZED));
}

#[tokio::test]
async fn auth_guard_rejects_wrong_cookie_only() {
    let result = invoke(None, Some("la_session=wrong-token")).await;
    assert_eq!(result.err(), Some(StatusCode::UNAUTHORIZED));
}

#[tokio::test]
async fn auth_guard_rejects_wrong_bearer_and_wrong_cookie() {
    let result = invoke(Some("Bearer wrong"), Some("la_session=wrong")).await;
    assert_eq!(result.err(), Some(StatusCode::UNAUTHORIZED));
}

#[tokio::test]
async fn auth_guard_rejects_missing_scheme() {
    // No "Bearer " prefix → not a valid bearer header → reject.
    let result = invoke(Some(TOKEN), None).await;
    assert_eq!(result.err(), Some(StatusCode::UNAUTHORIZED));
}

#[tokio::test]
async fn auth_guard_rejects_unrelated_cookie() {
    // Cookie present but no `la_session` entry.
    let result = invoke(None, Some("other=value; xyz=abc")).await;
    assert_eq!(result.err(), Some(StatusCode::UNAUTHORIZED));
}

#[tokio::test]
async fn auth_guard_rejects_cookie_prefix_collision() {
    // `la_session_extra` must not match `la_session`.
    let result = invoke(None, Some(&format!("la_session_extra={TOKEN}"))).await;
    assert_eq!(result.err(), Some(StatusCode::UNAUTHORIZED));
}

// ── Accept paths ────────────────────────────────────────────────────────────

#[tokio::test]
async fn auth_guard_accepts_valid_bearer_only() {
    let result = invoke(Some(&format!("Bearer {TOKEN}")), None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn auth_guard_accepts_valid_cookie_only() {
    let result = invoke(None, Some(&format!("la_session={TOKEN}"))).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn auth_guard_accepts_lowercase_bearer_scheme() {
    let result = invoke(Some(&format!("bearer {TOKEN}")), None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn auth_guard_accepts_cookie_in_multi_cookie_header() {
    let result = invoke(None, Some(&format!("foo=bar; la_session={TOKEN}; baz=qux"))).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn auth_guard_falls_back_to_cookie_when_bearer_wrong() {
    let result = invoke(
        Some("Bearer wrong-token"),
        Some(&format!("la_session={TOKEN}")),
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn auth_guard_accepts_when_bearer_valid_and_cookie_wrong() {
    // Bearer-first eval: valid bearer accepts even if cookie is bogus.
    let result = invoke(Some(&format!("Bearer {TOKEN}")), Some("la_session=wrong")).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn auth_guard_accepts_both_valid() {
    let result = invoke(
        Some(&format!("Bearer {TOKEN}")),
        Some(&format!("la_session={TOKEN}")),
    )
    .await;
    assert!(result.is_ok());
}
