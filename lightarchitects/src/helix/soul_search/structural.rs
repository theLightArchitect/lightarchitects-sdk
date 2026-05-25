//! Structural searcher — HNSW vector similarity via `GraphSAGE` inductive embeddings.
//!
//! Searches the `step-sage-embeddings` HNSW index (128-dim cosine) in Neo4j.
//! Falls back to `step-struct-embeddings` (`Node2Vec`) if the `GraphSAGE` index does
//! not yet exist (GDS consolidation hasn't run). Falls back to empty if neither
//! index exists.

use std::sync::Arc;

use tracing::{instrument, warn};

use crate::helix::db::{HelixDb, HelixDbError};
use crate::helix::embedding::EmbeddingProvider;
use crate::helix::search::{SearchOptions, index_names};

use super::{RetrievalSignal, ScoredId};

// ============================================================================
// StructuralSearcher
// ============================================================================

/// Structural similarity search over 128-dim `GraphSAGE` Step embeddings.
///
/// `GraphSAGE` is an **inductive** GNN: it produces embeddings for new Steps
/// immediately (no nightly GDS batch required), unlike `Node2Vec` which needs
/// a full graph re-training pass.
///
/// Two Steps that play the same structural role in the helix get similar
/// vectors even if their content differs — this is the P5 structural signal.
///
/// **Index fallback order**:
/// 1. `step-sage-embeddings` (`GraphSAGE`, written by GDS consolidation)
/// 2. `step-struct-embeddings` (`Node2Vec`, legacy — used during migration)
/// 3. Empty list (GDS not yet run, no structural signal available)
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

        // Try GraphSAGE index first; fall back to Node2Vec; then empty.
        let sage_result = db
            .vector_search(&query_vec, index_names::STEP_SAGE_EMBEDDINGS, opts)
            .await;

        let results = match sage_result {
            Ok(r) if !r.is_empty() => r,
            Ok(_) | Err(_) => {
                // SAGE index absent or empty — try legacy Node2Vec index
                match db
                    .vector_search(&query_vec, index_names::STEP_STRUCT_EMBEDDINGS, opts)
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(error = %e, "Structural search unavailable — returning empty results");
                        return Ok(Vec::new());
                    }
                }
            }
        };

        Ok(results
            .into_iter()
            .map(|r| ScoredId {
                step_id: r.item.id,
                score: r.score,
                signal: RetrievalSignal::Structural,
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
    fn test_structural_searcher_creation() {
        let provider = Arc::new(crate::helix::embedding::MockEmbeddingProvider::structural());
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
