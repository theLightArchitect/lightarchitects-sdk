//! Unified helix client — combines database and cache into one handle.
//!
//! [`HelixClient`] is the recommended entry point for applications.
//! It wraps [`HelixNeo4j`] + [`HelixCache`], runs migrations on open,
//! and integrates cache lookups with database queries.
//!
//! # Usage
//!
//! ```rust,no_run
//! use crate::helix::client::HelixClient;
//! use crate::helix::db::{HelixConfig, HelixDb};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = HelixConfig::from_env()?;
//! let client = HelixClient::open(&config).await?;
//!
//! // Access HelixDb methods via db()
//! let steps = client.db().get_steps("eva", None, None).await?;
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use tracing::instrument;

use crate::helix::cache::HelixCache;
use crate::helix::db::{HelixConfig, HelixDb, HelixDbError, HelixNeo4j};
use crate::helix::search::{ScoredResult, SearchOptions};
use crate::helix::types::Step;

// ============================================================================
// HelixClient
// ============================================================================

/// Unified helix client — database + cache.
///
/// Clone is cheap (all internals are `Arc`).
/// Implements [`HelixDb`] by delegating to the inner `HelixNeo4j`.
#[derive(Clone)]
pub struct HelixClient {
    db: Arc<HelixNeo4j>,
    cache: HelixCache,
}

impl std::fmt::Debug for HelixClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelixClient")
            .field("cache", &self.cache)
            .finish_non_exhaustive()
    }
}

impl HelixClient {
    /// Open a helix client: connect to Neo4j, run migrations, init cache.
    ///
    /// # Errors
    ///
    /// Returns [`HelixDbError`] if connection or migration fails.
    #[instrument(skip(config))]
    pub async fn open(config: &HelixConfig) -> Result<Self, HelixDbError> {
        let db = HelixNeo4j::connect(&config.neo4j).await?;
        let migrated = db.migrate().await?;
        if migrated > 0 {
            tracing::info!(count = migrated, "Applied helix schema migrations");
        }

        let cache = HelixCache::new(&config.cache_config());

        Ok(Self {
            db: Arc::new(db),
            cache,
        })
    }

    /// Access the underlying database directly.
    #[must_use]
    pub fn db(&self) -> &HelixNeo4j {
        &self.db
    }

    /// Access the cache directly.
    #[must_use]
    pub fn cache(&self) -> &HelixCache {
        &self.cache
    }

    /// Cached fulltext search — checks cache before querying the database.
    ///
    /// # Errors
    ///
    /// Returns [`HelixDbError`] if the database query fails.
    pub async fn cached_fulltext_search(
        &self,
        query: &str,
        opts: &SearchOptions,
    ) -> Result<Vec<ScoredResult<Step>>, HelixDbError> {
        let key = self.cache.search_key(query, opts);

        if let Some(results) = self.cache.get_search(&key).await {
            return Ok(results.as_ref().clone());
        }

        let results = self.db.fulltext_search(query, opts).await?;
        self.cache.put_search(&key, results.clone()).await;
        Ok(results)
    }

    /// Cached vector search — checks cache before querying the database.
    ///
    /// # Errors
    ///
    /// Returns [`HelixDbError`] if the database query fails.
    pub async fn cached_vector_search(
        &self,
        embedding: &[f32],
        index_name: &str,
        opts: &SearchOptions,
    ) -> Result<Vec<ScoredResult<Step>>, HelixDbError> {
        let key = self.cache.vector_key(embedding, index_name, opts);

        if let Some(results) = self.cache.get_search(&key).await {
            return Ok(results.as_ref().clone());
        }

        let results = self.db.vector_search(embedding, index_name, opts).await?;
        self.cache.put_search(&key, results.clone()).await;
        Ok(results)
    }

    /// Invalidate all cached search results.
    ///
    /// Call after bulk writes (ingestion, migration) to prevent stale reads.
    pub fn invalidate_cache(&self) {
        self.cache.invalidate_all();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_is_clone() {
        // HelixClient should be Clone (Arc internals)
        fn assert_clone<T: Clone>() {}
        assert_clone::<HelixClient>();
    }

    #[test]
    fn test_client_debug() {
        // Cannot construct without Neo4j, but verify Debug is implemented
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<HelixClient>();
    }
}
