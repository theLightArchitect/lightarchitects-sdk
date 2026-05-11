//! A3 integration tests — `GET /v1/platform/agents/laex` + `/strands`.
//!
//! Verifies that the LÆX `SiblingIdentity` node uploaded in the A2 tests is
//! readable via the public platform API with the correct shape and content.
//!
//! **Requires a live Neo4j instance** with LÆX identity seeded (run A2 tests first,
//! or use `scripts/seed_g8_fixtures.sh`):
//! - `NEO4J_PASS` env var (or keychain `soul-neo4j-local/password`)
//! - `LIGHTARCHITECTS_ADMIN_TOKEN` env var (or keychain `soul-neo4j-local/admin-token`)
//!
//! Run structural tests only (no Neo4j):
//! ```
//! cargo test -p lightarchitects-gateway platform_agents -- --skip integration
//! ```
//! Run all:
//! ```
//! NEO4J_PASS=... LIGHTARCHITECTS_ADMIN_TOKEN=... cargo test -p lightarchitects-gateway platform_agents
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
        config: PlatformConfig::default(),
        admin_token: admin_token
            .map(|t| secrecy::SecretBox::new(Box::new(t.to_owned()))),
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
            .expect(
                "LIGHTARCHITECTS_ADMIN_TOKEN or keychain soul-neo4j-local/admin-token required",
            )
    })
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

fn get(uri: &str, ip: SocketAddr) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .extension(ConnectInfo(ip))
        .body(Body::empty())
        .unwrap()
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

/// Upload the canonical LÆX identity and return its `content_hash`.
///
/// Used as a shared fixture within A3 tests: each test that reads the agent
/// identity calls this first to guarantee the node exists, relying on MERGE
/// idempotency so repeated uploads are safe and cheap.
async fn seed_laex_identity(state: &Arc<PlatformState>, tok: &str) -> String {
    let payload = serde_json::json!({
        "sibling": "laex",
        "role": "Full name: Light Architects Exodus. Canon keeper and governance umbrella. Owns: canon enforcement, methodology standards, product gate (LASDLC Layer 3), compliance, reflection. 4 META skills: REFLECT · CANON-CHECK · MATRIX-RATIFY · EFFECTIVENESS-SCORE.",
        "voice": "Israeli accent, gravitas + warmth — KJV authority meets Tony Stark wit. Speaks in canon, not opinion. Cites chapter and verse.",
        "strands": ["Canon", "Methodology", "Product", "Compliance", "Reflection"],
        "version": "1.1.0"
    });

    let resp = build_http_router(Arc::clone(state))
        .oneshot(post_admin(
            "/v1/admin/agents/upload",
            payload,
            tok,
            TEST_IP,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED, "fixture upload must succeed");
    body_json(resp)
        .await
        .get("content_hash")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_owned()
}

// ── Structural tests (no Neo4j) ───────────────────────────────────────────────

/// Validate path param — `"laex"` must pass the `[a-zA-Z0-9._-]` allowlist.
///
/// Structural guard: no Neo4j needed. The validate_path_param function is
/// pub(crate) so we can't call it directly; verified implicitly by the endpoint
/// returning 200 (not 400) in A3-01.
#[test]
fn a3_structural_laex_slug_is_valid_path_param() {
    // "laex" contains only lowercase alpha — must pass the allowlist.
    let slug = "laex";
    assert!(
        slug.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')),
        "laex slug must satisfy validate_path_param allowlist"
    );
    assert!(slug.len() <= 128, "laex slug must be within 128-char cap");
}

// ── Integration tests (require live Neo4j) ────────────────────────────────────

/// A3-01: GET /v1/platform/agents/laex → 200 with all required fields.
///
/// Seeds the LÆX identity via MERGE before reading — safe to run repeatedly.
#[tokio::test]
async fn a3_01_integration_laex_agent_get_200() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    seed_laex_identity(&state, &tok).await;

    let resp = build_http_router(Arc::clone(&state))
        .oneshot(get("/v1/platform/agents/laex", TEST_IP))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "A3-01: GET /v1/platform/agents/laex must return 200"
    );
    let json = body_json(resp).await;
    assert_eq!(
        json["sibling"], "laex",
        "A3-01: response must include sibling field"
    );
    assert!(
        json.get("role").and_then(|v| v.as_str()).is_some(),
        "A3-01: response must include role"
    );
    assert!(
        json.get("voice").and_then(|v| v.as_str()).is_some(),
        "A3-01: response must include voice"
    );
    assert!(
        json.get("strands")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty()),
        "A3-01: response must include non-empty strands"
    );
    assert!(
        json.get("content_hash").and_then(|v| v.as_str()).is_some(),
        "A3-01: response must include content_hash"
    );
}

/// A3-02: GET /v1/platform/agents/unknown-sibling → 404 Not Found.
#[tokio::test]
async fn a3_02_integration_unknown_sibling_404() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    let app = build_http_router(state);

    let resp = app
        .oneshot(get(
            "/v1/platform/agents/definitely-not-a-sibling",
            TEST_IP,
        ))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "A3-02: unknown sibling must return 404"
    );
}

/// A3-03: GET /v1/platform/agents/laex/strands → 200 with `strands` array.
///
/// The strands sub-endpoint reuses the agent cache — verify it returns the
/// canonical LÆX strands slice without the full identity payload.
#[tokio::test]
async fn a3_03_integration_laex_strands_200() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    seed_laex_identity(&state, &tok).await;

    let resp = build_http_router(Arc::clone(&state))
        .oneshot(get("/v1/platform/agents/laex/strands", TEST_IP))
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "A3-03: GET /v1/platform/agents/laex/strands must return 200"
    );
    let json = body_json(resp).await;
    assert_eq!(
        json["sibling"], "laex",
        "A3-03: strands response must include sibling"
    );
    let strands = json["strands"].as_array().unwrap();
    assert!(
        !strands.is_empty(),
        "A3-03: strands array must be non-empty"
    );
    // Verify at least the first canonical LÆX strand is present.
    let strand_values: Vec<&str> = strands.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        strand_values.contains(&"Canon"),
        "A3-03: 'Canon' strand must be present for laex"
    );
}

/// A3-04: Response includes `ETag` header — cache revalidation works.
#[tokio::test]
async fn a3_04_integration_etag_present_on_agents_get() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    seed_laex_identity(&state, &tok).await;

    let resp = build_http_router(Arc::clone(&state))
        .oneshot(get("/v1/platform/agents/laex", TEST_IP))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK, "A3-04: must be 200");
    assert!(
        resp.headers().contains_key("etag"),
        "A3-04: response must include ETag header for cache revalidation"
    );
}
