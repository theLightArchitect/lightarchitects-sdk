//! Shared state for the Platform HTTP server.

use governor::{RateLimiter, clock::DefaultClock, state::keyed::DefaultKeyedStateStore};
use lightarchitects::helix::{EmbeddingProvider, HelixCache, HelixDb};
use secrecy::SecretBox;
use serde_json::Value;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::IdentityScopePolicy;
use crate::http::circuit_breaker::CircuitBreaker;

/// Per-IP keyed rate limiter instance type.
pub type PlatformRateLimiter = RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>;

/// LRU response cache keyed on `(content_key, org_id)`, 60 s TTL.
///
/// Canon and agent content use separate instances so that a canon path that
/// happens to equal a sibling name (e.g. `"SOUL"`) never collides.
pub type ResponseCache = moka::future::Cache<(String, String), Arc<Value>>;

/// Shared state injected into every platform HTTP handler via `axum::State`.
pub struct PlatformState {
    /// Pooled Neo4j connection (neo4rs built-in pool; supports bolt:// + neo4j+s://).
    pub graph: Arc<neo4rs::Graph>,
    /// Per-IP rate limiter for read endpoints (100 req/min).
    pub read_limiter: Arc<PlatformRateLimiter>,
    /// Per-IP rate limiter for helix/query and vault/info (20 req/min).
    pub helix_limiter: Arc<PlatformRateLimiter>,
    /// Per-IP rate limiter for admin write endpoints (10 req/min).
    pub write_limiter: Arc<PlatformRateLimiter>,
    /// Per-IP rate limiter for authentication failures (5/min per IP).
    ///
    /// Tracks rapid auth failures separate from general read rate-limiting.
    /// Checked on every 401 response; consumed tokens are NOT returned on success.
    pub auth_fail_limiter: Arc<PlatformRateLimiter>,
    /// Per-IP rate limiter for the skill upload endpoint (≤1 req/sec — SERAPH F-MEDIUM-3).
    ///
    /// Checked in `rate_limit_middleware` before the general `write_limiter` so burst
    /// skill uploads (which could fill Neo4j with large text blobs) are throttled more
    /// tightly than ordinary admin writes.
    pub skills_limiter: Arc<PlatformRateLimiter>,
    /// Per-IP authentication failure counter for hard-lockout enforcement.
    ///
    /// Incremented on every 401. Reset to zero on successful authentication for
    /// the same IP. When the count reaches 20 the IP receives HTTP 429 with no
    /// further `Authorization` attempts accepted until the counter is reset.
    pub auth_fail_counts: Arc<dashmap::DashMap<IpAddr, u32>>,
    /// Neo4j circuit breaker — shared across all handlers.
    ///
    /// Trips to Open after 5 consecutive failures; allows one probe after 30 s
    /// (HalfOpen); closes on the first successful query.
    pub circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    /// Override-aware response cache for `/v1/platform/canon/*`.
    pub canon_cache: ResponseCache,
    /// Override-aware response cache for `/v1/platform/agents/*`.
    pub agent_cache: ResponseCache,
    /// Arch model response cache for `/v1/platform/arch/*` — 5 min TTL.
    pub arch_cache: ResponseCache,
    /// Platform server configuration.
    pub config: PlatformConfig,
    /// Admin token for `POST /v1/admin/*` endpoints.
    ///
    /// When `None`, admin endpoints return 503 (no token configured).
    pub admin_token: Option<SecretBox<String>>,
    /// Bearer read token for all non-admin, non-health endpoints.
    ///
    /// When `None`, read endpoints are freely accessible (localhost trust model).
    /// When `Some`, requests without a valid `Authorization: Bearer <token>` header
    /// receive HTTP 401.
    pub read_token: Option<SecretBox<String>>,
    /// Helix-domain database accessor — used by `CachedRetriever` in retrieve handlers.
    ///
    /// Separate from `graph` (raw `neo4rs::Graph`) so helix operations use the
    /// typed `HelixDb` interface with allowlist enforcement.
    /// `HelixDb: Send + Sync` (supertrait) so `Arc<dyn>` is thread-safe.
    pub helix_db: Arc<dyn HelixDb>,
    /// `TinyLFU` byte-weight cache for helix retrieve results.
    ///
    /// Shared across all helix retrieve handlers. Hit ratio flows to the
    /// `soul.helix.retrieve` AYIN span via `CachedRetriever`.
    pub helix_cache: HelixCache,
    /// Embedding provider — used by `CachedRetriever` on cache miss.
    ///
    /// Backend selected from `PlatformConfig::embedding_backend` at startup.
    /// `EmbeddingProvider: Send + Sync` (supertrait) so `Arc<dyn>` is thread-safe.
    pub embedding_provider: Arc<dyn EmbeddingProvider>,
}

/// Configuration for the platform HTTP server.
#[derive(Clone, Debug)]
pub struct PlatformConfig {
    /// TCP port to bind (default: 8080).
    pub port: u16,
    /// Neo4j URI (bolt://localhost:7687 for local; neo4j+s://... for Aura — no code change).
    pub neo4j_uri: String,
    /// ISO date string injected as `lightarchitects-version` response header.
    pub version_date: String,
    /// Platform API version string returned in `/v1/vault/info`.
    pub api_version: &'static str,
    /// User identifier returned in `/v1/vault/info` (system user or env override).
    pub user_id: String,
    /// Identity scope policy for unauthenticated requests.
    pub identity_scope_policy: IdentityScopePolicy,
    /// Max in-memory bytes for the helix retrieve `TinyLFU` cache (default 64 MiB).
    pub helix_cache_max_capacity_bytes: u64,
    /// TTL in seconds for helix cache entries (default 300 s = 5 min).
    pub helix_cache_ttl_secs: u64,
    /// Embedding backend: `"fastembed"` | `"ollama"` | `"mock"` (default `"fastembed"`).
    pub embedding_backend: String,
    /// Embedding model tag interpreted by the chosen backend (default `"BGESmallENV15"`).
    pub embedding_model: String,
    /// Expected embedding dimensionality — validated against the backend at startup.
    pub embedding_dim: usize,
}

impl Default for PlatformConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            neo4j_uri: "bolt://localhost:7687".to_owned(),
            version_date: "2026-05-04".to_owned(),
            api_version: "v1",
            user_id: "local".to_owned(),
            identity_scope_policy: IdentityScopePolicy::AllowAuthenticated,
            helix_cache_max_capacity_bytes: 67_108_864, // 64 MiB
            helix_cache_ttl_secs: 300,
            embedding_backend: "fastembed".to_owned(),
            embedding_model: "BGESmallENV15".to_owned(),
            embedding_dim: 384,
        }
    }
}
