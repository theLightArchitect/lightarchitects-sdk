//! Semantic searcher — HNSW vector similarity via `nomic-embed-text` embeddings.
//!
//! Embeds the query text via the configured [`EmbeddingProvider`], then searches
//! the `step-embeddings` HNSW index (768-dim cosine) in Neo4j.

use std::sync::Arc;

use tracing::{instrument, warn};

use crate::db::{HelixDb, HelixDbError};
use crate::embedding::EmbeddingProvider;
use crate::search::{SearchOptions, index_names};

use super::{RetrievalSignal, ScoredId};

// ============================================================================
// SemanticSearcher
// ============================================================================

/// Semantic similarity search over 768-dim Step embeddings.
///
/// Flow: query text → embed via provider → HNSW ANN in Neo4j → scored IDs.
/// All computation stays within a single Cypher round-trip (no cross-system RPC).
pub struct SemanticSearcher {
    provider: Arc<dyn EmbeddingProvider>,
}

impl SemanticSearcher {
    /// Create a new semantic searcher with the given embedding provider.
    #[must_use]
    pub fn new(provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self { provider }
    }

    /// Search for steps semantically similar to the query text.
    ///
    /// Returns scored IDs sorted by cosine similarity (highest first).
    ///
    /// # Errors
    ///
    /// Returns error if embedding fails or the database query fails.
    #[instrument(skip(self, db), fields(query_len = query.len(), limit = opts.limit))]
    pub async fn search(
        &self,
        db: &dyn HelixDb,
        query: &str,
        opts: &SearchOptions,
    ) -> Result<Vec<ScoredId>, HelixDbError> {
        if query.is_empty() {
            return Ok(Vec::new());
        }

        // Embed the query
        let embeddings = self
            .provider
            .embed(&[query])
            .await
            .map_err(|e| HelixDbError::Validation(format!("embedding failed: {e}")))?;

        let Some(query_vec) = embeddings.into_iter().next() else {
            warn!("Embedding provider returned empty result for query");
            return Ok(Vec::new());
        };

        // Search HNSW index
        let results = db
            .vector_search(&query_vec, index_names::STEP_EMBEDDINGS, opts)
            .await?;

        Ok(results
            .into_iter()
            .map(|r| ScoredId {
                step_id: r.item.id,
                score: r.score,
                signal: RetrievalSignal::Semantic,
            })
            .collect())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_searcher_creation() {
        let provider = Arc::new(crate::embedding::MockEmbeddingProvider::nomic());
        let searcher = SemanticSearcher::new(provider);
        assert_eq!(searcher.provider.dimensions(), 768);
    }

    #[test]
    fn test_scored_id_signal_is_semantic() {
        let sid = ScoredId {
            step_id: "step-42".into(),
            score: 0.87,
            signal: RetrievalSignal::Semantic,
        };
        assert_eq!(sid.signal, RetrievalSignal::Semantic);
    }
}
