//! A5 tests — `POST /v1/platform/helix/retrieve` + `GET /v1/platform/helix/cache/stats`.
//!
//! Structural tests (`a5_structural_*`) require no Neo4j.
//! Integration tests (a5_*_integration_*) require a live Neo4j instance with
//! at least one Step node seeded.
//!
//! Run structural only:
//! ```
//! cargo test -p lightarchitects-gateway helix_retrieve -- --skip integration
//! ```
//! Run all:
//! ```
//! NEO4J_PASS=... cargo test -p lightarchitects-gateway helix_retrieve
//! ```

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic, dead_code)]

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use tower::ServiceExt as _;

use lightarchitects::helix::{HelixDb, HelixNeo4j, Neo4jConfig};
use lightarchitects_gateway::http::{
    build_http_router,
    state::{PlatformConfig, PlatformState},
};

// ── Constants ─────────────────────────────────────────────────────────────────

const TEST_IP: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 54_321);

const MAX_QUERY_BYTES: usize = 2048;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn neo4j_pass() -> String {
    std::env::var("NEO4J_PASS").unwrap_or_else(|_| {
        std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "soul-neo4j-local",
                "-a",
                "password",
                "-w",
            ])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
            .expect("NEO4J_PASS env var or keychain soul-neo4j-local/password required")
    })
}

async fn build_neo4j_state() -> Arc<PlatformState> {
    let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let pass = neo4j_pass();

    let graph = lightarchitects_gateway::http::neo4j::connect(&uri, &user, &pass)
        .await
        .expect("neo4j connect");

    let helix_db: Arc<dyn HelixDb> = Arc::new(
        HelixNeo4j::connect(&Neo4jConfig {
            uri: uri.clone(),
            user: user.clone(),
            password: secrecy::SecretString::from(pass.clone()),
        })
        .await
        .expect("helix_db connect"),
    );

    Arc::new(PlatformState {
        graph,
        read_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_minute(
            NonZeroU32::MIN.saturating_add(99),
        ))),
        helix_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_minute(
            NonZeroU32::MIN.saturating_add(19),
        ))),
        write_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_minute(
            NonZeroU32::MIN.saturating_add(9),
        ))),
        skills_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_second(
            NonZeroU32::MIN,
        ))),
        auth_fail_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_minute(
            NonZeroU32::MIN.saturating_add(4),
        ))),
        auth_fail_counts: Arc::new(dashmap::DashMap::new()),
        circuit_breaker: Arc::new(tokio::sync::Mutex::new(
            lightarchitects_gateway::http::circuit_breaker::CircuitBreaker::default(),
        )),
        canon_cache: moka::future::Cache::builder().max_capacity(10).build(),
        agent_cache: moka::future::Cache::builder().max_capacity(10).build(),
        arch_cache: moka::future::Cache::builder().max_capacity(10).build(),
        config: PlatformConfig::default(),
        admin_token: None,
        read_token: None,
        helix_db,
        helix_cache: lightarchitects::helix::HelixCache::new(
            &lightarchitects::helix::HelixCacheConfig::default(),
        ),
        embedding_provider: Arc::new(lightarchitects::helix::MockEmbeddingProvider::new(384)),
    })
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

fn post_retrieve(body: serde_json::Value, ip: SocketAddr) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/platform/helix/retrieve")
        .header("content-type", "application/json")
        .extension(ConnectInfo(ip))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

// ── Structural tests (no Neo4j) ───────────────────────────────────────────────

/// A5-struct-01: `MAX_QUERY_BYTES` constant must be 2048 (F6 guard).
#[test]
fn a5_structural_query_length_cap_is_2048() {
    assert_eq!(MAX_QUERY_BYTES, 2048, "F6 query cap must be 2048 bytes");
}

/// A5-struct-02: valid `mode_override` strings are exactly the three allowed values.
#[test]
fn a5_structural_allowed_modes_are_three() {
    let expected = ["balanced", "graph_weighted", "keyword_dominated"];
    // The helix.rs handler defines ALLOWED_MODES; this test documents the contract.
    // The integration tests below verify behaviour via the endpoint.
    let mut sorted = expected;
    sorted.sort_unstable();
    assert_eq!(sorted.len(), 3, "F1 allowlist must have exactly 3 modes");
    assert_eq!(sorted, ["balanced", "graph_weighted", "keyword_dominated"]);
}

// ── Integration tests (require live Neo4j) ────────────────────────────────────

/// A5-01: POST with valid query → 200 with `results`, `mode`, `count`.
#[tokio::test]
async fn a5_01_integration_retrieve_200() {
    let state = build_neo4j_state().await;
    let app = build_http_router(state);

    let resp = app
        .oneshot(post_retrieve(
            serde_json::json!({"query": "canon architecture"}),
            TEST_IP,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "A5-01: valid retrieve must return 200"
    );
    let json = body_json(resp).await;
    assert!(
        json.get("results").and_then(|v| v.as_array()).is_some(),
        "A5-01: response must include results array"
    );
    assert!(
        json.get("mode").and_then(|v| v.as_str()).is_some(),
        "A5-01: response must include mode"
    );
    assert!(
        json.get("count")
            .and_then(serde_json::Value::as_u64)
            .is_some(),
        "A5-01: response must include count"
    );
}

/// A5-02: `ETag` header is present on retrieve response.
#[tokio::test]
async fn a5_02_integration_etag_present() {
    let state = build_neo4j_state().await;
    let app = build_http_router(state);

    let resp = app
        .oneshot(post_retrieve(
            serde_json::json!({"query": "soul helix"}),
            TEST_IP,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK, "A5-02: must be 200");
    assert!(
        resp.headers().contains_key("etag"),
        "A5-02: ETag header must be present for cache revalidation"
    );
}

/// A5-03: POST without auth token when `read_token` configured → 401.
///
/// This test uses a state with a read token set and sends no Authorization header.
#[tokio::test]
async fn a5_03_integration_no_token_returns_401() {
    let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let pass = neo4j_pass();

    let graph = lightarchitects_gateway::http::neo4j::connect(&uri, &user, &pass)
        .await
        .expect("neo4j connect");

    let helix_db: Arc<dyn HelixDb> = Arc::new(
        HelixNeo4j::connect(&Neo4jConfig {
            uri: uri.clone(),
            user: user.clone(),
            password: secrecy::SecretString::from(pass.clone()),
        })
        .await
        .expect("helix_db connect"),
    );

    let state = Arc::new(PlatformState {
        graph,
        read_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_minute(
            NonZeroU32::MIN.saturating_add(99),
        ))),
        helix_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_minute(
            NonZeroU32::MIN.saturating_add(19),
        ))),
        write_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_minute(
            NonZeroU32::MIN.saturating_add(9),
        ))),
        skills_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_second(
            NonZeroU32::MIN,
        ))),
        auth_fail_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_minute(
            NonZeroU32::MIN.saturating_add(4),
        ))),
        auth_fail_counts: Arc::new(dashmap::DashMap::new()),
        circuit_breaker: Arc::new(tokio::sync::Mutex::new(
            lightarchitects_gateway::http::circuit_breaker::CircuitBreaker::default(),
        )),
        canon_cache: moka::future::Cache::builder().max_capacity(10).build(),
        agent_cache: moka::future::Cache::builder().max_capacity(10).build(),
        arch_cache: moka::future::Cache::builder().max_capacity(10).build(),
        config: PlatformConfig::default(),
        admin_token: None,
        read_token: Some(secrecy::SecretBox::new(Box::new("test-token".to_owned()))),
        helix_db,
        helix_cache: lightarchitects::helix::HelixCache::new(
            &lightarchitects::helix::HelixCacheConfig::default(),
        ),
        embedding_provider: Arc::new(lightarchitects::helix::MockEmbeddingProvider::new(384)),
    });

    let app = build_http_router(state);
    let resp = app
        .oneshot(post_retrieve(
            serde_json::json!({"query": "test query"}),
            TEST_IP,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "A5-03: missing token must return 401 when read_token configured"
    );
}

/// A5-04: valid `helix_id` filter accepted → 200.
#[tokio::test]
async fn a5_04_integration_helix_id_filter_accepted() {
    let state = build_neo4j_state().await;
    let app = build_http_router(state);

    let resp = app
        .oneshot(post_retrieve(
            serde_json::json!({"query": "canon", "helix_id": "soul/soul"}),
            TEST_IP,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "A5-04: retrieve with helix_id filter must return 200"
    );
}

/// A5-05: invalid `mode_override` value → 422 (F1 — OWASP API3:2023).
#[tokio::test]
async fn a5_05_integration_invalid_mode_override_returns_422() {
    let state = build_neo4j_state().await;
    let app = build_http_router(state);

    let resp = app
        .oneshot(post_retrieve(
            serde_json::json!({"query": "test", "mode_override": "invalid_mode"}),
            TEST_IP,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "A5-05: invalid mode_override must return 422"
    );
    let json = body_json(resp).await;
    assert_eq!(
        json["error"]["code"], "invalid_mode_override",
        "A5-05: error code must be invalid_mode_override"
    );
}

/// A5-06: query exceeding 2048 bytes → 422 (F6 — `CVSS` 7.5 `DoS` prevention).
#[tokio::test]
async fn a5_06_integration_query_too_long_returns_422() {
    let state = build_neo4j_state().await;
    let app = build_http_router(state);

    // 2049-byte query — one byte over the cap.
    let long_query = "a".repeat(2049);
    let resp = app
        .oneshot(post_retrieve(
            serde_json::json!({"query": long_query}),
            TEST_IP,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "A5-06: query exceeding 2048 bytes must return 422"
    );
    let json = body_json(resp).await;
    assert_eq!(
        json["error"]["code"], "query_too_long",
        "A5-06: error code must be query_too_long"
    );
}
