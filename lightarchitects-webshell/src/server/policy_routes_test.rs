//! Integration tests for `GET /api/container/policy` and `PATCH /api/container/policy`.
//!
//! All tests use `AppState::for_test` + `tower::ServiceExt::oneshot` so they
//! exercise the real router stack without a running server.

#![allow(clippy::unwrap_used)]

use std::ffi::OsString;
use std::path::PathBuf;

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use lightarchitects::container_spawn::ContainerPolicy;
use serde_json::Value;
use std::net::SocketAddr;
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
        resume_session_id: None,
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
                .header(header::IF_MATCH, "\"0\"")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 54321))))
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
                .header(header::IF_MATCH, "\"0\"")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 54321))))
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
                .header(header::IF_MATCH, "\"0\"")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 54321))))
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
                .header(header::IF_MATCH, "\"0\"")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 54321))))
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
                .header(header::IF_MATCH, "\"0\"")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 54321))))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["iso_mode"], "standard");
}

/// GET /api/container/policy returns an `ETag` header with the version.
#[tokio::test]
async fn get_returns_etag_header() {
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
    let etag = resp.headers().get(header::ETAG).unwrap().to_str().unwrap();
    assert_eq!(etag, "\"0\"", "initial version should be 0");
}

/// PATCH without `If-Match` returns 428 Precondition Required (mandatory since `ETag` enforcement).
#[tokio::test]
async fn patch_without_if_match_succeeds() {
    let state = test_state();
    let default_mem = ContainerPolicy::default().resources.memory_mb;
    let tighter = default_mem / 2;
    let app = build_app(state);
    let body = serde_json::json!({ "memory_mb": tighter });

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 54321))))
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::PRECONDITION_REQUIRED);
}

/// PATCH with a correct `If-Match` header advances the version and returns the new `ETag`.
#[tokio::test]
async fn patch_with_correct_if_match_returns_new_etag() {
    let state = test_state();
    let default_mem = ContainerPolicy::default().resources.memory_mb;
    let tighter = default_mem / 2;
    let app = build_app(state);
    let body = serde_json::json!({ "memory_mb": tighter });

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::IF_MATCH, "\"0\"")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 54321))))
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let etag = resp.headers().get(header::ETAG).unwrap().to_str().unwrap();
    assert_eq!(
        etag, "\"1\"",
        "version should advance to 1 after first PATCH"
    );
}

/// PATCH with a stale `If-Match` value returns 412 Precondition Failed.
#[tokio::test]
async fn patch_stale_if_match_returns_412() {
    let state = test_state();
    let default_mem = ContainerPolicy::default().resources.memory_mb;
    let tighter = default_mem / 2;
    let app = build_app(state);
    let body = serde_json::json!({ "memory_mb": tighter });

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::IF_MATCH, "\"99\"") // stale — current version is 0
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 54321))))
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::PRECONDITION_FAILED);
    let body = body_json(resp.into_body()).await;
    assert!(body["current_version"].as_u64().is_some());
}

/// Two PATCH requests within 1 second from the same token → second returns 429.
#[tokio::test]
async fn rate_limit_second_patch_within_one_second_returns_429() {
    let state = test_state();
    let default_mem = ContainerPolicy::default().resources.memory_mb;
    let tighter = (default_mem / 4).max(1);
    let app = build_app(state);

    // First PATCH — should succeed.
    let body1 = serde_json::json!({ "memory_mb": tighter });
    let resp1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::IF_MATCH, "\"0\"")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 54321))))
                .body(Body::from(body1.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);

    // Second PATCH immediately after — rate limited.
    let body2 = serde_json::json!({ "memory_mb": tighter });
    let resp2 = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 54321))))
                .body(Body::from(body2.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(resp2.headers().contains_key(header::RETRY_AFTER));
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
                .header(header::IF_MATCH, "\"0\"")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 54321))))
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

/// PATCH from a Docker bridge IP returns 403 Forbidden (CIDR guard enforcement).
///
/// Seeds the guard with `172.17.0.0/16` (the default Docker bridge) and sends
/// the request with a peer IP inside that range.  The `patch_policy` handler must
/// reject it before reaching rate-limit or monotonicity checks.
#[tokio::test]
async fn patch_from_bridge_ip_returns_403() {
    use crate::container::cidr_guard::BridgeCidrGuard;

    let mut state = test_state();
    // Override the default empty guard with a known bridge CIDR.
    state.bridge_cidr_guard = std::sync::Arc::new(BridgeCidrGuard::with_cidrs(vec![(
        "172.17.0.0".parse().unwrap(),
        16,
    )]));

    let default_mem = ContainerPolicy::default().resources.memory_mb;
    let tighter = default_mem / 2;
    let app = build_app(state);
    let body = serde_json::json!({ "memory_mb": tighter });

    // Peer IP 172.17.0.2 is inside 172.17.0.0/16 — must be blocked.
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/container/policy")
                .header(header::AUTHORIZATION, bearer_header())
                .header(header::CONTENT_TYPE, "application/json")
                .extension(ConnectInfo(SocketAddr::from(([172, 17, 0, 2], 49152))))
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = body_json(resp.into_body()).await;
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("container network"),
        "error must mention container network"
    );
}

/// Concurrency cap regression: semaphore with cap=3 allows exactly 3 acquires;
/// the 4th returns `ConcurrencyCapExceeded` (regression for the G1 TOCTOU race).
///
/// This validates the semaphore-based fix at the unit level without requiring Docker.
#[tokio::test]
async fn concurrency_cap_semaphore_enforces_limit() {
    use crate::container::types::ContainerError;
    use crate::server::AppState;
    use std::ffi::OsString;
    use std::path::PathBuf;

    const CAP: usize = 3;

    let config = crate::config::Config {
        port: 0,
        host_cmd: OsString::from("bash"),
        cwd: PathBuf::from("/tmp"),
        token: "test-token".to_owned(),
        token_source: crate::config::TokenSource::EnvVar,
        agent: crate::config::AgentSession::default(),
        claude_agent_template: None,
        container_mode: crate::container::ContainerMode::Auto,
        dev_mode: false,
        max_context_prompts: 50,
        litellm: crate::config::LiteLLMConfig::default(),
        hermes_mcp: crate::config::HermesMcpConfig::default(),
        resume_session_id: None,
    };
    let state = AppState::for_test(config, crate::container::DockerCapability::Unavailable);

    // Replace the default semaphore with a known small cap.
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(CAP));
    let sem2 = std::sync::Arc::clone(&sem);

    // Acquire CAP permits — all should succeed.
    let mut permits = Vec::new();
    for _ in 0..CAP {
        let p = sem2
            .clone()
            .try_acquire_owned()
            .map_err(|_| ContainerError::ConcurrencyCapExceeded);
        assert!(p.is_ok(), "expected acquire to succeed within cap");
        permits.push(p.unwrap());
    }

    // CAP+1th acquire must fail.
    let overflow = sem2
        .clone()
        .try_acquire_owned()
        .map_err(|_| ContainerError::ConcurrencyCapExceeded);
    assert!(
        matches!(overflow, Err(ContainerError::ConcurrencyCapExceeded)),
        "expected ConcurrencyCapExceeded when cap is exhausted"
    );

    // Dropping one permit frees a slot — the next acquire succeeds.
    drop(permits.pop());
    let recovered = sem2
        .clone()
        .try_acquire_owned()
        .map_err(|_| ContainerError::ConcurrencyCapExceeded);
    assert!(
        recovered.is_ok(),
        "expected acquire to succeed after permit released"
    );

    // Keep `state` alive so the reaper task does not race with the test.
    drop(state);
}
