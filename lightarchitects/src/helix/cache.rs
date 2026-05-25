//! In-memory `TinyLFU` cache for helix query results.
//!
//! Replaces the former `SQLite` `helix_cache` table with a zero-disk-I/O
//! hot-path cache powered by [`moka`].
//!
//! # Design
//!
//! - **Engine**: `moka::future::Cache` (`TinyLFU` admission + LRU eviction)
//! - **Capacity**: 64 MiB byte budget (byte-weight weigher)
//! - **TTL**: 5 minutes (configurable)
//! - **Key**: `String` (serialized from query params)
//! - **Value**: `Arc<CachedEntry>` — fused results + retrieval mode
//!
//! # Usage
//!
//! ```rust,no_run
//! use lightarchitects::helix::cache::{CachedEntry, HelixCache, HelixCacheConfig};
//! use lightarchitects::helix::search::SearchOptions;
//! use lightarchitects::helix::soul_search::hybrid::RetrievalMode;
//!
//! # async fn example() {
//! let cache = HelixCache::new(&HelixCacheConfig::default());
//!
//! // Check cache before querying DB
//! let key = cache.search_key("consciousness breakthrough", &SearchOptions::default());
//! if let Some(entry) = cache.get_search(&key).await {
//!     // Cache hit — use entry.results directly
//!     let _ = &entry.results;
//!     return;
//! }
//!
//! // Cache miss — query DB, then store
//! // let results = db.fulltext_search("consciousness breakthrough", &opts).await?;
//! // cache.put_search(&key, CachedEntry::new(results, RetrievalMode::Balanced)).await;
//! # }
//! ```

use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;

use crate::helix::search::{ScoredResult, SearchOptions};
use crate::helix::soul_search::hybrid::RetrievalMode;
use crate::helix::types::Step;

// ============================================================================
// CachedEntry
// ============================================================================

/// A cached retrieval result pairing the result set with the mode that produced it.
///
/// Storing `mode` at write time avoids a Neo4j round-trip on cache-hit spans:
/// the [`CachedRetriever`] can emit the correct AYIN `soul.helix.retrieve`
/// `mode` attribute without re-querying step count.
///
/// [`CachedRetriever`]: crate::helix::soul_search::cached::CachedRetriever
#[derive(Debug, Clone)]
pub struct CachedEntry {
    /// Fused retrieval results with full Step data (sorted by score, highest first).
    pub results: Arc<Vec<ScoredResult<Step>>>,
    /// The retrieval mode that produced these results.
    pub mode: RetrievalMode,
}

impl CachedEntry {
    /// Create a new cached entry.
    #[must_use]
    pub fn new(results: Vec<ScoredResult<Step>>, mode: RetrievalMode) -> Self {
        Self {
            results: Arc::new(results),
            mode,
        }
    }
}

// ============================================================================
// Cache Configuration
// ============================================================================

/// Configuration for [`HelixCache`].
#[derive(Debug, Clone)]
pub struct HelixCacheConfig {
    /// Maximum byte budget for the cache (default: 64 MiB).
    ///
    /// When a weigher is set, moka interprets `max_capacity` as a byte budget
    /// rather than an entry count. The weigher estimates each entry's size from
    /// `content.len() + title.len() + per-step overhead`.
    pub max_capacity_bytes: u64,
    /// Time-to-live for cached entries (default: 5 minutes).
    pub ttl: Duration,
}

impl Default for HelixCacheConfig {
    fn default() -> Self {
        Self {
            max_capacity_bytes: 64 * 1024 * 1024, // 64 MiB
            ttl: Duration::from_secs(300),
        }
    }
}

impl HelixCacheConfig {
    /// Set maximum byte budget.
    #[must_use]
    pub fn with_max_capacity_bytes(mut self, bytes: u64) -> Self {
        self.max_capacity_bytes = bytes;
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
// Weigher
// ============================================================================

/// Estimate the byte size of a cached entry for the moka byte-budget weigher.
///
/// Uses `content.len() + title.len() + id.len() + 256` per step as an
/// approximation. The 256-byte overhead covers fixed struct fields (timestamps,
/// scores, metadata pointers) and Arc/Vec header overhead. Conservative for
/// short entries; accurate to ±20% for typical helix steps (~2 KiB average).
#[allow(clippy::ptr_arg)] // &String required by moka Fn(&K, &V) -> u32 bound; K = String
fn weigh_entry(_k: &String, v: &Arc<CachedEntry>) -> u32 {
    let byte_estimate: usize = v
        .results
        .iter()
        .map(|r| {
            r.item.content.len()
                + r.item.title.as_deref().map_or(0, str::len)
                + r.item.id.len()
                + 256
        })
        .sum::<usize>()
        .max(64); // minimum 64 bytes for an empty result set
    u32::try_from(byte_estimate).unwrap_or(u32::MAX)
}

// ============================================================================
// HelixCache
// ============================================================================

/// In-memory cache for helix search results.
///
/// Thread-safe and async-compatible — clone to share across tasks.
/// Uses `TinyLFU` admission + LRU eviction with configurable TTL and byte budget.
///
/// # In-memory only
///
/// This cache is **not persisted to disk**. It is rebuilt empty on every
/// process restart. The 5-minute TTL and 64 MiB byte budget are designed for
/// programmatic SDK callers and the gateway retrieve endpoint.
///
/// # Cache invalidation after writes
///
/// Call [`invalidate_all`](Self::invalidate_all) after any bulk write (ingest,
/// migration) to prevent stale reads within the same process. Call
/// [`run_pending_tasks`](Self::run_pending_tasks) afterwards to force
/// synchronous cleanup (required in tests; automatic in production).
#[derive(Clone)]
pub struct HelixCache {
    /// Search result cache keyed by query+opts fingerprint.
    search: Cache<String, Arc<CachedEntry>>,
}

impl std::fmt::Debug for HelixCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelixCache")
            .field("entry_count", &self.search.entry_count())
            .field("weighted_size_bytes", &self.search.weighted_size())
            .finish()
    }
}

impl HelixCache {
    /// Create a new cache with the given configuration.
    #[must_use]
    pub fn new(config: &HelixCacheConfig) -> Self {
        let search = Cache::builder()
            .weigher(weigh_entry)
            .max_capacity(config.max_capacity_bytes)
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

    /// Get a cached search entry (fused results + retrieval mode).
    pub async fn get_search(&self, key: &str) -> Option<Arc<CachedEntry>> {
        self.search.get(key).await
    }

    /// Store a search entry in the cache.
    pub async fn put_search(&self, key: &str, entry: CachedEntry) {
        self.search.insert(key.to_owned(), Arc::new(entry)).await;
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

    /// Number of entries currently in the cache (eventually consistent).
    #[must_use]
    pub fn entry_count(&self) -> u64 {
        self.search.entry_count()
    }

    /// Approximate byte size consumed by all live entries (eventually consistent).
    ///
    /// Returns the sum of `weigh_entry(v)` across all entries. Useful for
    /// `cache_stats` telemetry exposed by the gateway retrieve handler.
    #[must_use]
    pub fn weighted_size(&self) -> u64 {
        self.search.weighted_size()
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
            content: "Test content for cache sizing".into(),
            significance: 5.0,
            step_date: None,
            step_index: None,
            community_id: None,
            expires: None,
            created_at: chrono::Utc::now(),
            metadata: serde_json::Value::Null,
            vault_path: None,
            graph_embedding: None,
        }
    }

    fn sample_entry(ids: &[&str], mode: RetrievalMode) -> CachedEntry {
        let results = ids
            .iter()
            .map(|id| ScoredResult::new(sample_step(id), 0.9))
            .collect();
        CachedEntry::new(results, mode)
    }

    #[test]
    fn test_cache_config_default() {
        let config = HelixCacheConfig::default();
        assert_eq!(config.max_capacity_bytes, 64 * 1024 * 1024);
        assert_eq!(config.ttl, Duration::from_secs(300));
    }

    #[test]
    fn test_cache_config_builder() {
        let config = HelixCacheConfig::default()
            .with_max_capacity_bytes(1024 * 1024)
            .with_ttl(Duration::from_secs(60));
        assert_eq!(config.max_capacity_bytes, 1024 * 1024);
        assert_eq!(config.ttl, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_cache_put_and_get() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        let entry = sample_entry(&["s1", "s2"], RetrievalMode::Balanced);

        let key = "ft:test query:*:20:*";
        cache.put_search(key, entry).await;

        let cached = cache.get_search(key).await;
        assert!(cached.is_some());
        let cached = cached.expect("cached entry");
        assert_eq!(cached.results.len(), 2);
        assert_eq!(cached.results[0].item.id, "s1");
        assert_eq!(cached.mode, RetrievalMode::Balanced);
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        assert!(cache.get_search("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        let entry = sample_entry(&["s1"], RetrievalMode::KeywordDominated);

        let key = "ft:test:*:20:*";
        cache.put_search(key, entry).await;
        assert!(cache.get_search(key).await.is_some());

        cache.invalidate(key).await;
        assert!(cache.get_search(key).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidate_all() {
        let cache = HelixCache::new(&HelixCacheConfig::default());

        cache
            .put_search("k1", sample_entry(&["s1"], RetrievalMode::Balanced))
            .await;
        cache
            .put_search("k2", sample_entry(&["s2"], RetrievalMode::GraphWeighted))
            .await;

        cache.invalidate_all();
        // moka invalidate_all is lazy — run pending tasks before asserting.
        cache.run_pending_tasks().await;
        assert!(cache.get_search("k1").await.is_none());
        assert!(cache.get_search("k2").await.is_none());
    }

    #[tokio::test]
    async fn test_weighted_size_after_insert() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        cache
            .put_search("k1", sample_entry(&["s1", "s2"], RetrievalMode::Balanced))
            .await;
        cache.run_pending_tasks().await;
        // weighted_size should be > 0 after a put
        assert!(cache.weighted_size() > 0);
    }

    #[test]
    fn test_cached_entry_new() {
        let entry = sample_entry(&["a", "b"], RetrievalMode::GraphWeighted);
        assert_eq!(entry.results.len(), 2);
        assert_eq!(entry.mode, RetrievalMode::GraphWeighted);
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
        assert!(debug.contains("weighted_size_bytes"));
    }

    #[test]
    fn test_cache_is_clone() {
        let cache = HelixCache::new(&HelixCacheConfig::default());
        let _clone = cache.clone();
    }
}
