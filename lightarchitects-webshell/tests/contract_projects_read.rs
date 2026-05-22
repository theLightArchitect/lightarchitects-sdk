//! Contract tests for GET /api/projects and GET /api/projects/:slug
//! Phase 2 — read-only endpoints (Part XXI §XXI.3)

#![allow(clippy::unwrap_used, clippy::expect_used, unsafe_code)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use lightarchitects_webshell::{
    config::{Cli, Config},
    server::{AppState, build_app},
};
use std::{ffi::OsString, sync::OnceLock};
use tower::ServiceExt;

// Serialize tests that mutate HOME — set_var is not thread-safe.
// tokio::sync::Mutex so the guard can be held across .await points.
static HOME_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
fn home_lock() -> &'static tokio::sync::Mutex<()> {
    HOME_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

const TOKEN: &str = "test-token-projects-read";

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

async fn body_bytes(resp: axum::response::Response) -> Vec<u8> {
    resp.into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

// ── list_projects_requires_auth ───────────────────────────────────────────────

#[tokio::test]
async fn list_projects_requires_auth() {
    let resp = make_app()
        .oneshot(Request::get("/api/projects").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── list_projects_returns_empty_when_no_projects ──────────────────────────────

#[tokio::test]
async fn list_projects_returns_empty_when_no_projects() {
    let _guard = home_lock().lock().await;
    // Override HOME so the handler scans a temp dir with no project manifests.
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: HOME_LOCK serializes all HOME mutations across parallel test threads.
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let resp = make_app()
        .oneshot(
            Request::get("/api/projects")
                .header("Authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // SAFETY: single-threaded test; no concurrent env reads.
    unsafe { std::env::remove_var("HOME") };

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(&body_bytes(resp).await).unwrap();
    assert!(body.as_array().unwrap().is_empty());
}

// ── get_project_404_when_no_toml ──────────────────────────────────────────────

#[tokio::test]
async fn get_project_404_when_no_toml() {
    let _guard = home_lock().lock().await;
    let tmp = tempfile::tempdir().unwrap();
    // Create the project directory but NOT .lightarchitects/project.toml
    std::fs::create_dir_all(tmp.path().join("Projects").join("test-proj")).unwrap();
    // SAFETY: HOME_LOCK serializes all HOME mutations across parallel test threads.
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let resp = make_app()
        .oneshot(
            Request::get("/api/projects/test-proj")
                .header("Authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // SAFETY: single-threaded test; no concurrent env reads.
    unsafe { std::env::remove_var("HOME") };

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body: serde_json::Value = serde_json::from_slice(&body_bytes(resp).await).unwrap();
    assert_eq!(body["code"], "MANIFEST_MISSING");
    assert_eq!(body["hint"], "POST /api/projects/init to create");
}

// ── get_project_200_with_valid_toml ───────────────────────────────────────────

#[tokio::test]
async fn get_project_200_with_valid_toml() {
    let _guard = home_lock().lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("Projects").join("my-proj");
    let dot_dir = project_dir.join(".lightarchitects");
    std::fs::create_dir_all(&dot_dir).unwrap();

    // Write a minimal valid project.toml
    let id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();
    let helix_link = tmp.path().join("helix").join("my-proj");
    let toml = format!(
        r#"[project]
id = "{id}"
slug = "my-proj"
name = "My Project"
kind = "folder"
created_at = "{now}"
helix_link = "{helix}"

[agents]
active = []
"#,
        id = id,
        now = now,
        helix = helix_link.display(),
    );
    std::fs::write(dot_dir.join("project.toml"), toml).unwrap();
    // SAFETY: HOME_LOCK serializes all HOME mutations across parallel test threads.
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let resp = make_app()
        .oneshot(
            Request::get("/api/projects/my-proj")
                .header("Authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // SAFETY: single-threaded test; no concurrent env reads.
    unsafe { std::env::remove_var("HOME") };

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(&body_bytes(resp).await).unwrap();
    assert_eq!(body["project"]["slug"], "my-proj");
    assert_eq!(body["project"]["name"], "My Project");
    assert_eq!(body["project"]["kind"], "folder");
    assert_eq!(body["project"]["id"], id.to_string());
}
