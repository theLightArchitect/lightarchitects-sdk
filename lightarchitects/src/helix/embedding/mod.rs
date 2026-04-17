//! Embedding pipeline — semantic and structural vector generation.
//!
//! Two embedding dimensions:
//! - **Semantic** (768-dim): What content *means* — via Ollama `nomic-embed-text`
//! - **Structural** (128-dim): Where content *sits* in the graph — via GDS `Node2Vec`
//!
//! Both write directly to Neo4j Step node properties, triggering HNSW index updates.
//!
//! # Architecture
//!
//! The [`EmbeddingProvider`] trait, [`EmbeddingError`], and [`PrivacyLevel`] are
//! defined in `lightarchitects-soul` (the SDK) and re-exported here. All
//! concrete implementations (Ollama, Cloud, TEI, `FastEmbed`, Mock) live in
//! this module as server infrastructure.

pub mod chunker;
pub mod cloud;
#[cfg(feature = "fastembed")]
pub mod fastembed_provider;
pub mod mock;
pub mod ollama;
pub mod pipeline;
pub mod tei;

// ── Re-export trait + error types from SDK ────────────────────────────────────
// All callers that use `crate::helix::EmbeddingProvider` etc. continue
// to work unchanged — the re-export preserves type identity.
pub use crate::soul::{EmbeddingError, EmbeddingProvider, EmbeddingResult, PrivacyLevel};

// ── Re-export concrete implementations ───────────────────────────────────────
pub use chunker::{Chunk, Chunker, ChunkerConfig};
pub use cloud::CloudEmbeddingProvider;
#[cfg(feature = "fastembed")]
pub use fastembed_provider::{FastEmbedConfig, FastEmbedModel, FastEmbedProvider};
pub use mock::MockEmbeddingProvider;
pub use ollama::OllamaEmbeddingProvider;
pub use pipeline::{
    EmbeddingPipelineConfig, SemanticEmbeddingPipeline, StructuralEmbeddingPipeline,
};
pub use tei::{TeiConfig, TeiEmbeddingProvider};

// ============================================================================
// Tests — PrivacyLevel::from_metadata (server-side classification helper)
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_privacy_level_standard() {
        let meta = serde_json::json!({});
        assert_eq!(PrivacyLevel::from_metadata(&meta), PrivacyLevel::Standard);
    }

    #[test]
    fn test_privacy_level_local() {
        let meta = serde_json::json!({"privacy": "local"});
        assert_eq!(PrivacyLevel::from_metadata(&meta), PrivacyLevel::LocalOnly);
    }

    #[test]
    fn test_privacy_level_redacted() {
        let meta = serde_json::json!({"redacted": true});
        assert_eq!(PrivacyLevel::from_metadata(&meta), PrivacyLevel::LocalOnly);
    }

    #[test]
    fn test_privacy_level_both() {
        let meta = serde_json::json!({"privacy": "local", "redacted": true});
        assert_eq!(PrivacyLevel::from_metadata(&meta), PrivacyLevel::LocalOnly);
    }

    #[test]
    fn test_privacy_level_other_privacy_value() {
        let meta = serde_json::json!({"privacy": "public"});
        assert_eq!(PrivacyLevel::from_metadata(&meta), PrivacyLevel::Standard);
    }
}
