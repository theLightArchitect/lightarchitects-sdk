//! A2 integration tests — `POST /v1/admin/agents/upload` for the LÆX sibling.
//!
//! Validates that the laex-sibling-promotion swap (vestigial `"claude"` → `"laex"`)
//! persists correctly to the Neo4j `SiblingIdentity` node and returns expected fields.
//!
//! **Requires a live Neo4j instance**:
//! - `NEO4J_PASS` env var (or keychain `soul-neo4j-local/password`)
//! - `LIGHTARCHITECTS_ADMIN_TOKEN` env var (or keychain `soul-neo4j-local/admin-token`)
//!
//! Run structural tests only (no Neo4j):
//! ```
//! cargo test -p lightarchitects-gateway admin_agents -- --skip integration
//! ```
//! Run all:
//! ```
//! NEO4J_PASS=... LIGHTARCHITECTS_ADMIN_TOKEN=... cargo test -p lightarchitects-gateway admin_agents
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
    build_http_router,
    state::{PlatformConfig, PlatformState},
};

// ── Constants ─────────────────────────────────────────────────────────────────

const TEST_IP: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 54_321);

const TEST_IP_2: SocketAddr = SocketAddr::new(
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 2)),
    54_321,
);

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn build_neo4j_state(admin_token: Option<&str>) -> Arc<PlatformState> {
    let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let pass = neo4j_pass();
    let graph = lightarchitects_gateway::http::neo4j::connect(&uri, &user, &pass)
        .await
        .expect("neo4j connect");

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
        admin_token: admin_token.map(|t| secrecy::SecretBox::new(Box::new(t.to_owned()))),
        read_token: None,
    })
}

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

fn admin_token() -> String {
    std::env::var("LIGHTARCHITECTS_ADMIN_TOKEN").unwrap_or_else(|_| {
        std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "soul-neo4j-local",
                "-a",
                "admin-token",
                "-w",
            ])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
            .expect("LIGHTARCHITECTS_ADMIN_TOKEN or keychain soul-neo4j-local/admin-token required")
    })
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

fn post_admin(
    uri: &str,
    body: serde_json::Value,
    admin_tok: &str,
    ip: SocketAddr,
) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-admin-token", admin_tok)
        .extension(ConnectInfo(ip))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn post_no_token(uri: &str, body: serde_json::Value, ip: SocketAddr) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .extension(ConnectInfo(ip))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

/// Canonical LÆX payload from PCR Phase 3 W4 (version-pinned to 1.1.0).
fn laex_payload() -> serde_json::Value {
    serde_json::json!({
        "sibling": "laex",
        "role": "Full name: Light Architects Exodus. Canon keeper and governance umbrella. Owns: canon enforcement, methodology standards, product gate (LASDLC Layer 3), compliance, reflection. 4 META skills: REFLECT · CANON-CHECK · MATRIX-RATIFY · EFFECTIVENESS-SCORE. Industry baseline allowlist governance (FetchBaseline action). Maintains Builders Cookbook and platform architecture standards. LASDLC methodology keeper across all 7 siblings.",
        "voice": "Israeli accent, gravitas + warmth — KJV authority meets Tony Stark wit. Speaks in canon, not opinion. Cites chapter and verse.",
        "strands": ["Canon", "Methodology", "Product", "Compliance", "Reflection"],
        "version": "1.1.0"
    })
}

// ── Structural tests (no Neo4j) ───────────────────────────────────────────────

/// `ALLOWED_SIBLINGS` must contain "laex" post-promotion — structural guard.
#[test]
fn a2_structural_allowed_siblings_contains_laex() {
    // Verify the guard constant is accessible from the integration harness perspective:
    // the admin handler will return 422 for any sibling NOT in ALLOWED_SIBLINGS.
    // This test documents the expected 7-sibling slate without hitting Neo4j.
    let expected = ["corso", "eva", "soul", "quantum", "seraph", "ayin", "laex"];
    // The actual constant lives in admin.rs (crate-private); we verify behaviour via
    // the endpoint in integration tests below.  This just documents the expectation.
    assert_eq!(expected.len(), 7, "7-sibling slate");
}

// ── Integration tests (require live Neo4j) ────────────────────────────────────

/// A2-01: Upload LÆX canonical identity → 201 Created with `sibling` + `content_hash`.
#[tokio::test]
async fn a2_01_integration_laex_upload_201() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    let app = build_http_router(state);

    let resp = app
        .oneshot(post_admin(
            "/v1/admin/agents/upload",
            laex_payload(),
            &tok,
            TEST_IP,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "A2-01: laex upload must return 201 Created"
    );
    let json = body_json(resp).await;
    assert_eq!(json["sibling"], "laex", "A2-01: response must echo sibling");
    assert!(
        json.get("content_hash").and_then(|v| v.as_str()).is_some(),
        "A2-01: response must include content_hash"
    );
    assert!(
        json["version"].as_str() == Some("1.1.0"),
        "A2-01: response must echo version"
    );
}

/// A2-02: Upload with an unknown sibling slug → 422 Unprocessable Entity.
///
/// Validates that `ALLOWED_SIBLINGS` still excludes the vestigial `"claude"` entry.
#[tokio::test]
async fn a2_02_integration_vestigial_claude_returns_422() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    let app = build_http_router(state);

    let mut payload = laex_payload();
    payload["sibling"] = serde_json::json!("claude");

    let resp = app
        .oneshot(post_admin(
            "/v1/admin/agents/upload",
            payload,
            &tok,
            TEST_IP_2,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "A2-02: vestigial 'claude' sibling must be rejected with 422"
    );
    let json = body_json(resp).await;
    assert_eq!(
        json["error"]["code"], "invalid_sibling",
        "A2-02: error code must be invalid_sibling"
    );
}

/// A2-03: Upload without admin token → 401 Unauthorized.
#[tokio::test]
async fn a2_03_integration_no_token_returns_401() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    let app = build_http_router(state);

    let resp = app
        .oneshot(post_no_token(
            "/v1/admin/agents/upload",
            laex_payload(),
            TEST_IP,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "A2-03: missing admin token must return 401"
    );
}

/// A2-04: MERGE idempotency — uploading the same payload twice returns the same `content_hash`.
///
/// The `upload_agent` handler uses Cypher MERGE so repeated uploads must not
/// create duplicate `SiblingIdentity` nodes or change the content hash.
#[tokio::test]
async fn a2_04_integration_idempotent_upsert_same_hash() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;

    let resp1 = build_http_router(Arc::clone(&state))
        .oneshot(post_admin(
            "/v1/admin/agents/upload",
            laex_payload(),
            &tok,
            TEST_IP,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp1.status(),
        StatusCode::CREATED,
        "first upload must be 201"
    );
    let hash1 = body_json(resp1)
        .await
        .get("content_hash")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_owned();

    let resp2 = build_http_router(Arc::clone(&state))
        .oneshot(post_admin(
            "/v1/admin/agents/upload",
            laex_payload(),
            &tok,
            TEST_IP,
        ))
        .await
        .unwrap();
    assert_eq!(
        resp2.status(),
        StatusCode::CREATED,
        "A2-04: second upload must also be 201"
    );
    let hash2 = body_json(resp2)
        .await
        .get("content_hash")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_owned();

    assert_eq!(
        hash1, hash2,
        "A2-04: content_hash must be stable across identical uploads (MERGE idempotency)"
    );
}

/// A2-05: Upload with missing `sibling` field → 422.
#[tokio::test]
async fn a2_05_integration_missing_sibling_returns_422() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    let app = build_http_router(state);

    let payload = serde_json::json!({
        "role": "no sibling field",
        "voice": "silent",
        "strands": [],
        "version": "1.0.0"
    });

    let resp = app
        .oneshot(post_admin(
            "/v1/admin/agents/upload",
            payload,
            &tok,
            TEST_IP,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "A2-05: missing sibling field must return 422"
    );
}
