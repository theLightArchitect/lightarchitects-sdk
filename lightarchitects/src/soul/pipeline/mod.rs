//! Hybrid retrieval pipeline — BM25 + semantic RRF fusion.
//!
//! The pipeline combines multiple retrieval signals and fuses them via
//! **Reciprocal Rank Fusion** (RRF, k=60) to produce a ranked list of
//! [`RetrievalHit`] results.
//!
//! # Signals
//!
//! | Signal | Source | Notes |
//! |--------|--------|-------|
//! | [`RetrievalSignal::Bm25`] | SQLite FTS5 | Always available |
//! | [`RetrievalSignal::Semantic`] | Embedding HNSW | Requires `EmbeddingProvider` |
//! | [`RetrievalSignal::Graph`] | Neo4j traversal | Not yet wired (future) |
//! | [`RetrievalSignal::Structural`] | Node2Vec | Not yet wired (future) |
//!
//! # Usage
//!
//! ```rust,no_run
//! use crate::soul::pipeline::RetrievalPipeline;
//! use crate::soul::sqlite::SqliteBackend;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), crate::soul::pipeline::PipelineError> {
//! let backend = SqliteBackend::open_in_memory()?;
//! let pipeline = RetrievalPipeline::builder()
//!     .storage(Arc::new(backend))
//!     .build()?;
//!
//! let hits = pipeline.retrieve("consciousness and identity", 5).await?;
//! println!("found {} hits", hits.len());
//! # Ok(())
//! # }
//! ```

pub mod distiller;
pub mod error;
pub mod fts5;

pub use distiller::{DistilledContext, DistillerConfig, RetrievalDistiller, SortBy};
pub use error::{PipelineError, PipelineResult};
pub use fts5::fts5_or_expr;

use std::collections::HashMap;
use std::sync::Arc;

use crate::soul::embedding::EmbeddingProvider;
use crate::soul::storage::{StorageBackend, StorageEntry};

// ============================================================================
// RetrievalSignal
// ============================================================================

/// A retrieval signal source contributing to RRF fusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RetrievalSignal {
    /// BM25 keyword match via FTS5.
    Bm25,
    /// Semantic vector similarity via HNSW.
    Semantic,
    /// Graph traversal distance (Neo4j path patterns).
    Graph,
    /// Structural `Node2Vec` embedding similarity.
    Structural,
}

// ============================================================================
// RetrievalHit
// ============================================================================

/// A fused retrieval result returned after RRF fusion.
#[derive(Debug, Clone)]
pub struct RetrievalHit {
    /// The matched storage entry.
    pub entry: StorageEntry,
    /// Contributing signals with their per-signal RRF scores.
    pub signals: Vec<(RetrievalSignal, f32)>,
    /// Final RRF-fused score (sum of per-signal contributions).
    pub final_score: f32,
}

// ============================================================================
// RetrievalPipelineBuilder
// ============================================================================

/// Builder for [`RetrievalPipeline`].
#[derive(Default)]
pub struct RetrievalPipelineBuilder {
    storage: Option<Arc<dyn StorageBackend + Send + Sync>>,
    embedding: Option<Arc<dyn EmbeddingProvider>>,
    rrf_k: Option<usize>,
}

impl RetrievalPipelineBuilder {
    /// Set the storage backend (required).
    #[must_use]
    pub fn storage(mut self, s: Arc<dyn StorageBackend + Send + Sync>) -> Self {
        self.storage = Some(s);
        self
    }

    /// Set an optional embedding provider for semantic search.
    #[must_use]
    pub fn embedding(mut self, e: Arc<dyn EmbeddingProvider>) -> Self {
        self.embedding = Some(e);
        self
    }

    /// Set the RRF `k` constant (default: 60).
    #[must_use]
    pub fn rrf_k(mut self, k: usize) -> Self {
        self.rrf_k = Some(k);
        self
    }

    /// Build the [`RetrievalPipeline`].
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::InvalidQuery`] if no storage backend was set.
    pub fn build(self) -> PipelineResult<RetrievalPipeline> {
        let storage = self
            .storage
            .ok_or_else(|| PipelineError::InvalidQuery("storage backend is required".into()))?;
        Ok(RetrievalPipeline {
            storage,
            embedding: self.embedding,
            rrf_k: self.rrf_k.unwrap_or(60),
        })
    }
}

// ============================================================================
// RetrievalPipeline
// ============================================================================

/// Hybrid retrieval pipeline using Reciprocal Rank Fusion.
///
/// Combines BM25 keyword search and (optionally) semantic vector search
/// into a single ranked list via RRF.
pub struct RetrievalPipeline {
    storage: Arc<dyn StorageBackend + Send + Sync>,
    embedding: Option<Arc<dyn EmbeddingProvider>>,
    rrf_k: usize,
}

impl RetrievalPipeline {
    /// Create a new pipeline builder.
    #[must_use]
    pub fn builder() -> RetrievalPipelineBuilder {
        RetrievalPipelineBuilder::default()
    }

    /// Retrieve the top-K results for the given query.
    ///
    /// When an embedding provider is configured, performs hybrid BM25 + semantic
    /// fusion. Otherwise falls back to BM25-only.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError`] on storage or embedding failure.
    pub async fn retrieve(&self, query: &str, top_k: usize) -> PipelineResult<Vec<RetrievalHit>> {
        if query.is_empty() {
            return Err(PipelineError::InvalidQuery(
                "query must not be empty".into(),
            ));
        }

        // Fused results: entry_id → (entry, HashMap<signal, best_rrf_score>)
        let mut fused: HashMap<String, (StorageEntry, Vec<(RetrievalSignal, f32)>)> =
            HashMap::new();

        // ── Signal 1: BM25 ────────────────────────────────────────────────────
        let fts5_expr = fts5_or_expr(query);
        if !fts5_expr.is_empty() {
            let bm25_results = self
                .storage
                .search_bm25(&fts5_expr, Some(top_k.saturating_mul(2)))
                .await?;

            for (rank, entry) in bm25_results.into_iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let rrf_score = 1.0_f32 / (self.rrf_k as f32 + rank as f32 + 1.0);
                let id = entry.id.clone();
                fused
                    .entry(id)
                    .and_modify(|(_, signals)| {
                        signals.push((RetrievalSignal::Bm25, rrf_score));
                    })
                    .or_insert_with(|| (entry, vec![(RetrievalSignal::Bm25, rrf_score)]));
            }
        }

        // ── Signal 2: Semantic ────────────────────────────────────────────────
        if let Some(ref embedder) = self.embedding {
            match embedder.embed(&[query]).await {
                Ok(vecs) => {
                    if let Some(query_vec) = vecs.into_iter().next() {
                        let sem_results = self
                            .storage
                            .search_semantic(&query_vec, top_k.saturating_mul(2))
                            .await?;

                        for (rank, entry) in sem_results.into_iter().enumerate() {
                            #[allow(clippy::cast_precision_loss)]
                            let rrf_score = 1.0_f32 / (self.rrf_k as f32 + rank as f32 + 1.0);
                            let id = entry.id.clone();
                            fused
                                .entry(id)
                                .and_modify(|(_, signals)| {
                                    signals.push((RetrievalSignal::Semantic, rrf_score));
                                })
                                .or_insert_with(|| {
                                    (entry, vec![(RetrievalSignal::Semantic, rrf_score)])
                                });
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Semantic embedding failed, using BM25 only");
                }
            }
        }

        // ── RRF fusion: compute final score, sort, take top-K ─────────────────
        let mut hits: Vec<RetrievalHit> = fused
            .into_values()
            .map(|(entry, signals)| {
                let final_score: f32 = signals.iter().map(|(_, s)| s).sum();
                RetrievalHit {
                    entry,
                    signals,
                    final_score,
                }
            })
            .collect();

        hits.sort_by(|a, b| {
            b.final_score
                .partial_cmp(&a.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(top_k);

        Ok(hits)
    }
}
