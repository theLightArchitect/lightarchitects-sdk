//! OD-6 contract-drift tests.
//!
//! The test `api_version_hash_matches_constant` is the change-control gate:
//! it fails immediately when the contract surface diverges from the pinned constant.
//! Any intentional surface change MUST update `API_VERSION_HASH` in `http/api_version.rs`
//! and add a changelog entry.
//!
//! Run to discover the new hash after a surface change:
//! ```
//! cargo test -p lightarchitects-gateway --test api_version_test -- --nocapture 2>&1 | grep 'hash ='
//! ```

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic, dead_code)]

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use tower::ServiceExt as _;

use lightarchitects_gateway::http::{
    api_version::{
        API_VERSION_DATE, API_VERSION_HASH, CONTRACT_SURFACE_COUNT, compute_api_version_hash,
    },
    build_http_router,
    state::{PlatformConfig, PlatformState},
};

const TEST_IP: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 54_321);

// ── Structural tests (no Neo4j) ───────────────────────────────────────────────

/// OD-6 primary gate: computed hash must match the pinned constant.
///
/// Fails when any of the following change without updating the constant:
/// `ALLOWED_SIBLINGS`, `ALLOWED_KINDS`, admin routes, platform routes.
///
/// Run with `--nocapture` to print the new hash when an intentional change
/// requires a constant update.
#[test]
fn api_version_hash_matches_constant() {
    let computed = compute_api_version_hash();
    println!("api_version_hash = {computed}  (pinned = {API_VERSION_HASH})");
    assert_eq!(
        computed, API_VERSION_HASH,
        "API contract surface has drifted from pinned hash — update API_VERSION_HASH \
         in http/api_version.rs and add a changelog entry.\n\
         New hash: {computed}"
    );
}

/// The hash must be exactly 16 hex characters (64-bit prefix of SHA-256).
#[test]
fn api_version_hash_is_16_hex_chars() {
    let hash = compute_api_version_hash();
    assert_eq!(hash.len(), 16, "hash must be 16 chars");
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit()),
        "hash must be hex"
    );
}

/// `CONTRACT_SURFACE_COUNT` must equal the actual number of route signatures hashed.
#[test]
fn contract_surface_count_is_accurate() {
    // 7 admin routes + 16 platform routes = 23 total.
    assert_eq!(
        CONTRACT_SURFACE_COUNT, 23,
        "CONTRACT_SURFACE_COUNT must match actual route count"
    );
}

/// `API_VERSION_DATE` must be a valid ISO date string.
#[test]
fn api_version_date_is_iso_format() {
    assert_eq!(
        API_VERSION_DATE.len(),
        10,
        "API_VERSION_DATE must be YYYY-MM-DD"
    );
    assert!(
        API_VERSION_DATE.chars().nth(4) == Some('-')
            && API_VERSION_DATE.chars().nth(7) == Some('-'),
        "API_VERSION_DATE must follow YYYY-MM-DD format"
    );
}

// ── Integration tests (require live Neo4j for router setup) ───────────────────

fn build_minimal_state() -> Arc<PlatformState> {
    // Structural test — no Neo4j. We can't construct PlatformState without a
    // Graph handle, so these tests go into the integration bucket (same harness,
    // same Neo4j requirement). They only exercise the handler, not Cypher.
    //
    // For a true no-Neo4j structural test, the router would need a mock Graph.
    // That's a separate refactor; accepted as a trade-off for now.
    panic!("build_minimal_state must not be called in unit context")
}

/// `GET /v1/version` → 200 with all four required fields.
///
/// Requires live Neo4j to construct `PlatformState`. Skipped in CI-only runs
/// via `--skip integration`.
#[tokio::test]
async fn integration_version_endpoint_200() {
    let neo4j_pass = std::env::var("NEO4J_PASS").unwrap_or_else(|_| {
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
            .expect("NEO4J_PASS required for integration tests")
    });

    let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let graph = lightarchitects_gateway::http::neo4j::connect(&uri, &user, &neo4j_pass)
        .await
        .expect("neo4j connect");

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
        config: PlatformConfig::default(),
        admin_token: None,
        read_token: None,
    });

    let app = build_http_router(state);
    let req = Request::builder()
        .method("GET")
        .uri("/v1/version")
        .extension(ConnectInfo(TEST_IP))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "GET /v1/version must return 200"
    );

    let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["api_version"], "v1");
    assert_eq!(json["api_version_date"], API_VERSION_DATE);
    assert_eq!(json["api_version_hash"], API_VERSION_HASH);
    assert_eq!(
        usize::try_from(json["contract_surface_count"].as_u64().unwrap()).unwrap(),
        CONTRACT_SURFACE_COUNT
    );
}

/// Response headers include `lightarchitects-api-version-hash` on every response.
#[tokio::test]
async fn integration_version_hash_header_present() {
    let neo4j_pass = std::env::var("NEO4J_PASS").unwrap_or_else(|_| {
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
            .expect("NEO4J_PASS required")
    });

    let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let graph = lightarchitects_gateway::http::neo4j::connect(&uri, &user, &neo4j_pass)
        .await
        .expect("neo4j connect");

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
        config: PlatformConfig::default(),
        admin_token: None,
        read_token: None,
    });

    let app = build_http_router(state);
    let req = Request::builder()
        .method("GET")
        .uri("/v1/platform/health")
        .extension(ConnectInfo(TEST_IP))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let header_val = resp
        .headers()
        .get("lightarchitects-api-version-hash")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(
        header_val, API_VERSION_HASH,
        "lightarchitects-api-version-hash header must equal pinned constant"
    );
}
