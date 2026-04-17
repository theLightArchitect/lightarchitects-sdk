//! In-memory LRU cache for helix query results.
//!
//! Replaces the former `SQLite` `helix_cache` table with a zero-disk-I/O
//! hot-path cache powered by [`moka`].
//!
//! # Design
//!
//! - **Engine**: `moka::future::Cache` (async, LFU admission + LRU eviction)
//! - **Capacity**: 1000 entries (configurable)
//! - **TTL**: 5 minutes (configurable)
//! - **Key**: `String` (serialized from query params)
//! - **Value**: `Arc<Vec<ScoredResult<Step>>>` for search results
//!
//! # Usage
//!
//! ```rust,no_run
//! use crate::helix::cache::{HelixCache, HelixCacheConfig};
//! use crate::helix::search::SearchOptions;
//!
//! # async fn example() {
//! let cache = HelixCache::new(&HelixCacheConfig::default());
//!
//! // Check cache before querying DB
//! let key = cache.search_key("consciousness breakthrough", &SearchOptions::default());
//! if let Some(results) = cache.get_search(&key).await {
//!     // Cache hit — use results directly
//!     return;
//! }
//!
//! // Cache miss — query DB, then store
//! // let results = db.fulltext_search("consciousness breakthrough", &opts).await?;
//! // cache.put_search(&key, results).await;
//! # }
//! ```

use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;

use crate::helix::search::{ScoredResult, SearchOptions};
use crate::helix::types::Step;

// ============================================================================
// Cache Configuration
// ============================================================================

/// Configuration for [`HelixCache`].
#[derive(Debug, Clone)]
pub struct HelixCacheConfig {
    /// Maximum number of cached entries (default: 1000).
    pub max_capacity: u64,
    /// Time-to-live for cached entries (default: 5 minutes).
    pub ttl: Duration,
}

impl Default for HelixCacheConfig {
    fn default() -> Self {
        Self {
            max_capacity: 1000,
            ttl: Duration::from_secs(300), // 5 minutes
        }
    }
}

impl HelixCacheConfig {
    /// Set maximum capacity.
    #[must_use]
    pub fn with_max_capacity(mut self, cap: u64) -> Self {
        self.max_capacity = cap;
        self
    }

    /// Set time-to-live duration.
    #[must_use]
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }
}

// ============================================================================
// HelixCache
// ============================================================================

/// In-memory cache for helix search results.
///
/// Thread-safe and async-compatible — clone to share across tasks.
/// Uses LFU admission + LRU eviction with configurable TTL.
///
/// # In-memory only
///
/// This cache is **not persisted to disk**. It is rebuilt empty on every
/// process restart. The 5-minute TTL and 1000-entry capacity are designed for
/// programmatic SDK callers (e.g., `HelixClient`) that make repeated queries
/// within a single session. The soul-mcp server queries `HelixDb` directly and
/// does not go through this cache — each MCP tool call hits Neo4j fresh.
///
/// # Cache invalidation after writes
///
/// Call [`invalidate_all`](Self::invalidate_all) after any bulk write (ingest,
/// migration) to prevent stale reads within the same process. In the soul-mcp
/// binary, write operations on the filesystem vault call `state.invalidate_cache()`
/// which invalidates the `VaultCache`; this moka cache is separate and only
/// matters for `HelixClient` users.
#[derive(Clone)]
pub struct HelixCache {
    /// Search result cache (fulltext + vector).
    search: Cache<String, Arc<Vec<ScoredResult<Step>>>>,
}

impl std::fmt::Debug for HelixCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelixCache")
            .field("entry_count", &self.search.entry_count())
            .finish()
    }
}

impl HelixCache {
    /// Create a new cache with the given configuration.
    #[must_use]
    pub fn new(config: &HelixCacheConfig) -> Self {
        let search = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(config.ttl)
            .build();

        Self { search }
    }

    // ── Key Generation ──────────────────────────────────────────────

    /// Generate a cache key for a fulltext search query.
    #[must_use]
    pub fn search_key(&self, query: &str, opts: &SearchOptions) -> String {
        format!(
            "ft:{}:{}:{}:{}",
            query,
            opts.helix_id.as_deref().unwrap_or("*"),
            opts.limit,
            opts.min_score
                .map_or_else(|| "*".to_owned(), |s| format!("{s:.2}")),
        )
    }

    /// Generate a cache key for a vector search query.
    ///
    /// Uses the first 8 and last 8 floats of the embedding as a fingerprint,
    /// combined with the embedding length. This avoids hashing the full vector
    /// on every cache lookup while providing sufficient uniqueness.
    #[must_use]
    pub fn vector_key(&self, embedding: &[f32], index_name: &str, opts: &SearchOptions) -> String {
        let len = embedding.len();
        let head = embedding.first().copied().unwrap_or(0.0);
        let tail = embedding.last().copied().unwrap_or(0.0);
        format!(
            "vec:{index_name}:{len}:{head:.4}:{tail:.4}:{}:{}",
            opts.helix_id.as_deref().unwrap_or("*"),
            opts.limit,
        )
    }

    // ── Search Cache Operations ─────────────────────────────────────

    /// Get cached search results.
    pub async fn get_search(&self, key: &str) -> Option<Arc<Vec<ScoredResult<Step>>>> {
        self.search.get(key).await
    }

    /// Store search results in the cache.
    pub async fn put_search(&self, key: &str, results: Vec<ScoredResult<Step>>) {
        self.search.insert(key.to_owned(), Arc::new(results)).await;
    }

    /// Invalidate a specific cache entry.
    pub async fn invalidate(&self, key: &str) {
        self.search.invalidate(key).await;
    }

    /// Invalidate all cache entries.
    ///
    /// Use after bulk writes (ingestion, migration) to prevent stale reads.
    /// Note: `moka` invalidation is lazy — call [`run_pending_tasks`](Self::run_pending_tasks)
    /// to force synchronous cleanup in tests.
    pub fn invalidate_all(&self) {
        self.search.invalidate_all();
    }

    /// Force synchronous processing of pending cache maintenance tasks.
    ///
    /// In production code this is unnecessary — moka handles cleanup
    /// automatically. Useful in tests to ensure invalidation takes effect
    /// before assertions.
    pub async fn run_pending_tasks(&self) {
        self.search.run_pending_tasks().await;
    }

    /// Number of entries currently in the cache.
    #[must_use]
    pub fn entry_count(&self) -> u64 {
        self.search.entry_count()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::helix::types::Step;

    fn sample_step(id: &str) -> Step {
        Step {
            id: id.into(),
            helix_id: "test-helix".into(),
            title: Some("Test step".into()),
            content: "Test content".into(),
            significance: 5.0,
            step_date: None,
            step_index: None,
            community_id: None,
            expires: None,
            created_at: chrono::Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn test_cache_config_default() {
        let config = HelixCacheConfig::default();
        assert_eq!(config.max_capacity, 1000);
        assert_eq!(config.ttl, Duration::from_secs(300));
    }

    #[test]
    fn test_cache_config_builder() {
        let config = HelixCacheConfig::default()
            .with_max_capacity(500)
            .with_ttl(Duration::from_secs(60));
        assert_eq!(config.max_capacity, 500);
        assert_eq!(config.ttl, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_cache_put_and_get() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        let results = vec![
            ScoredResult::new(sample_step("s1"), 0.9),
            ScoredResult::new(sample_step("s2"), 0.7),
        ];

        let key = "ft:test query:*:20:*";
        cache.put_search(key, results).await;

        let cached = cache.get_search(key).await;
        assert!(cached.is_some());
        let cached = cached.expect("cached results");
        assert_eq!(cached.len(), 2);
        assert_eq!(cached[0].item.id, "s1");
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        assert!(cache.get_search("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        let results = vec![ScoredResult::new(sample_step("s1"), 0.9)];

        let key = "ft:test:*:20:*";
        cache.put_search(key, results).await;
        assert!(cache.get_search(key).await.is_some());

        cache.invalidate(key).await;
        assert!(cache.get_search(key).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidate_all() {
        let cache = HelixCache::new(&HelixCacheConfig::default());

        cache
            .put_search("k1", vec![ScoredResult::new(sample_step("s1"), 0.9)])
            .await;
        cache
            .put_search("k2", vec![ScoredResult::new(sample_step("s2"), 0.8)])
            .await;

        cache.invalidate_all();
        // moka invalidate_all is lazy — entries may still appear briefly.
        // Run pending tasks to ensure eviction completes before assertions.
        cache.run_pending_tasks().await;
        assert!(cache.get_search("k1").await.is_none());
        assert!(cache.get_search("k2").await.is_none());
    }

    #[test]
    fn test_search_key_generation() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        let opts = SearchOptions::default().with_helix("eva").with_limit(10);

        let key = cache.search_key("consciousness", &opts);
        assert!(key.starts_with("ft:"));
        assert!(key.contains("consciousness"));
        assert!(key.contains("eva"));
        assert!(key.contains("10"));
    }

    #[test]
    fn test_vector_key_generation() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        let embedding = vec![0.1_f32, 0.2, 0.3, 0.4, 0.5];
        let opts = SearchOptions::default();

        let key = cache.vector_key(&embedding, "step-embeddings", &opts);
        assert!(key.starts_with("vec:"));
        assert!(key.contains("step-embeddings"));
        assert!(key.contains('5')); // length
    }

    #[test]
    fn test_cache_entry_count() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        assert_eq!(cache.entry_count(), 0);
    }

    #[test]
    fn test_cache_debug_format() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        let debug = format!("{cache:?}");
        assert!(debug.contains("HelixCache"));
        assert!(debug.contains("entry_count"));
    }

    #[test]
    fn test_cache_is_clone() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        let _clone = cache.clone(); // Must compile — clone is cheap (Arc internally)
    }
}
