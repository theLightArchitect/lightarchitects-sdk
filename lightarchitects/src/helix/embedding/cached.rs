//! Moka-backed TTL cache wrapping any [`EmbeddingProvider`].
//!
//! # Cache hit path (~0µs for cached texts)
//!
//! `SHA-256(text \x00 provider_name)` → `Cache::get()` → cached `Vec<f32>`
//!
//! # Cache miss path
//!
//! Pass-through to inner provider; populate cache on return.
//!
//! # When this helps
//!
//! Repeated queries with identical text hit the cache — e.g., the same helix
//! title appearing across multiple search sessions, or repeated embedding of
//! shared terminology. Semantic caching literature reports 40–70% hit rates
//! for knowledge-base retrieval workloads. With `nomic-embed-text` at ~50ms
//! per call, a 50% hit rate saves ~25ms per query at zero accuracy cost.
//!
//! # Cache coherence
//!
//! Text embeddings are deterministic for a fixed model version: the same text
//! always produces the same vector. No eviction is needed for correctness —
//! only the TTL ensures memory is bounded. On model upgrade, replace or
//! re-create the [`CachedEmbeddingProvider`]; all entries will TTL out.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use moka::future::Cache;
use sha2::{Digest, Sha256};

use super::{EmbeddingProvider, EmbeddingResult};
use crate::soul::EmbeddingError;

/// Maximum number of cached embedding vectors.
const CACHE_MAX_ENTRIES: u64 = 4_096;

/// Default TTL for cached embeddings. Text→vector mappings are deterministic
/// per model version so longer TTLs are safe; 5 minutes keeps memory bounded.
const CACHE_TTL_SECS: u64 = 300;

/// Wraps an [`EmbeddingProvider`] with a per-text moka TTL cache.
///
/// Thread-safe: `Clone`able, backed by `moka::future::Cache` which is `Send + Sync`.
pub struct CachedEmbeddingProvider {
    inner: Arc<dyn EmbeddingProvider>,
    cache: Cache<[u8; 32], Arc<Vec<f32>>>,
}

impl CachedEmbeddingProvider {
    /// Wrap `inner` with a default 5-minute, 4096-entry cache.
    pub fn new(inner: Arc<dyn EmbeddingProvider>) -> Self {
        Self::with_config(
            inner,
            CACHE_MAX_ENTRIES,
            Duration::from_secs(CACHE_TTL_SECS),
        )
    }

    /// Wrap `inner` with custom capacity and TTL.
    pub fn with_config(inner: Arc<dyn EmbeddingProvider>, max_entries: u64, ttl: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_entries)
            .time_to_live(ttl)
            .build();
        Self { inner, cache }
    }

    /// SHA-256(text \x00 `provider_name`) — stable key per text+model combination.
    fn cache_key(text: &str, provider_name: &str) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        hasher.update(b"\x00");
        hasher.update(provider_name.as_bytes());
        hasher.finalize().into()
    }
}

#[async_trait]
impl EmbeddingProvider for CachedEmbeddingProvider {
    async fn embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        let provider_name = self.inner.name();
        let mut results: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
        let mut missed_indices: Vec<usize> = Vec::new();
        let mut missed_texts: Vec<&str> = Vec::new();

        // Phase 1: populate from cache.
        for (i, &text) in texts.iter().enumerate() {
            let key = Self::cache_key(text, provider_name);
            if let Some(cached) = self.cache.get(&key).await {
                results[i] = Some((*cached).clone());
            } else {
                missed_indices.push(i);
                missed_texts.push(text);
            }
        }

        if missed_texts.is_empty() {
            // Full cache hit — every slot is Some.
            return Ok(results.into_iter().map(Option::unwrap_or_default).collect());
        }

        // Phase 2: fetch misses from inner provider and populate cache.
        // `missed_indices[offset]` = original index in `texts`;
        // `missed_texts[offset]`   = the corresponding text slice.
        let embeddings = self.inner.embed(&missed_texts).await?;
        for (offset, (&orig_i, embedding)) in missed_indices.iter().zip(embeddings).enumerate() {
            let key = Self::cache_key(missed_texts[offset], provider_name);
            self.cache.insert(key, Arc::new(embedding.clone())).await;
            results[orig_i] = Some(embedding);
        }

        results
            .into_iter()
            .enumerate()
            .map(|(i, v)| {
                v.ok_or_else(|| {
                    EmbeddingError::Provider(format!("missing embedding result at index {i}"))
                })
            })
            .collect()
    }

    fn dimensions(&self) -> usize {
        self.inner.dimensions()
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn max_batch_size(&self) -> usize {
        self.inner.max_batch_size()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::helix::embedding::MockEmbeddingProvider;

    #[tokio::test]
    async fn cache_hit_returns_same_vector() {
        let mock = Arc::new(MockEmbeddingProvider::new(4));
        let provider = CachedEmbeddingProvider::new(mock);

        let r1 = provider.embed(&["hello world"]).await.unwrap();
        let r2 = provider.embed(&["hello world"]).await.unwrap();
        assert_eq!(r1, r2);
    }

    #[tokio::test]
    async fn different_texts_get_different_keys() {
        let k1 = CachedEmbeddingProvider::cache_key("foo", "test");
        let k2 = CachedEmbeddingProvider::cache_key("bar", "test");
        assert_ne!(k1, k2);
    }

    #[tokio::test]
    async fn batch_partial_miss_works() {
        let mock = Arc::new(MockEmbeddingProvider::new(4));
        let provider = CachedEmbeddingProvider::new(mock);

        // Warm "a".
        provider.embed(&["a"]).await.unwrap();
        // "a" is cached, "b" is a miss.
        let results = provider.embed(&["a", "b"]).await.unwrap();
        assert_eq!(results.len(), 2);
    }
}
