//! Structural searcher — HNSW vector similarity via `Node2Vec` embeddings.
//!
//! Searches the `step-struct-embeddings` HNSW index (128-dim cosine) in Neo4j.
//! Falls back gracefully if the index does not exist (GDS not run yet).

use std::sync::Arc;

use tracing::{instrument, warn};

use crate::db::{HelixDb, HelixDbError};
use crate::embedding::EmbeddingProvider;
use crate::search::{SearchOptions, index_names};

use super::{RetrievalSignal, ScoredId};

// ============================================================================
// StructuralSearcher
// ============================================================================

/// Structural similarity search over 128-dim `Node2Vec` Step embeddings.
///
/// `Node2Vec` captures graph-neighborhood similarity: two Steps that play
/// the same structural role in the helix get similar vectors even if their
/// content is completely different.
///
/// **Graceful fallback**: if `step-struct-embeddings` index does not exist
/// (GDS hasn't run yet), returns an empty list and logs a warning.
pub struct StructuralSearcher {
    provider: Arc<dyn EmbeddingProvider>,
}

impl StructuralSearcher {
    /// Create a new structural searcher with a structural embedding provider.
    ///
    /// The provider should produce 128-dim vectors (e.g., `MockEmbeddingProvider::structural()`).
    #[must_use]
    pub fn new(provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self { provider }
    }

    /// Search for steps structurally similar to the query text.
    ///
    /// Returns scored IDs sorted by cosine similarity (highest first).
    /// Returns empty vec if the structural embedding index does not exist.
    ///
    /// # Errors
    ///
    /// Returns error if embedding fails. Database errors from a missing index
    /// are caught and returned as empty results (graceful fallback).
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

        // Embed the query in structural space
        let embeddings =
            self.provider.embed(&[query]).await.map_err(|e| {
                HelixDbError::Validation(format!("structural embedding failed: {e}"))
            })?;

        let Some(query_vec) = embeddings.into_iter().next() else {
            warn!("Structural embedding provider returned empty result");
            return Ok(Vec::new());
        };

        // Search HNSW index — graceful fallback if index missing
        match db
            .vector_search(&query_vec, index_names::STEP_STRUCT_EMBEDDINGS, opts)
            .await
        {
            Ok(results) => Ok(results
                .into_iter()
                .map(|r| ScoredId {
                    step_id: r.item.id,
                    score: r.score,
                    signal: RetrievalSignal::Structural,
                })
                .collect()),
            Err(e) => {
                // Graceful fallback: index may not exist if GDS hasn't run
                warn!(error = %e, "Structural search unavailable — returning empty results");
                Ok(Vec::new())
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_structural_searcher_creation() {
        let provider = Arc::new(crate::embedding::MockEmbeddingProvider::structural());
        let searcher = StructuralSearcher::new(provider);
        assert_eq!(searcher.provider.dimensions(), 128);
    }

    #[test]
    fn test_scored_id_signal_is_structural() {
        let sid = ScoredId {
            step_id: "step-99".into(),
            score: 0.72,
            signal: RetrievalSignal::Structural,
        };
        assert_eq!(sid.signal, RetrievalSignal::Structural);
    }
}
