//! [`PlatformClient`] — typed REST client for the platform API on `localhost:8080`.
//!
//! The platform API is a private HTTP server embedded in the `lightarchitects-gateway`
//! binary. It exposes canonical content, agent identities, skills, and standards
//! stored in local Neo4j, optionally filtered by org-specific overrides.
//!
//! # Example
//!
//! ```no_run
//! use lightarchitects::platform::{PlatformClient, HelixQueryParams};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = PlatformClient::builder().build()?;
//! let entry = client.canon("canon/builders-cookbook").await?;
//! println!("{}", entry.version);
//!
//! let page = client
//!     .helix_query(HelixQueryParams { kind: Some("decision".into()), ..Default::default() })
//!     .await?;
//! println!("{} helix entries", page.count);
//! # Ok(())
//! # }
//! ```
//!
//! # Filesystem cache
//!
//! Enable the optional filesystem mirror via [`PlatformClientBuilder::cache_dir`] or
//! [`PlatformClientBuilder::with_default_cache`]. When enabled every successful GET
//! response is written to `{cache_dir}/{key}.json` alongside a `{key}.etag` file.
//! Subsequent requests include `If-None-Match` and short-circuit on 304.

use std::path::PathBuf;
use std::time::Duration;

use reqwest::Client;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::error::PlatformError;
use super::types::{
    AgentEntry, AgentStrands, CanonEntry, HealthStatus, HelixPage, HelixQueryParams, SkillEntry,
    SkillsPage, StandardEntry, UploadCanonRequest, UploadCanonResponse, VaultInfo,
};

/// Default platform API base URL (gateway default port 8080).
pub const DEFAULT_PLATFORM_BASE_URL: &str = "http://localhost:8080";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

// ── Client ────────────────────────────────────────────────────────────────────

/// Typed REST client for the `lightarchitects-gateway` platform API.
///
/// Cheap to clone — the inner [`reqwest::Client`] is `Arc`-backed.
///
/// # Org overrides
///
/// Set [`PlatformClientBuilder::org_id`] to have every request include the
/// `X-Org-Id` header, enabling server-side per-org content overrides for canon
/// and agent endpoints.
///
/// # Filesystem cache
///
/// Set [`PlatformClientBuilder::cache_dir`] to enable ETag-based filesystem caching.
/// Cached responses are reused on 304 Not Modified, reducing Neo4j round-trips.
#[derive(Clone, Debug)]
pub struct PlatformClient {
    inner: Client,
    base_url: String,
    org_id: Option<String>,
    cache_dir: Option<PathBuf>,
}

impl PlatformClient {
    /// Start building a [`PlatformClient`].
    pub fn builder() -> PlatformClientBuilder {
        PlatformClientBuilder::default()
    }

    // ── Platform read endpoints ────────────────────────────────────────────────

    /// `GET /v1/platform/canon/:name` — canonical content with org override.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::NotFound`] if no entry exists for `name`.
    pub async fn canon(&self, name: &str) -> Result<CanonEntry, PlatformError> {
        self.get(format!("/v1/platform/canon/{name}")).await
    }

    /// `GET /v1/platform/agents/:sibling` — full agent identity with org override.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::NotFound`] if `sibling` is not registered.
    pub async fn agent(&self, sibling: &str) -> Result<AgentEntry, PlatformError> {
        self.get(format!("/v1/platform/agents/{sibling}")).await
    }

    /// `GET /v1/platform/agents/:sibling/strands` — strands list only.
    ///
    /// More efficient than [`PlatformClient::agent`] when only strand labels
    /// are needed; the gateway reuses its agent cache for this endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::NotFound`] if `sibling` is not registered.
    pub async fn agent_strands(&self, sibling: &str) -> Result<AgentStrands, PlatformError> {
        self.get(format!("/v1/platform/agents/{sibling}/strands"))
            .await
    }

    /// `GET /v1/platform/skills` — cursor-paginated published skills.
    ///
    /// Pass `after_id` (the previous page's `next_cursor`) to advance the cursor.
    /// `limit` is server-capped at 100 and defaults to 50.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Http`] on server errors.
    pub async fn skills(
        &self,
        after_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<SkillsPage, PlatformError> {
        self.get_with_query("/v1/platform/skills", &SkillsParams { after_id, limit })
            .await
    }

    /// `GET /v1/platform/skills/:name` — single published skill by name.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::NotFound`] if `name` is not registered.
    pub async fn skill(&self, name: &str) -> Result<SkillEntry, PlatformError> {
        self.get(format!("/v1/platform/skills/{name}")).await
    }

    /// `GET /v1/platform/standards/:name` — canonical standard document.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::NotFound`] if `name` is not registered.
    pub async fn standard(&self, name: &str) -> Result<StandardEntry, PlatformError> {
        self.get(format!("/v1/platform/standards/{name}")).await
    }

    /// `GET /v1/platform/helix/query` — filtered helix entry listing.
    ///
    /// All [`HelixQueryParams`] fields are optional; omitting them returns the
    /// 20 most recent entries across all kinds and tiers.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Http`] on server errors.
    pub async fn helix_query(&self, params: HelixQueryParams) -> Result<HelixPage, PlatformError> {
        // Filtered — bypass cache; query params form part of the cache key, not modelled here.
        self.get_with_query("/v1/platform/helix/query", &params)
            .await
    }

    /// `GET /v1/platform/health` — liveness probe; no Neo4j dependency.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Http`] if the server cannot be reached.
    pub async fn health(&self) -> Result<HealthStatus, PlatformError> {
        // Health endpoint carries no ETag; skip cache.
        parse_response(
            self.apply_org_header(self.inner.get(self.url("/v1/platform/health")))
                .send()
                .await
                .map_err(|e| PlatformError::Http(e.to_string()))?,
            "/v1/platform/health",
        )
        .await
    }

    /// `GET /v1/vault/info` — Neo4j node-count summary.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Http`] on server or database errors.
    pub async fn vault_info(&self) -> Result<VaultInfo, PlatformError> {
        self.get("/v1/vault/info".to_owned()).await
    }

    // ── Admin endpoint ─────────────────────────────────────────────────────────

    /// `POST /v1/admin/canon/upload` — upsert a `PlatformEntry` node.
    ///
    /// Only available when the gateway is running in local mode. The gateway
    /// enforces localhost-origin constraints via CORS.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Http`] on server or database errors.
    pub async fn upload_canon(
        &self,
        req: UploadCanonRequest,
    ) -> Result<UploadCanonResponse, PlatformError> {
        self.post("/v1/admin/canon/upload", &req).await
    }

    // ── Internal helpers ───────────────────────────────────────────────────────

    fn url(&self, path: impl AsRef<str>) -> String {
        format!("{}{}", self.base_url, path.as_ref())
    }

    fn apply_org_header(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(org) = &self.org_id {
            req.header("x-org-id", org.as_str())
        } else {
            req
        }
    }

    /// GET with optional ETag-based filesystem cache.
    ///
    /// Sends `If-None-Match` when a cached `ETag` exists. On 304, returns the
    /// cached value without deserializing a body. On 200, writes the new value
    /// and `ETag` to the cache directory before returning.
    async fn get<T>(&self, path: impl AsRef<str>) -> Result<T, PlatformError>
    where
        T: DeserializeOwned + Serialize,
    {
        let path = path.as_ref();
        let key = self.cache_key(path);
        let cached = self.read_cached::<T>(&key);

        let url = self.url(path);
        let mut builder = self.apply_org_header(self.inner.get(&url));
        if let Some((_, ref etag)) = cached {
            builder = builder.header(reqwest::header::IF_NONE_MATCH, etag.as_str());
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| PlatformError::Http(e.to_string()))?;

        let status = resp.status();
        let status_u16 = status.as_u16();

        if status_u16 == 304 {
            return cached
                .map(|(v, _)| v)
                .ok_or_else(|| PlatformError::Http("304 without cached copy".into()));
        }
        if status_u16 == 404 {
            return Err(PlatformError::NotFound(path.to_owned()));
        }
        if status_u16 == 429 {
            let secs = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);
            return Err(PlatformError::RateLimited {
                retry_after_secs: secs,
            });
        }

        let etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PlatformError::Http(format!("HTTP {status_u16}: {body}")));
        }

        let value = resp
            .json::<T>()
            .await
            .map_err(|e| PlatformError::Http(e.to_string()))?;

        if let Some(ref e) = etag {
            self.write_cached(&key, &value, e);
        }

        Ok(value)
    }

    async fn get_with_query<Q, T>(&self, path: &str, query: &Q) -> Result<T, PlatformError>
    where
        Q: Serialize,
        T: DeserializeOwned,
    {
        let url = self.url(path);
        let req = self.apply_org_header(self.inner.get(&url)).query(query);
        let resp = req
            .send()
            .await
            .map_err(|e| PlatformError::Http(e.to_string()))?;
        parse_response(resp, path).await
    }

    async fn post<B, T>(&self, path: &str, body: &B) -> Result<T, PlatformError>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        let url = self.url(path);
        let req = self.apply_org_header(self.inner.post(&url)).json(body);
        let resp = req
            .send()
            .await
            .map_err(|e| PlatformError::Http(e.to_string()))?;
        parse_response(resp, path).await
    }

    // ── Cache helpers ─────────────────────────────────────────────────────────

    /// Derive a filesystem-safe cache key from the request path and `org_id`.
    ///
    /// Leading slash and internal slashes are replaced with `__`. The `org_id` is
    /// appended with an `__org__` separator so org-scoped and unscoped entries
    /// never collide.
    fn cache_key(&self, path: &str) -> String {
        let normalized = path.trim_start_matches('/').replace('/', "__");
        if let Some(org) = &self.org_id {
            format!("{normalized}__org__{org}")
        } else {
            normalized
        }
    }

    /// Read a cached value and its `ETag` from the cache directory.
    ///
    /// Returns `None` on any I/O or deserialization error so callers degrade
    /// gracefully to a live fetch.
    fn read_cached<T: DeserializeOwned>(&self, key: &str) -> Option<(T, String)> {
        let dir = self.cache_dir.as_ref()?;
        let bytes = std::fs::read(dir.join(format!("{key}.json"))).ok()?;
        let etag = std::fs::read_to_string(dir.join(format!("{key}.etag"))).ok()?;
        let value = serde_json::from_slice::<T>(&bytes).ok()?;
        Some((value, etag.trim().to_owned()))
    }

    /// Write a value and its `ETag` to the cache directory.
    ///
    /// Errors are silently swallowed — a cache write failure must never propagate
    /// to the caller; the returned value is already correct.
    fn write_cached<T: Serialize>(&self, key: &str, value: &T, etag: &str) {
        let Some(dir) = &self.cache_dir else { return };
        let _ = std::fs::create_dir_all(dir);
        if let Ok(bytes) = serde_json::to_vec(value) {
            let _ = std::fs::write(dir.join(format!("{key}.json")), &bytes);
            let _ = std::fs::write(dir.join(format!("{key}.etag")), etag);
        }
    }
}

// ── Shared response helper ────────────────────────────────────────────────────

/// Inspect the HTTP status, map 404/429 to typed errors, then deserialize.
///
/// Used by non-cached paths (`get_with_query`, `post`, `health`).
async fn parse_response<T: DeserializeOwned>(
    resp: reqwest::Response,
    display_path: &str,
) -> Result<T, PlatformError> {
    let status = resp.status();
    let status_u16 = status.as_u16();
    if status_u16 == 404 {
        return Err(PlatformError::NotFound(display_path.to_owned()));
    }
    if status_u16 == 429 {
        let secs = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(60);
        return Err(PlatformError::RateLimited {
            retry_after_secs: secs,
        });
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(PlatformError::Http(format!("HTTP {status_u16}: {body}")));
    }
    resp.json::<T>()
        .await
        .map_err(|e| PlatformError::Http(e.to_string()))
}

// ── Internal query param types ────────────────────────────────────────────────

#[derive(Serialize)]
struct SkillsParams<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    after_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
}

// ── Builder ───────────────────────────────────────────────────────────────────

/// Builder for [`PlatformClient`].
#[derive(Debug)]
pub struct PlatformClientBuilder {
    base_url: String,
    org_id: Option<String>,
    timeout: Duration,
    cache_dir: Option<PathBuf>,
}

impl Default for PlatformClientBuilder {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_PLATFORM_BASE_URL.to_owned(),
            org_id: None,
            timeout: DEFAULT_TIMEOUT,
            cache_dir: None,
        }
    }
}

impl PlatformClientBuilder {
    /// Override the gateway base URL.
    ///
    /// Default: `http://localhost:8080`.
    #[must_use]
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the org identifier sent as `X-Org-Id` on every request.
    ///
    /// Required to receive per-org content overrides from canon and agent endpoints.
    #[must_use]
    pub fn org_id(mut self, org: impl Into<String>) -> Self {
        self.org_id = Some(org.into());
        self
    }

    /// Override the HTTP request timeout.
    ///
    /// Default: 30 s.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Enable the filesystem `ETag` cache, storing files in `dir`.
    ///
    /// The directory is created on first write. Each cached path produces two
    /// files: `{key}.json` (the serialized response) and `{key}.etag`.
    #[must_use]
    pub fn cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(dir.into());
        self
    }

    /// Enable the filesystem cache at the default location:
    /// `~/.lightarchitects/platform-cache/`.
    ///
    /// Falls back to no cache if the home directory cannot be determined.
    #[must_use]
    pub fn with_default_cache(mut self) -> Self {
        if let Some(home) = dirs::home_dir() {
            self.cache_dir = Some(home.join(".lightarchitects/platform-cache"));
        }
        self
    }

    /// Build the [`PlatformClient`].
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Config`] if the base URL is empty or the HTTP
    /// client cannot be constructed.
    pub fn build(self) -> Result<PlatformClient, PlatformError> {
        if self.base_url.is_empty() {
            return Err(PlatformError::Config("base_url must not be empty".into()));
        }
        let inner = Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| PlatformError::Config(format!("failed to build HTTP client: {e}")))?;
        Ok(PlatformClient {
            inner,
            base_url: self.base_url,
            org_id: self.org_id,
            cache_dir: self.cache_dir,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::platform::types::CanonEntry;

    #[test]
    fn builder_default_base_url() {
        let client = PlatformClient::builder().build().unwrap();
        assert_eq!(client.base_url, DEFAULT_PLATFORM_BASE_URL);
    }

    #[test]
    fn builder_rejects_empty_base_url() {
        let result = PlatformClient::builder().base_url("").build();
        assert!(matches!(result, Err(PlatformError::Config(_))));
    }

    #[test]
    fn builder_stores_org_id() {
        let client = PlatformClient::builder()
            .org_id("acme-corp")
            .build()
            .unwrap();
        assert_eq!(client.org_id.as_deref(), Some("acme-corp"));
    }

    #[test]
    fn url_construction() {
        let client = PlatformClient::builder()
            .base_url("http://localhost:9090")
            .build()
            .unwrap();
        assert_eq!(
            client.url("/v1/platform/health"),
            "http://localhost:9090/v1/platform/health"
        );
    }

    #[test]
    fn cache_key_no_org() {
        let client = PlatformClient::builder().build().unwrap();
        assert_eq!(
            client.cache_key("/v1/platform/canon/builders-cookbook"),
            "v1__platform__canon__builders-cookbook"
        );
    }

    #[test]
    fn cache_key_with_org() {
        let client = PlatformClient::builder()
            .org_id("acme-corp")
            .build()
            .unwrap();
        assert_eq!(
            client.cache_key("/v1/platform/canon/builders-cookbook"),
            "v1__platform__canon__builders-cookbook__org__acme-corp"
        );
    }

    #[test]
    fn cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let client = PlatformClient::builder()
            .cache_dir(dir.path())
            .build()
            .unwrap();

        let entry = CanonEntry {
            path: "test".into(),
            kind: "canon".into(),
            content_json: None,
            content_text: Some("hello".into()),
            version: "1.0.0".into(),
            updated_at: "2026-05-04T00:00:00Z".into(),
            content_hash: "abc".into(),
        };
        let key = "test_key";

        client.write_cached(key, &entry, "\"abc123\"");
        let (loaded, etag): (CanonEntry, String) = client.read_cached(key).unwrap();
        assert_eq!(loaded.path, "test");
        assert_eq!(etag, "\"abc123\"");
    }
}
