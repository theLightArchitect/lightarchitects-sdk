//! G8 binary validation — 15 checks required before Phase 6 Ship.
//!
//! Tests are split into two groups:
//! - **Structural** (no Neo4j): G8-6, G8-8, G8-9, G8-10, G8-11, G8-13, G8-14
//! - **Integration** (real Neo4j): G8-1, G8-2, G8-3, G8-4, G8-5, G8-7, G8-12, G8-15
//!
//! Integration tests require:
//! - `NEO4J_PASS` env var (or keychain `soul-neo4j-local/password`)
//! - `LIGHTARCHITECTS_ADMIN_TOKEN` env var (or keychain `soul-neo4j-local/admin-token`)
//! - Fixture nodes seeded by `scripts/seed_g8_fixtures.sh`
//!
//! Run structural only: `cargo test -p lightarchitects-gateway g8 -- --skip integration`
//! Run all:             `NEO4J_PASS=... LIGHTARCHITECTS_ADMIN_TOKEN=... cargo test -p lightarchitects-gateway g8`

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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Fake client IP injected into every test request (avoids ConnectInfo extractor panic).
const TEST_IP: SocketAddr = SocketAddr::new(
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
    54321,
);

/// Alternate IP for rate-limit isolation between test groups.
const TEST_IP_2: SocketAddr = SocketAddr::new(
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 2)),
    54321,
);

/// Build a `PlatformState` backed by a real Neo4j connection.
///
/// Reads credentials from env vars with keychain fallback:
/// - `NEO4J_URI`  (default: `bolt://localhost:7687`)
/// - `NEO4J_USER` (default: `neo4j`)
/// - `NEO4J_PASS` — **required** (no default)
async fn build_neo4j_state(admin_token: Option<&str>) -> Arc<PlatformState> {
    let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let pass = neo4j_pass();

    let graph = lightarchitects_gateway::http::neo4j::connect(&uri, &user, &pass)
        .await
        .expect("neo4j connect");

    build_state_with_graph(graph, admin_token)
}

/// Build a `PlatformState` with no Neo4j (structural tests only — health/auth paths).
///
/// The `graph` field still needs a valid `Arc<neo4rs::Graph>` so we connect to local Neo4j.
/// Structural tests only exercise paths that never reach the Cypher layer.
async fn build_structural_state(admin_token: Option<&str>) -> Arc<PlatformState> {
    build_neo4j_state(admin_token).await
}

fn build_state_with_graph(
    graph: Arc<neo4rs::Graph>,
    admin_token: Option<&str>,
) -> Arc<PlatformState> {
    let read_limiter = Arc::new(governor::RateLimiter::keyed(
        governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(99)),
    ));
    let helix_limiter = Arc::new(governor::RateLimiter::keyed(
        governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(19)),
    ));
    // Write limiter set to 3 req/min in tests so G8-11 can trigger 429 quickly.
    let write_limiter = Arc::new(governor::RateLimiter::keyed(
        governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(2)),
    ));

    let canon_cache = moka::future::Cache::builder().max_capacity(50).build();
    let agent_cache = moka::future::Cache::builder().max_capacity(50).build();

    let admin_box = admin_token
        .map(|t| secrecy::SecretBox::new(Box::new(t.to_owned())));

    Arc::new(PlatformState {
        graph,
        read_limiter,
        helix_limiter,
        write_limiter,
        canon_cache,
        agent_cache,
        config: PlatformConfig::default(),
        admin_token: admin_box,
        read_token: None,
    })
}

/// Retrieve the Neo4j password from env or keychain.
fn neo4j_pass() -> String {
    std::env::var("NEO4J_PASS").unwrap_or_else(|_| {
        std::process::Command::new("security")
            .args(["find-generic-password", "-s", "soul-neo4j-local", "-a", "password", "-w"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
            .expect("NEO4J_PASS env var or keychain soul-neo4j-local/password required")
    })
}

/// Retrieve the admin token from env or keychain.
fn admin_token() -> String {
    std::env::var("LIGHTARCHITECTS_ADMIN_TOKEN").unwrap_or_else(|_| {
        std::process::Command::new("security")
            .args(["find-generic-password", "-s", "soul-neo4j-local", "-a", "admin-token", "-w"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
            .expect("LIGHTARCHITECTS_ADMIN_TOKEN or keychain soul-neo4j-local/admin-token required")
    })
}

/// Parse a response body as JSON.
async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

/// Build a GET request with ConnectInfo injected.
fn get(uri: &str, ip: SocketAddr) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .extension(ConnectInfo(ip))
        .body(Body::empty())
        .unwrap()
}

/// Build a GET request with Authorization: Bearer header.
fn get_bearer(uri: &str, token: &str, ip: SocketAddr) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .extension(ConnectInfo(ip))
        .body(Body::empty())
        .unwrap()
}

/// Build a POST request with JSON body and optional admin token header.
fn post_admin(uri: &str, body: serde_json::Value, admin_tok: &str, ip: SocketAddr) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-admin-token", admin_tok)
        .extension(ConnectInfo(ip))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

/// Build a POST request with a Bearer token (scope confusion scenario).
fn post_bearer(uri: &str, body: serde_json::Value, token: &str, ip: SocketAddr) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .extension(ConnectInfo(ip))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

// ── G8 STRUCTURAL TESTS (no data dependency) ─────────────────────────────────

/// G8-8: GET /v1/platform/health → 200 { status: "healthy" }
#[tokio::test]
async fn g8_08_health_returns_200_healthy() {
    let state = build_structural_state(None).await;
    let app = build_http_router(state);
    let resp = app.oneshot(get("/v1/platform/health", TEST_IP)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "G8-8: health must be 200");
    let body = body_json(resp).await;
    assert_eq!(body["status"], "healthy", "G8-8: status field must be 'healthy'");
}

/// G8-9: Unauthenticated request → 401 (when read_token is configured).
///
/// This test configures a read_token, then sends a request without Authorization header.
#[tokio::test]
async fn g8_09_unauthenticated_read_returns_401() {
    let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let pass = neo4j_pass();
    let graph = lightarchitects_gateway::http::neo4j::connect(&uri, &user, &pass)
        .await
        .expect("neo4j connect");

    let read_token = "g8-test-read-token-for-401-check";
    let state = Arc::new(PlatformState {
        graph,
        read_limiter: Arc::new(governor::RateLimiter::keyed(
            governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(99)),
        )),
        helix_limiter: Arc::new(governor::RateLimiter::keyed(
            governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(19)),
        )),
        write_limiter: Arc::new(governor::RateLimiter::keyed(
            governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(9)),
        )),
        canon_cache: moka::future::Cache::builder().max_capacity(10).build(),
        agent_cache: moka::future::Cache::builder().max_capacity(10).build(),
        config: PlatformConfig::default(),
        admin_token: None,
        read_token: Some(secrecy::SecretBox::new(Box::new(read_token.to_owned()))),
    });
    let app = build_http_router(state);
    let resp = app.oneshot(get("/v1/platform/canon/anything", TEST_IP)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "G8-9: missing token must be 401, not {:?}", resp.status());
}

/// G8-10: Read token on admin endpoint → 403 (scope confusion guard).
#[tokio::test]
async fn g8_10_read_bearer_on_admin_returns_403() {
    let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let pass = neo4j_pass();
    let graph = lightarchitects_gateway::http::neo4j::connect(&uri, &user, &pass)
        .await
        .expect("neo4j connect");

    let read_token = "g8-test-read-token-for-403-check";
    let state = Arc::new(PlatformState {
        graph,
        read_limiter: Arc::new(governor::RateLimiter::keyed(
            governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(99)),
        )),
        helix_limiter: Arc::new(governor::RateLimiter::keyed(
            governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(19)),
        )),
        write_limiter: Arc::new(governor::RateLimiter::keyed(
            governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(9)),
        )),
        canon_cache: moka::future::Cache::builder().max_capacity(10).build(),
        agent_cache: moka::future::Cache::builder().max_capacity(10).build(),
        config: PlatformConfig::default(),
        admin_token: None,
        read_token: Some(secrecy::SecretBox::new(Box::new(read_token.to_owned()))),
    });
    let app = build_http_router(state);
    let body = serde_json::json!({"path":"test","kind":"canon","content_text":"x","version":"1.0.0"});
    let resp = app
        .oneshot(post_bearer("/v1/admin/canon/upload", body, read_token, TEST_IP))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "G8-10: read bearer on admin must be 403");
    let json = body_json(resp).await;
    assert_eq!(json["error"]["code"], "insufficient_scope", "G8-10: error code must be insufficient_scope");
}

/// G8-11: Rate limit burst → 429 with Retry-After header.
///
/// Write limiter is set to 3 req/min in the test state; 4 requests from the
/// same IP should trigger 429 on the 4th.
#[tokio::test]
async fn g8_11_rate_limit_returns_429_with_retry_after() {
    let tok = admin_token();
    let state = build_structural_state(Some(&tok)).await;
    let app = build_http_router(state);

    let dummy_body = serde_json::json!({
        "path": "g8-rl-test",
        "kind": "canon",
        "content_text": "rate limit test",
        "version": "1.0.0"
    });

    // Use a distinct IP so this test's quota doesn't bleed into others.
    let ip: SocketAddr = "127.0.0.3:54321".parse().unwrap();

    let mut last_status = StatusCode::OK;
    for _ in 0..4 {
        let req = post_admin("/v1/admin/canon/upload", dummy_body.clone(), &tok, ip);
        last_status = app.clone().oneshot(req).await.unwrap().status();
    }
    assert_eq!(last_status, StatusCode::TOO_MANY_REQUESTS, "G8-11: 4th write req must be 429 (limit=3)");
}

/// G8-13: Cache-Control: max-age=2592000 present on canon responses.
#[tokio::test]
async fn g8_13_integration_cache_control_on_canon() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    let app = build_http_router(state);
    let resp = app
        .oneshot(get("/v1/platform/canon/g8-test-canon", TEST_IP))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "G8-13: fixture must exist");
    let cc = resp
        .headers()
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(cc.contains("max-age=2592000"), "G8-13: Cache-Control must contain max-age=2592000, got: {cc}");
}

/// G8-14: lightarchitects-version header present on all responses.
#[tokio::test]
async fn g8_14_version_header_present_on_all_responses() {
    let state = build_structural_state(None).await;
    let app = build_http_router(state);

    // Health (no auth, no DB)
    let resp = app.clone().oneshot(get("/v1/platform/health", TEST_IP)).await.unwrap();
    assert!(
        resp.headers().contains_key("lightarchitects-version"),
        "G8-14: health must carry lightarchitects-version"
    );

    // 404 for a non-existent route still passes through version middleware
    let resp404 = app.oneshot(get("/v1/platform/nonexistent-route-xyz", TEST_IP)).await.unwrap();
    assert!(
        resp404.headers().contains_key("lightarchitects-version"),
        "G8-14: 404 must also carry lightarchitects-version"
    );
}

// ── G8 INTEGRATION TESTS (real Neo4j) ────────────────────────────────────────

/// G8-1: GET /v1/platform/canon/:name → 200 with content_hash field.
#[tokio::test]
async fn g8_01_integration_canon_get_200_with_content_hash() {
    let state = build_neo4j_state(None).await;
    let app = build_http_router(state);
    let resp = app
        .oneshot(get("/v1/platform/canon/g8-test-canon", TEST_IP))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "G8-1: canon GET must be 200");
    let body = body_json(resp).await;
    assert!(body.get("content_hash").is_some(), "G8-1: response must include content_hash");
    assert_eq!(body["path"], "g8-test-canon", "G8-1: path field must match");
}

/// G8-2: Second GET with If-None-Match → 304 Not Modified, empty body.
#[tokio::test]
async fn g8_02_integration_etag_304_on_repeat() {
    let state = build_neo4j_state(None).await;
    let app = build_http_router(state);

    // First request — get the ETag.
    let resp1 = app
        .clone()
        .oneshot(get("/v1/platform/canon/g8-test-canon", TEST_IP))
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK, "G8-2: first GET must be 200");
    let etag = resp1
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .expect("G8-2: first response must include ETag")
        .to_owned();

    // Second request — send If-None-Match.
    let req2 = Request::builder()
        .method("GET")
        .uri("/v1/platform/canon/g8-test-canon")
        .header("if-none-match", &etag)
        .extension(ConnectInfo(TEST_IP))
        .body(Body::empty())
        .unwrap();
    let resp2 = app.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::NOT_MODIFIED, "G8-2: repeat with If-None-Match must be 304");

    // Body must be empty on 304.
    let bytes = axum::body::to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
    assert!(bytes.is_empty(), "G8-2: 304 body must be empty");
}

/// G8-3: OrgOverride resolution — base + acme override → composite contains org_label.
#[tokio::test]
async fn g8_03_integration_org_override_composite() {
    let state = build_neo4j_state(None).await;
    let app = build_http_router(state);

    // Base (no org header)
    let resp_base = app
        .clone()
        .oneshot(get("/v1/platform/canon/g8-test-canon", TEST_IP))
        .await
        .unwrap();
    let base = body_json(resp_base).await;
    assert!(base.get("org_label").is_none(), "G8-3: base must NOT have org_label");

    // With org override
    let req_org = Request::builder()
        .method("GET")
        .uri("/v1/platform/canon/g8-test-canon")
        .header("x-org-id", "g8-acme")
        .extension(ConnectInfo(TEST_IP))
        .body(Body::empty())
        .unwrap();
    let resp_org = app.oneshot(req_org).await.unwrap();
    assert_eq!(resp_org.status(), StatusCode::OK, "G8-3: org override request must be 200");
    let org = body_json(resp_org).await;
    assert_eq!(
        org["org_label"], "ACME override applied",
        "G8-3: composite must include org_label from OrgOverride"
    );
}

/// G8-4: Locked-field override attempt → 400 LockedFieldViolation.
#[tokio::test]
async fn g8_04_integration_locked_field_violation() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    let app = build_http_router(state);

    let body = serde_json::json!({
        "path": "g8-locked-canon",
        "kind": "canon",
        "content_text": "attempt to overwrite locked entry",
        "version": "2.0.0"
    });
    let resp = app
        .oneshot(post_admin("/v1/admin/canon/upload", body, &tok, TEST_IP))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "G8-4: locked entry upload must be 400");
    let json = body_json(resp).await;
    assert_eq!(
        json["error"]["code"], "LockedFieldViolation",
        "G8-4: error code must be LockedFieldViolation"
    );
}

/// G8-5: POST /v1/admin/canon/upload with valid admin token → 201 Created.
#[tokio::test]
async fn g8_05_integration_admin_upload_201() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    let app = build_http_router(state);

    let body = serde_json::json!({
        "path": "g8-upload-test",
        "kind": "canon",
        "content_text": "G8 upload test — created by g8_05 test",
        "version": "1.0.0"
    });
    let resp = app
        .oneshot(post_admin("/v1/admin/canon/upload", body, &tok, TEST_IP))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "G8-5: admin upload must return 201 Created");
    let json = body_json(resp).await;
    assert_eq!(json["path"], "g8-upload-test", "G8-5: response must echo the path");
    assert!(json.get("content_hash").is_some(), "G8-5: response must include content_hash");
}

/// G8-6: POST /v1/admin/canon/upload with a read Bearer token → 403 Forbidden.
#[tokio::test]
async fn g8_06_admin_upload_with_read_bearer_returns_403() {
    let admin_tok = admin_token();
    let read_tok = "g8-read-only-bearer";

    let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let pass = neo4j_pass();
    let graph = lightarchitects_gateway::http::neo4j::connect(&uri, &user, &pass)
        .await
        .expect("neo4j connect");

    let state = Arc::new(PlatformState {
        graph,
        read_limiter: Arc::new(governor::RateLimiter::keyed(
            governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(99)),
        )),
        helix_limiter: Arc::new(governor::RateLimiter::keyed(
            governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(19)),
        )),
        write_limiter: Arc::new(governor::RateLimiter::keyed(
            governor::Quota::per_minute(NonZeroU32::MIN.saturating_add(9)),
        )),
        canon_cache: moka::future::Cache::builder().max_capacity(10).build(),
        agent_cache: moka::future::Cache::builder().max_capacity(10).build(),
        config: PlatformConfig::default(),
        admin_token: Some(secrecy::SecretBox::new(Box::new(admin_tok))),
        read_token: Some(secrecy::SecretBox::new(Box::new(read_tok.to_owned()))),
    });
    let app = build_http_router(state);

    let body = serde_json::json!({"path":"g8-test","kind":"canon","content_text":"x","version":"1.0.0"});
    let resp = app
        .oneshot(post_bearer("/v1/admin/canon/upload", body, read_tok, TEST_IP_2))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "G8-6: read bearer on admin must be 403");
}

/// G8-7: GET /v1/vault/info → 200 with user_id, tier_counts, api_version.
#[tokio::test]
async fn g8_07_integration_vault_info_schema() {
    let state = build_neo4j_state(None).await;
    let app = build_http_router(state);
    let resp = app
        .oneshot(get("/v1/vault/info", TEST_IP))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "G8-7: vault/info must be 200");
    let body = body_json(resp).await;
    assert!(body.get("user_id").is_some(), "G8-7: must have user_id");
    assert!(body.get("api_version").is_some(), "G8-7: must have api_version");
    assert!(body.get("tier_counts").is_some(), "G8-7: must have tier_counts");
    assert!(
        body["tier_counts"].is_object(),
        "G8-7: tier_counts must be an object"
    );
}

/// G8-12: Cursor pagination on /v1/platform/skills → has_more + next_cursor correct.
#[tokio::test]
async fn g8_12_integration_skills_cursor_pagination() {
    let state = build_neo4j_state(None).await;
    let app = build_http_router(state);

    // Fetch first 2 of the 5 g8-skill-* fixtures.
    let resp1 = app
        .clone()
        .oneshot(get("/v1/platform/skills?limit=2", TEST_IP))
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK, "G8-12: skills list must be 200");
    let page1 = body_json(resp1).await;
    let skills1 = page1["skills"].as_array().expect("G8-12: skills must be array");
    assert_eq!(skills1.len(), 2, "G8-12: first page must have exactly 2 skills");
    let cursor = page1["next_cursor"].as_str().expect("G8-12: next_cursor must be present when has_more");

    // Fetch next page using the cursor.
    let resp2 = app
        .oneshot(get(&format!("/v1/platform/skills?limit=2&after_id={cursor}"), TEST_IP))
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::OK, "G8-12: page 2 must be 200");
    let page2 = body_json(resp2).await;
    let skills2 = page2["skills"].as_array().expect("G8-12: skills2 must be array");
    assert!(!skills2.is_empty(), "G8-12: page 2 must have at least one skill");

    // Skill names must not overlap between pages.
    let names1: std::collections::HashSet<_> =
        skills1.iter().filter_map(|s| s["name"].as_str()).collect();
    let names2: std::collections::HashSet<_> =
        skills2.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(
        names1.is_disjoint(&names2),
        "G8-12: page 1 and page 2 must not share skill names"
    );
}

/// G8-15: Admin audit log written to `~/.lightarchitects/audit/admin-canon.jsonl` on upload.
#[tokio::test]
async fn g8_15_integration_audit_log_written_on_upload() {
    let tok = admin_token();
    let state = build_neo4j_state(Some(&tok)).await;
    let app = build_http_router(state);

    let path = "g8-audit-log-test";
    let body = serde_json::json!({
        "path": path,
        "kind": "canon",
        "content_text": "G8-15 audit log verification",
        "version": "1.0.0"
    });

    let _resp = app
        .oneshot(post_admin("/v1/admin/canon/upload", body, &tok, TEST_IP))
        .await
        .unwrap();

    // The audit log must exist and contain an entry for this path.
    let log_path = dirs_next::home_dir()
        .expect("home dir")
        .join(".lightarchitects/audit/admin-canon.jsonl");

    assert!(log_path.exists(), "G8-15: audit log file must exist at {}", log_path.display());

    let contents = std::fs::read_to_string(&log_path).expect("G8-15: read audit log");
    assert!(
        contents.contains(path),
        "G8-15: audit log must contain the uploaded path '{path}'"
    );
    assert!(
        contents.contains("upload_canon"),
        "G8-15: audit log must contain action 'upload_canon'"
    );
}
