//! Security tests for `POST /api/projects/init` (Part XXI §XXI.3).
//!
//! 5 cases per the Phase 3 security test matrix:
//! - S1: slug with `..` rejected before FS access (`SLUG_INVALID` 400)
//! - S2: slug with `/` rejected before FS access (`SLUG_INVALID` 400)
//! - S3: slug with uppercase rejected before FS access (`SLUG_INVALID` 400)
//! - S4: unauthenticated request rejected 401
//! - S5: path traversal via symlink caught by `canonicalize_and_check`

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::ffi::OsString;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt as _;
use lightarchitects_webshell::{
    config::{Cli, Config},
    server::{AppState, build_app},
};
use tower::ServiceExt as _;

const TOKEN: &str = "test-token-init-security";

fn make_app() -> axum::Router {
    let cli = Cli {
        port: 8733,
        host_cmd: OsString::from("echo"),
        cwd: None,
        ..Default::default()
    };
    let cfg = Config::resolve_with_token(cli, Some(TOKEN.to_owned())).unwrap();
    build_app(AppState::for_test(
        cfg,
        lightarchitects_webshell::container::DockerCapability::Unavailable,
    ))
}

async fn post_init(slug: &str) -> (StatusCode, serde_json::Value) {
    let body = serde_json::json!({"slug": slug}).to_string();
    let resp = make_app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/projects/init")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// S1 — slug `..` must never reach the filesystem.
#[tokio::test]
async fn rejects_invalid_slug_dotdot() {
    let (status, body) = post_init("..").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");
    assert_eq!(body["code"], "SLUG_INVALID", "body: {body}");
}

// S2 — slug `foo/bar` must never reach the filesystem.
#[tokio::test]
async fn rejects_invalid_slug_slash() {
    let (status, body) = post_init("foo/bar").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");
    assert_eq!(body["code"], "SLUG_INVALID", "body: {body}");
}

// S3 — uppercase slug must be rejected before FS access.
#[tokio::test]
async fn rejects_invalid_slug_uppercase() {
    let (status, body) = post_init("MyProject").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");
    assert_eq!(body["code"], "SLUG_INVALID", "body: {body}");
}

// S4 — unauthenticated request must be rejected 401 (AuthGuard fires first).
#[tokio::test]
async fn rejects_unauthenticated() {
    let resp = make_app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/projects/init")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"slug":"my-project"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// S5 — symlink escape is caught by canonicalize_and_check (unit-level).
//
// Tests the security guard that `init_project` delegates to.  We bypass the
// HTTP layer to avoid HOME manipulation; the HTTP path through init_project
// exercises the same code-path on every valid slug.
#[test]
fn rejects_path_traversal_via_symlink() {
    use lightarchitects_webshell::projects::canonicalize_and_check;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("Projects");
    let real_dir = projects.join("legit-project");
    std::fs::create_dir_all(&real_dir).unwrap();

    // Symlink that points outside Projects/ — simulates a crafted symlink in
    // the project dir that could redirect `.lightarchitects/` writes.
    let escape_link = real_dir.join("escape");
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc", &escape_link).unwrap();

    let allowed = vec![projects.clone()];

    // The symlink target `/etc` escapes `Projects/` — must be rejected.
    let result = canonicalize_and_check(&escape_link, &allowed);
    assert!(
        result.is_err(),
        "expected PathError for symlink escape, got {result:?}"
    );

    // The project directory itself (no symlink) must be accepted.
    let ok = canonicalize_and_check(&real_dir, &allowed);
    assert!(
        ok.is_ok(),
        "legitimate project dir should be allowed: {ok:?}"
    );
}
