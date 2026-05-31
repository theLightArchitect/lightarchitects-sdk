//! Integration tests for `GET /api/container/policy` and `PATCH /api/container/policy`.
//!
//! All tests use `AppState::for_test` + `tower::ServiceExt::oneshot` so they
//! exercise the real router stack without a running server.

#![allow(clippy::unwrap_used)]

use std::ffi::OsString;
use std::path::PathBuf;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use lightarchitects::container_spawn::ContainerPolicy;
use serde_json::Value;
use tower::ServiceExt;

use crate::{
    config::{AgentSession, Config, TokenSource},
    container::DockerCapability,
    server::{AppState, build_app},
};

fn bearer_header() -> &'static str {
    "Bearer test-token"
}

fn test_state() -> AppState {
    let config = Config {
        port: 0,
        host_cmd: OsString::from("bash"),
        cwd: PathBuf::from("/tmp"),
        token: "test-token".to_owned(),
        token_source: TokenSource::EnvVar,
        agent: AgentSession::default(),
        claude_agent_template: None,
        container_mode: crate::container::ContainerMode::Auto,
        dev_mode: false,
        max_context_prompts: 50,
        litellm: crate::config::LiteLLMConfig::default(),
        hermes_mcp: crate::config::HermesMcpConfig::default(),
    };
    AppState::for_test(config, DockerCapability::Unavailable)
}

async fn body_json(body: Body) -> Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// GET /api/container/policy returns the default policy.
#[tokio::test]
async fn get_policy_returns_default() {
    let state = test_state();
    let app = build_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["iso_mode"], "standard");
    assert_eq!(body["network"], "bridge");
}

/// GET /api/container/policy returns 401 without auth.
#[tokio::test]
async fn get_policy_requires_auth() {
    let state = test_state();
    let app = build_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/container/policy")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// PATCH with `memory_mb` tighter than default succeeds.
#[tokio::test]
async fn patch_tighter_memory_succeeds() {
    let state = test_state();
    let default_mem = ContainerPolicy::default().resources.memory_mb;
    let tighter_mem = default_mem / 2;

    let app = build_app(state);
    let body = serde_json::json!({ "memory_mb": tighter_mem });

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["memory_mb"], tighter_mem);
}

/// PATCH with `memory_mb` looser than default returns 422.
#[tokio::test]
async fn patch_looser_memory_rejected() {
    let state = test_state();
    let default_mem = ContainerPolicy::default().resources.memory_mb;
    let looser_mem = default_mem * 2;

    let app = build_app(state);
    let body = serde_json::json!({ "memory_mb": looser_mem });

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(resp.into_body()).await;
    assert!(body["error"].as_str().unwrap().contains("must not loosen"));
}

/// PATCH upgrading `iso_mode` to "hardened" succeeds.
#[tokio::test]
async fn patch_iso_mode_tighter_succeeds() {
    let state = test_state();
    let app = build_app(state);
    let body = serde_json::json!({ "iso_mode": "hardened" });

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["iso_mode"], "hardened");
}

/// PATCH with an unknown `iso_mode` string returns 422.
#[tokio::test]
async fn patch_unknown_iso_mode_rejected() {
    let state = test_state();
    let app = build_app(state);
    let body = serde_json::json!({ "iso_mode": "ultra-lockdown" });

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(resp.into_body()).await;
    assert!(body["error"].as_str().unwrap().contains("unknown iso_mode"));
}

/// PATCH with empty body (no fields) returns current policy unchanged.
#[tokio::test]
async fn patch_empty_body_no_change() {
    let state = test_state();
    let app = build_app(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["iso_mode"], "standard");
}

/// PATCH policy update is immediately visible on the next GET.
#[tokio::test]
async fn get_reflects_patch() {
    let state = test_state();
    let default_mem = ContainerPolicy::default().resources.memory_mb;
    let tighter_mem = default_mem / 2;

    let app = build_app(state.clone());

    // Apply a tightening patch.
    let patch_body = serde_json::json!({ "memory_mb": tighter_mem });
    let patch_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(patch_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), StatusCode::OK);

    // GET should now reflect the patched memory.
    let get_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let body = body_json(get_resp.into_body()).await;
    assert_eq!(body["memory_mb"], tighter_mem);
}
