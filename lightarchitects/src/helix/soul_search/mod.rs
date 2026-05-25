//! `soul-search` — hybrid retrieval engine with 4-signal RRF and adaptive mode selection.
//!
//! Combines four retrieval signals into a single ranked result:
//! - **BM25 keyword** (Lucene full-text index)
//! - **Semantic similarity** (768-dim HNSW via `nomic-embed-text`)
//! - **Structural similarity** (128-dim HNSW via `Node2Vec`)
//! - **Graph traversal** (Cypher path patterns, bounded `{1,7}`)
//!
//! Reciprocal Rank Fusion (RRF, k=60) fuses ranked lists with adaptive
//! signal weights determined by [`RetrievalMode`].
//!
//! Optional re-ranking pass available behind the `rerank` feature gate
//! (signal-diversity reranker that boosts multi-signal results).

pub mod cached;
pub mod context;
pub mod convergence;
pub mod fulltext;
pub mod graph;
pub mod hybrid;
pub mod personality;
pub mod reranker;
pub mod semantic;
pub mod structural;

// Re-exports
pub use cached::{CachedRetrievalResult, CachedRetriever};
pub use context::{ContextFormatter, FormattedContext};
pub use convergence::{ConvergenceParams, ConvergenceParticipant, ConvergenceResult};
pub use fulltext::FulltextSearcher;
pub use graph::{GraphFilter, GraphSearcher};
pub use hybrid::{
    HybridRetriever, HybridRetrieverConfig, RetrievalMode, RetrievalResult, precision_at_k,
    precision_at_k_patterns, recall_at_k,
};
pub use personality::{PersonalityEngine, PersonalityEngineConfig};
pub use reranker::{Reranker, RerankerConfig};
pub use semantic::SemanticSearcher;
pub use structural::StructuralSearcher;

use serde::{Deserialize, Serialize};

// ============================================================================
// Shared Types
// ============================================================================

/// A scored step ID — the common currency between retrieval signals.
///
/// Each searcher produces `Vec<ScoredId>`. The hybrid retriever fuses them
/// via RRF into a single ranked list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredId {
    /// Step ID.
    pub step_id: String,
    /// Signal-specific score (semantics vary by signal type).
    pub score: f64,
    /// Which retrieval signal produced this result.
    pub signal: RetrievalSignal,
}

/// Which retrieval signal produced a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RetrievalSignal {
    /// Lucene BM25 full-text search.
    Fulltext,
    /// Cosine similarity on 768-dim semantic embeddings.
    Semantic,
    /// Cosine similarity on 128-dim structural embeddings.
    Structural,
    /// Graph traversal distance score.
    Graph,
}

impl std::fmt::Display for RetrievalSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fulltext => write!(f, "fulltext"),
            Self::Semantic => write!(f, "semantic"),
            Self::Structural => write!(f, "structural"),
            Self::Graph => write!(f, "graph"),
        }
    }
}

/// RRF constant (k=60, standard value from the original RRF paper).
pub const RRF_K: f64 = 60.0;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scored_id() {
        let sid = ScoredId {
            step_id: "step-1".into(),
            score: 0.95,
            signal: RetrievalSignal::Semantic,
        };
        assert_eq!(sid.step_id, "step-1");
        assert!((sid.score - 0.95).abs() < f64::EPSILON);
        assert_eq!(sid.signal, RetrievalSignal::Semantic);
    }

    #[test]
    fn test_signal_display() {
        assert_eq!(RetrievalSignal::Fulltext.to_string(), "fulltext");
        assert_eq!(RetrievalSignal::Semantic.to_string(), "semantic");
        assert_eq!(RetrievalSignal::Structural.to_string(), "structural");
        assert_eq!(RetrievalSignal::Graph.to_string(), "graph");
    }

    #[test]
    fn test_rrf_k_constant() {
        assert!((RRF_K - 60.0).abs() < f64::EPSILON);
    }
}
