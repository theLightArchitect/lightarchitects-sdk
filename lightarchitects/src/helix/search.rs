//! Full-text and vector search over helix Step nodes.
//!
//! Wraps Neo4j's Lucene full-text indexes and HNSW vector indexes
//! through `graph-engine`'s parameterized query interface.
//!
//! # Index Names (must match `migrations.rs` definitions)
//!
//! | Index | Type | Properties | Use |
//! |-------|------|------------|-----|
//! | `step-fulltext` | Lucene | `content`, `title` | BM25 keyword search |
//! | `step-embeddings` | HNSW | `embedding` (768-dim) | Semantic similarity |
//! | `step-struct-embeddings` | HNSW | `struct_embedding` (128-dim) | Structural similarity |
//!
//! # Score Semantics
//!
//! - **Fulltext**: Lucene BM25 score (0.0+, unbounded, higher = more relevant)
//! - **Vector**: Cosine similarity (0.0–1.0, higher = more similar)

use serde::{Deserialize, Serialize};

// ============================================================================
// Scored Result
// ============================================================================

/// A search result with a relevance score.
///
/// Generic over the item type — used for both fulltext and vector results.
/// Score semantics differ by search type (see module docs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredResult<T> {
    /// The matched item.
    pub item: T,
    /// Relevance score (higher = more relevant).
    pub score: f64,
}

impl<T> ScoredResult<T> {
    /// Create a new scored result.
    #[must_use]
    pub fn new(item: T, score: f64) -> Self {
        Self { item, score }
    }

    /// Map the inner item to a different type.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> ScoredResult<U> {
        ScoredResult {
            item: f(self.item),
            score: self.score,
        }
    }
}

// ============================================================================
// Search Options
// ============================================================================

/// Configuration for search queries.
///
/// All filters are optional — `None` means no filter applied.
/// Builder pattern via chainable `with_*` methods.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Maximum number of results to return (default: 20).
    pub limit: u32,
    /// Minimum score threshold — results below this are excluded.
    pub min_score: Option<f64>,
    /// Filter results to a specific helix.
    pub helix_id: Option<String>,
    /// Filter results to a specific owner (sibling name).
    pub owner: Option<String>,
    /// Minimum significance threshold on Step.significance.
    pub min_significance: Option<f64>,
    /// Strand affinity hint — boost steps tagged to this strand after RRF fusion.
    ///
    /// Domain-agnostic: callers supply any strand name their domain defines
    /// (e.g. `"preference"` for preference questions, `"diagnosis"` in healthcare,
    /// `"purchase"` in e-commerce). `None` = no strand boost applied.
    ///
    /// The boost is applied as a post-RRF reranker: it lifts matching atoms
    /// without redistributing weight away from the core retrieval signals.
    pub strand_affinity: Option<String>,
    /// Exclude turn-level Steps from results (only return session-level Steps).
    ///
    /// When `true`, adds `AND node.turn_role IS NULL` to Cypher queries, filtering
    /// out per-turn Steps that have `turn_role = "user"` or `"assistant"`.
    /// Session-level Steps have no `turn_role` property. Default: `false`.
    pub session_only: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 20,
            min_score: None,
            helix_id: None,
            owner: None,
            min_significance: None,
            strand_affinity: None,
            session_only: false,
        }
    }
}

impl SearchOptions {
    /// Set the maximum number of results.
    #[must_use]
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// Filter to a specific helix.
    #[must_use]
    pub fn with_helix(mut self, helix_id: impl Into<String>) -> Self {
        self.helix_id = Some(helix_id.into());
        self
    }

    /// Set minimum score threshold.
    #[must_use]
    pub fn with_min_score(mut self, min_score: f64) -> Self {
        self.min_score = Some(min_score);
        self
    }

    /// Filter to a specific owner (sibling name).
    #[must_use]
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// Set minimum significance threshold.
    #[must_use]
    pub fn with_min_significance(mut self, min_sig: f64) -> Self {
        self.min_significance = Some(min_sig);
        self
    }

    /// Set strand affinity boost — lifts steps tagged to this strand.
    ///
    /// Caller determines the appropriate strand name from query intent.
    #[must_use]
    pub fn with_strand_affinity(mut self, strand_name: impl Into<String>) -> Self {
        self.strand_affinity = Some(strand_name.into());
        self
    }

    /// Exclude turn-level Steps — only return session-level Steps.
    #[must_use]
    pub fn with_session_only(mut self) -> Self {
        self.session_only = true;
        self
    }
}

// ============================================================================
// Index Name Constants
// ============================================================================

/// Neo4j index names — must match `migrations.rs` definitions.
pub mod index_names {
    /// Lucene full-text index on `Step.content` and `Step.title`.
    pub const STEP_FULLTEXT: &str = "step-fulltext";

    /// HNSW vector index on `Step.embedding` (768-dim, nomic-embed-text).
    pub const STEP_EMBEDDINGS: &str = "step-embeddings";

    /// HNSW vector index on `Step.struct_embedding` (128-dim, `Node2Vec`).
    pub const STEP_STRUCT_EMBEDDINGS: &str = "step-struct-embeddings";

    /// HNSW vector index on `Step.sage_embedding` (128-dim, `GraphSAGE` inductive).
    ///
    /// Written by `gds.beta.graphSage.write(writeProperty:'sage_embedding')`.
    /// Falls back to `STEP_STRUCT_EMBEDDINGS` during migration.
    pub const STEP_SAGE_EMBEDDINGS: &str = "step-sage-embeddings";
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_scored_result_new() {
        let result = ScoredResult::new("hello", 0.95);
        assert_eq!(result.item, "hello");
        assert!((result.score - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn test_scored_result_map() {
        let result = ScoredResult::new(42_i32, 0.8);
        let mapped = result.map(|x| x.to_string());
        assert_eq!(mapped.item, "42");
        assert!((mapped.score - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_scored_result_serde_roundtrip() {
        let result = ScoredResult::new("test item".to_owned(), 3.15);
        let json = serde_json::to_string(&result).expect("serialize");
        let back: ScoredResult<String> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.item, "test item");
        assert!((back.score - 3.15).abs() < f64::EPSILON);
    }

    #[test]
    fn test_search_options_default() {
        let opts = SearchOptions::default();
        assert_eq!(opts.limit, 20);
        assert!(opts.min_score.is_none());
        assert!(opts.helix_id.is_none());
        assert!(opts.owner.is_none());
        assert!(opts.min_significance.is_none());
    }

    #[test]
    fn test_search_options_builder() {
        let opts = SearchOptions::default()
            .with_limit(50)
            .with_helix("eva")
            .with_min_score(0.5)
            .with_owner("eva")
            .with_min_significance(7.0);

        assert_eq!(opts.limit, 50);
        assert_eq!(opts.helix_id.as_deref(), Some("eva"));
        assert_eq!(opts.min_score, Some(0.5));
        assert_eq!(opts.owner.as_deref(), Some("eva"));
        assert_eq!(opts.min_significance, Some(7.0));
    }

    #[test]
    fn test_index_names_match_migrations() {
        // These must match the names in migrations.rs HELIX_SCHEMA_STATEMENTS.
        assert_eq!(index_names::STEP_FULLTEXT, "step-fulltext");
        assert_eq!(index_names::STEP_EMBEDDINGS, "step-embeddings");
        assert_eq!(
            index_names::STEP_STRUCT_EMBEDDINGS,
            "step-struct-embeddings"
        );
    }
}
