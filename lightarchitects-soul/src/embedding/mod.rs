//! Embedding provider abstraction for semantic vector generation.
//!
//! Defines the [`EmbeddingProvider`] trait, [`EmbeddingError`], and
//! [`PrivacyLevel`] — the portable interface that concrete implementations
//! (Ollama, cloud, mock) satisfy.
//!
//! The *trait* lives in the SDK so any crate can depend on the interface
//! without pulling in the server-side implementations. Concrete providers
//! (Ollama, OpenAI-compatible, TEI, FastEmbed) live in `soul-helix` as
//! server infrastructure.
//!
//! # Usage
//!
//! ```rust,no_run
//! use lightarchitects_soul::embedding::{EmbeddingProvider, EmbeddingResult};
//! use std::sync::Arc;
//!
//! # async fn example(provider: Arc<dyn EmbeddingProvider>) -> EmbeddingResult<()> {
//! let vectors = provider.embed(&["hello world", "consciousness and identity"]).await?;
//! assert_eq!(vectors.len(), 2);
//! # Ok(())
//! # }
//! ```

// NOTE: module declarations here — lib.rs (EVA-owned) exposes `pub mod embedding`.
pub mod mock;

#[cfg(feature = "embedding-ollama")]
pub mod ollama;

/// In-process ONNX embedding via FastEmbed (no server required).
#[cfg(feature = "embedding-fastembed")]
pub mod fastembed;

/// `llama.cpp` HTTP embedding provider (requires a running llama.cpp server).
#[cfg(feature = "embedding-llama-cpp")]
pub mod llama_cpp;

use async_trait::async_trait;
use thiserror::Error;

// ============================================================================
// EmbeddingError
// ============================================================================

/// Error type for all [`EmbeddingProvider`] operations.
#[derive(Debug, Error)]
pub enum EmbeddingError {
    /// HTTP or network error from the provider endpoint.
    #[error("provider error: {0}")]
    Provider(String),

    /// Model not available or misconfigured.
    #[error("model error: {0}")]
    Model(String),

    /// Privacy gate blocked cloud embedding for this content.
    #[error("privacy gate blocked cloud embedding: {0}")]
    PrivacyBlocked(String),

    /// Invalid input (empty text, oversized batch, etc.).
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Database write error while persisting embeddings.
    #[error("db error: {0}")]
    Database(String),
}

/// Result type for embedding operations.
pub type EmbeddingResult<T> = Result<T, EmbeddingError>;

// ============================================================================
// PrivacyLevel
// ============================================================================

/// Privacy classification for an entry to be embedded.
///
/// Used by the embedding pipeline to decide whether cloud providers
/// are allowed for a given piece of content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyLevel {
    /// No restrictions — any provider (local or cloud) is allowed.
    Standard,
    /// Local-only — cloud providers must be skipped for this entry.
    LocalOnly,
}

impl PrivacyLevel {
    /// Classify a step's privacy level from its JSON metadata.
    ///
    /// Returns [`LocalOnly`][Self::LocalOnly] when:
    /// - `"privacy": "local"` is set, or
    /// - `"redacted": true` is set.
    ///
    /// Returns [`Standard`][Self::Standard] otherwise.
    #[must_use]
    pub fn from_metadata(metadata: &serde_json::Value) -> Self {
        let is_local = metadata
            .get("privacy")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|v| v == "local");

        let is_redacted = metadata
            .get("redacted")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        if is_local || is_redacted {
            Self::LocalOnly
        } else {
            Self::Standard
        }
    }
}

// ============================================================================
// EmbeddingProvider trait
// ============================================================================

/// Trait for generating semantic embedding vectors from text.
///
/// Implementations include Ollama (local-first), OpenAI-compatible cloud,
/// TEI (Text Embeddings Inference), `FastEmbed` (in-process ONNX), and a
/// mock provider for tests. All live in `soul-helix` as server infrastructure.
///
/// SDK consumers implement this trait to plug in custom embedding backends
/// when using [`crate::SqliteBackend`] for offline semantic search.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embeddings for a batch of texts.
    ///
    /// Returns one vector per input text. Vector dimensionality
    /// depends on the provider (768 for nomic-embed-text, 1536 for
    /// text-embedding-3-small, 128 for structural/Node2Vec).
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddingError`] on network failure, model error, or
    /// privacy gate rejection.
    async fn embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>>;

    /// Number of dimensions produced by this provider.
    fn dimensions(&self) -> usize;

    /// Provider name (for logging and metrics).
    fn name(&self) -> &'static str;

    /// Maximum batch size supported (texts per [`embed`][Self::embed] call).
    fn max_batch_size(&self) -> usize;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_error_display() {
        assert_eq!(
            EmbeddingError::Provider("timeout".into()).to_string(),
            "provider error: timeout"
        );
        assert_eq!(
            EmbeddingError::PrivacyBlocked("local-only".into()).to_string(),
            "privacy gate blocked cloud embedding: local-only"
        );
        assert_eq!(
            EmbeddingError::InvalidInput("empty".into()).to_string(),
            "invalid input: empty"
        );
    }

    #[test]
    fn test_privacy_level_equality() {
        assert_eq!(PrivacyLevel::Standard, PrivacyLevel::Standard);
        assert_ne!(PrivacyLevel::Standard, PrivacyLevel::LocalOnly);
    }
}
