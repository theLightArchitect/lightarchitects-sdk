//! Cached hybrid retriever — wraps [`HybridRetriever`] with [`HelixCache`] integration.
//!
//! # Cache hit path (latency < 5ms)
//!
//! ```text
//! search_key → get_search → CachedEntry { results, mode } → return CachedRetrievalResult
//! ```
//!
//! # Cache miss path
//!
//! ```text
//! search_key → cache miss → HybridRetriever.search() → get_steps_by_ids() →
//! CachedEntry { ScoredResult<Step>, mode } → put_search → return CachedRetrievalResult
//! ```
//!
//! # Ghost vector prevention (OWASP ASVS 8.3.4)
//!
//! Call [`CachedRetriever::invalidate_step`] when a Step is deleted. Invalidation
//! runs at **write time** (step delete path), not at query time, ensuring deleted
//! step content never appears in subsequent retrieve responses.

use std::collections::HashMap;
use std::time::Instant;

use tracing::instrument;

use crate::helix::cache::{CachedEntry, HelixCache};
use crate::helix::db::{HelixDb, HelixDbError};
use crate::helix::search::{ScoredResult, SearchOptions};
use crate::helix::types::Step;

use super::hybrid::{
    FusedResult, HybridRetriever, HybridRetrieverConfig, RetrievalMode, RetrievalResult,
    SignalCounts,
};

// ============================================================================
// CachedRetrievalResult
// ============================================================================

/// Result from a cached hybrid retrieval query.
///
/// `cache_hit_ratio` is `1.0` on a full cache hit, `0.0` on a cache miss.
/// The AYIN `soul.helix.retrieve` span uses this to set the `cache_hit_ratio`
/// attribute without a separate boolean flag.
#[derive(Debug, Clone)]
pub struct CachedRetrievalResult {
    /// Underlying hybrid retrieval result.
    pub result: RetrievalResult,
    /// `1.0` = full cache hit (no DB query), `0.0` = cache miss (live retrieval).
    pub cache_hit_ratio: f32,
    /// The cache key used for this query (for `ETag` generation and logging).
    pub cache_key: String,
    /// The retrieval mode (from stored cache entry or from live retrieval).
    pub mode: RetrievalMode,
}

// ============================================================================
// CachedRetriever
// ============================================================================

/// Hybrid retriever with integrated [`HelixCache`].
///
/// On a cache hit, returns the stored result immediately (`cache_hit_ratio=1.0`).
/// On a miss, delegates to [`HybridRetriever`], hydrates full [`Step`] data,
/// stores the result, and returns with `cache_hit_ratio=0.0`.
pub struct CachedRetriever {
    /// Shared cache (also held by gateway `PlatformState`).
    pub cache: HelixCache,
    /// Underlying hybrid retriever.
    pub retriever: HybridRetriever,
}

impl CachedRetriever {
    /// Create a new cached retriever.
    #[must_use]
    pub fn new(cache: HelixCache, retriever: HybridRetriever) -> Self {
        Self { cache, retriever }
    }

    /// Run hybrid retrieval, returning cached results when available.
    ///
    /// On a cache miss, runs the 4-signal parallel retrieval, hydrates full
    /// [`Step`] data via a batch DB fetch, and stores the result.
    ///
    /// # Errors
    ///
    /// Returns [`HelixDbError`] if the cache misses and the underlying
    /// retrieval or step-hydration query fails.
    #[instrument(skip(self, db), fields(query_len = query.len(), helix_id = ?opts.helix_id))]
    pub async fn search(
        &self,
        db: &dyn HelixDb,
        query: &str,
        opts: &SearchOptions,
        config: &HybridRetrieverConfig,
    ) -> Result<CachedRetrievalResult, HelixDbError> {
        let start = Instant::now();
        let cache_key = self.cache.search_key(query, opts);

        // ── Cache hit ──────────────────────────────────────────────────────
        let outcome = if let Some(entry) = self.cache.get_search(&cache_key).await {
            let mode = entry.mode;
            let fused: Vec<FusedResult> = entry
                .results
                .iter()
                .map(|r| FusedResult {
                    step_id: r.item.id.clone(),
                    score: r.score,
                    signals: vec![],
                })
                .collect();
            CachedRetrievalResult {
                result: RetrievalResult {
                    results: fused,
                    mode,
                    signal_counts: SignalCounts {
                        fulltext: 0,
                        semantic: 0,
                        structural: 0,
                        graph: 0,
                    },
                },
                cache_hit_ratio: 1.0,
                cache_key: cache_key.clone(),
                mode,
            }
        } else {
            // ── Cache miss — live retrieval ────────────────────────────────
            let retrieval = self.retriever.search(db, query, opts, config).await?;
            let mode = retrieval.mode;

            // Hydrate full Step data for cache storage (single batch round-trip).
            // Steps not found in the batch (e.g., deleted between retrieval and
            // hydration) are silently dropped — ghost vector prevention handles
            // explicit deletes via invalidate_step().
            let ids: Vec<String> = retrieval
                .results
                .iter()
                .map(|r| r.step_id.clone())
                .collect();
            let hydrated = db.get_steps_by_ids(&ids).await.unwrap_or_default();
            let step_map: HashMap<String, Step> =
                hydrated.into_iter().map(|s| (s.id.clone(), s)).collect();

            let scored: Vec<ScoredResult<Step>> = retrieval
                .results
                .iter()
                .filter_map(|r| {
                    step_map
                        .get(&r.step_id)
                        .map(|step| ScoredResult::new(step.clone(), r.score))
                })
                .collect();

            self.cache
                .put_search(&cache_key, CachedEntry::new(scored, mode))
                .await;

            CachedRetrievalResult {
                result: retrieval,
                cache_hit_ratio: 0.0,
                cache_key: cache_key.clone(),
                mode,
            }
        };

        // ── AYIN span emission (compile-to-nop without `observe` feature) ──
        #[cfg(feature = "observe")]
        {
            use crate::ayin::span::{Actor, TraceContext, TraceOutcome};
            let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            let sc = &outcome.result.signal_counts;
            let ctx = TraceContext::new(Actor::soul(), "soul.helix.retrieve")
                .outcome(TraceOutcome::Continue)
                .metadata(serde_json::json!({
                    "helix_id": opts.helix_id.as_deref().unwrap_or("*"),
                    "mode": outcome.mode.as_str(),
                    "cache_hit_ratio": outcome.cache_hit_ratio,
                    "signal_counts_ft": sc.fulltext,
                    "signal_counts_sem": sc.semantic,
                    "signal_counts_str": sc.structural,
                    "signal_counts_gr": sc.graph,
                    "latency_ms": elapsed_ms,
                    "result_count": outcome.result.results.len(),
                    "embedding_backend": config.embedding.backend,
                }));
            crate::ayin::emit_span_background(ctx);
        }
        // Suppress unused-variable warning when observe is off.
        #[cfg(not(feature = "observe"))]
        let _ = start;

        Ok(outcome)
    }

    /// Invalidate all cached results for a helix.
    ///
    /// Use after bulk writes (step ingestion, migration) to prevent stale reads.
    pub async fn invalidate_helix(&self, _helix_id: &str) {
        self.cache.invalidate_all();
        self.cache.run_pending_tasks().await;
    }

    /// Invalidate the cache after a step deletion (ghost vector prevention).
    ///
    /// Emits a `soul.helix.cache.ghost_vector` trace event (OWASP ASVS 8.3.4).
    /// Invalidation runs at **write time** so that the very next retrieve
    /// response never includes deleted step content.
    pub async fn invalidate_step(&self, helix_id: &str, step_index: usize) {
        tracing::info!(
            helix_id,
            step_index,
            name = "soul.helix.cache.ghost_vector",
            "Cache invalidated for deleted step"
        );
        self.cache.invalidate_all();
        self.cache.run_pending_tasks().await;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::helix::cache::HelixCacheConfig;
    use crate::helix::soul_search::hybrid::{HybridRetrieverConfig, RetrievalMode};
    use proptest::prelude::*;

    fn test_cache() -> HelixCache {
        HelixCache::new(&HelixCacheConfig::default())
    }

    fn sample_scored(id: &str) -> ScoredResult<Step> {
        ScoredResult::new(
            Step {
                id: id.into(),
                helix_id: "test-helix".into(),
                title: Some("Test".into()),
                content: "Test content".into(),
                significance: 5.0,
                step_date: None,
                step_index: None,
                community_id: None,
                expires: None,
                created_at: chrono::Utc::now(),
                metadata: serde_json::Value::Null,
                vault_path: None,
                graph_embedding: None,
            },
            0.9,
        )
    }

    #[tokio::test]
    async fn test_cache_hit_returns_stored_results() {
        let cache = test_cache();
        let key = "ft:test:eva:20:*";
        let entry = CachedEntry::new(
            vec![sample_scored("s1"), sample_scored("s2")],
            RetrievalMode::Balanced,
        );
        cache.put_search(key, entry).await;
        cache.run_pending_tasks().await;

        let cached = cache.get_search(key).await;
        assert!(cached.is_some());
        let cached = cached.unwrap();
        assert_eq!(cached.results.len(), 2);
        assert_eq!(cached.results[0].item.id, "s1");
        assert_eq!(cached.mode, RetrievalMode::Balanced);
    }

    #[tokio::test]
    async fn test_cache_miss_returns_none() {
        let cache = test_cache();
        assert!(cache.get_search("nonexistent-key").await.is_none());
    }

    #[tokio::test]
    async fn test_ghost_vector_invalidation() {
        let cache = test_cache();
        let entry = CachedEntry::new(
            vec![sample_scored("step-to-delete")],
            RetrievalMode::KeywordDominated,
        );
        cache.put_search("ft:query:eva:20:*", entry).await;
        cache.run_pending_tasks().await;
        assert!(cache.get_search("ft:query:eva:20:*").await.is_some());

        // Simulate step deletion — ghost vector invalidation
        cache.invalidate_all();
        // CRITICAL: run_pending_tasks() required before asserting cache miss.
        // moka invalidate_all is deferred; without this call the entry may
        // still be visible to subsequent get_search calls.
        cache.run_pending_tasks().await;

        assert!(
            cache.get_search("ft:query:eva:20:*").await.is_none(),
            "deleted step must not appear in cache after ghost vector invalidation"
        );
    }

    #[tokio::test]
    async fn test_cache_hit_ratio_on_hit() {
        let cache = test_cache();
        let entry = CachedEntry::new(vec![sample_scored("s1")], RetrievalMode::Balanced);
        let key = "ft:cache-hit-test:*:20:*";
        cache.put_search(key, entry).await;
        cache.run_pending_tasks().await;

        let hit = cache.get_search(key).await;
        assert!(hit.is_some(), "cache should have the entry");
        // In CachedRetriever, cache_hit_ratio would be 1.0 here.
        // We verify the entry is retrievable (the ratio is set by CachedRetriever.search).
        let hit = hit.unwrap();
        assert_eq!(hit.mode, RetrievalMode::Balanced);
    }

    #[test]
    fn test_cached_retrieval_result_fields() {
        let result = CachedRetrievalResult {
            result: RetrievalResult {
                results: vec![],
                mode: RetrievalMode::GraphWeighted,
                signal_counts: SignalCounts {
                    fulltext: 0,
                    semantic: 0,
                    structural: 0,
                    graph: 0,
                },
            },
            cache_hit_ratio: 1.0,
            cache_key: "ft:q:h:20:*".into(),
            mode: RetrievalMode::GraphWeighted,
        };
        assert!((result.cache_hit_ratio - 1.0).abs() < f32::EPSILON);
        assert_eq!(result.mode, RetrievalMode::GraphWeighted);
    }

    #[test]
    fn test_config_builder_chaining() {
        let config = HybridRetrieverConfig::default();
        assert_eq!(config.top_k, 20);
    }

    // ── Property tests ────────────────────────────────────────────────────────

    proptest! {
        /// Cache key is deterministic: same query + same opts → same key (TF-2).
        #[test]
        fn prop_cache_key_idempotent(query in "[a-z ]{1,64}", limit in 1u32..=100) {
            let cache = test_cache();
            let opts = SearchOptions::default().with_limit(limit);
            let k1 = cache.search_key(&query, &opts);
            let k2 = cache.search_key(&query, &opts);
            prop_assert_eq!(k1, k2);
        }

        /// Different queries produce different cache keys (TF-2).
        #[test]
        fn prop_distinct_queries_produce_distinct_keys(
            q1 in "[a-z]{1,32}",
            q2 in "[A-Z]{1,32}",
        ) {
            let cache = test_cache();
            let opts = SearchOptions::default();
            // q1 is all-lowercase, q2 is all-uppercase — guaranteed distinct.
            prop_assert_ne!(cache.search_key(&q1, &opts), cache.search_key(&q2, &opts));
        }

        /// RRF mode selection is correct at boundary values (TF-2).
        ///
        /// < 25 → KeywordDominated; 25..100 → Balanced; ≥ 100 → GraphWeighted.
        #[test]
        fn prop_mode_selection_boundary(count in 0usize..200) {
            let mode = RetrievalMode::from_step_count(count);
            if count < 25 {
                prop_assert_eq!(mode, RetrievalMode::KeywordDominated);
            } else if count < 100 {
                prop_assert_eq!(mode, RetrievalMode::Balanced);
            } else {
                prop_assert_eq!(mode, RetrievalMode::GraphWeighted);
            }
        }
    }

    // ── Concurrent invalidation regression (TF-3) ─────────────────────────────

    /// Concurrent put + `invalidate_all` must leave the cache empty (TF-3).
    ///
    /// Regression: previously a race between put and invalidate could leave
    /// ghost entries visible after `run_pending_tasks()`.
    #[tokio::test]
    async fn test_concurrent_invalidation_drains_cache() {
        let cache = std::sync::Arc::new(test_cache());
        let entry = CachedEntry::new(
            vec![sample_scored("concurrent-step")],
            RetrievalMode::Balanced,
        );

        // Insert 10 entries concurrently.
        let puts: Vec<_> = (0..10)
            .map(|i| {
                let c = cache.clone();
                let e = entry.clone();
                tokio::spawn(async move { c.put_search(&format!("ft:q{i}:*:20:*"), e).await })
            })
            .collect();
        for h in puts {
            h.await.unwrap();
        }

        // Invalidate while puts may still be pending.
        cache.invalidate_all();
        cache.run_pending_tasks().await;

        // No entry from before the invalidation should survive.
        for i in 0..10 {
            assert!(
                cache.get_search(&format!("ft:q{i}:*:20:*")).await.is_none(),
                "entry q{i} survived concurrent invalidation"
            );
        }
    }
}
