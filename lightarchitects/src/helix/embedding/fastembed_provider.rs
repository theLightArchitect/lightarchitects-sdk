//! `FastEmbed` embedding provider — in-process ONNX inference, no HTTP server.
//!
//! Runs embedding models directly via ONNX Runtime. Zero HTTP overhead compared
//! to Ollama — 3-5× faster on CPU for the same `nomic-embed-text-v1.5` model.
//!
//! Model files are cached at `~/.cache/fastembed_cache/` on first use (~100 MB).
//!
//! # Feature gate
//! Requires the `fastembed` feature in lightarchitects-helix.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use tracing::instrument;

use super::{EmbeddingError, EmbeddingProvider, EmbeddingResult};

// ============================================================================
// Model selection
// ============================================================================

/// Which ONNX model to load.
///
/// Both options produce `f32` vectors. Choose based on the Neo4j HNSW index
/// dimension configured in lightarchitects-helix migrations:
/// - `NomicEmbedTextV15` → 768-dim  (matches `step-embeddings` index)
/// - `AllMiniLML6V2`    → 384-dim  (requires a separate 384-dim index)
#[derive(Debug, Clone, Copy, Default)]
pub enum FastEmbedModel {
    /// nomic-ai/nomic-embed-text-v1.5 — 768-dim, production-compatible.
    ///
    /// Drop-in replacement for Ollama `nomic-embed-text`.
    #[default]
    NomicEmbedTextV15,
    /// sentence-transformers/all-MiniLM-L6-v2 — 384-dim.
    ///
    /// Same model used by `MemPalace`. Useful for A/B comparison runs.
    /// Requires a 384-dim HNSW index — incompatible with the default
    /// `step-embeddings` index (768-dim).
    AllMiniLML6V2,
}

impl FastEmbedModel {
    pub(crate) fn to_fastembed(self) -> EmbeddingModel {
        match self {
            Self::NomicEmbedTextV15 => EmbeddingModel::NomicEmbedTextV15,
            Self::AllMiniLML6V2 => EmbeddingModel::AllMiniLML6V2,
        }
    }

    /// Vector dimensionality for the chosen model.
    #[must_use]
    pub fn dimensions(self) -> usize {
        match self {
            Self::NomicEmbedTextV15 => 768,
            Self::AllMiniLML6V2 => 384,
        }
    }
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the `FastEmbed` embedding provider.
#[derive(Debug, Clone)]
pub struct FastEmbedConfig {
    /// Model to load (default: `NomicEmbedTextV15`).
    pub model: FastEmbedModel,
    /// Inference batch size (default: 256).
    pub batch_size: usize,
    /// Print download progress to stderr on first model fetch.
    pub show_download_progress: bool,
    /// Truncate input texts longer than this many characters.
    pub max_chars: usize,
}

impl Default for FastEmbedConfig {
    fn default() -> Self {
        Self {
            model: FastEmbedModel::default(),
            batch_size: 256,
            show_download_progress: false,
            max_chars: 2048,
        }
    }
}

// ============================================================================
// FastEmbedProvider
// ============================================================================

/// In-process ONNX embedding provider.
///
/// Wraps [`fastembed::TextEmbedding`] behind the [`EmbeddingProvider`] trait.
/// `TextEmbedding::embed` requires `&mut self`, so the session is held behind
/// a [`Mutex`] inside an [`Arc`] — cheap to clone, safe to share across tasks.
pub struct FastEmbedProvider {
    model: Arc<Mutex<TextEmbedding>>,
    config: FastEmbedConfig,
}

impl FastEmbedProvider {
    /// Load the model and return a ready provider.
    ///
    /// Downloads ~100 MB on first call for `NomicEmbedTextV15`; subsequent
    /// calls load from the local cache.
    ///
    /// # Errors
    /// Returns [`EmbeddingError::Model`] if initialisation or download fails.
    pub fn new(config: FastEmbedConfig) -> EmbeddingResult<Self> {
        let opts = InitOptions::new(config.model.to_fastembed())
            .with_show_download_progress(config.show_download_progress);

        let model = TextEmbedding::try_new(opts)
            .map_err(|e| EmbeddingError::Model(format!("fastembed init: {e}")))?;

        Ok(Self {
            model: Arc::new(Mutex::new(model)),
            config,
        })
    }

    /// Load with default configuration (`NomicEmbedTextV15`, batch=256).
    ///
    /// # Errors
    /// Returns [`EmbeddingError::Model`] if initialisation fails.
    pub fn with_defaults() -> EmbeddingResult<Self> {
        Self::new(FastEmbedConfig::default())
    }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    #[instrument(skip(self, texts), fields(provider = "fastembed", count = texts.len()))]
    async fn embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Truncate to stay within model context window (8192 tokens ≈ 2048 chars).
        let owned: Vec<String> = texts
            .iter()
            .map(|&t| {
                if t.chars().count() > self.config.max_chars {
                    t.chars().take(self.config.max_chars).collect()
                } else {
                    t.to_owned()
                }
            })
            .collect();

        let model = Arc::clone(&self.model);
        let batch_size = Some(self.config.batch_size);

        // fastembed inference is synchronous (ONNX Runtime) and requires &mut self.
        // Offload to the blocking thread pool — keeps the async runtime free.
        // The Mutex lock is acquired and released entirely within the blocking thread.
        tokio::task::spawn_blocking(move || {
            let mut guard = model
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard
                .embed(owned, batch_size)
                .map_err(|e| EmbeddingError::Provider(format!("fastembed embed: {e}")))
        })
        .await
        .map_err(|e| EmbeddingError::Provider(format!("spawn_blocking join: {e}")))?
    }

    fn dimensions(&self) -> usize {
        self.config.model.dimensions()
    }

    fn name(&self) -> &'static str {
        "fastembed"
    }

    fn max_batch_size(&self) -> usize {
        self.config.batch_size
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = FastEmbedConfig::default();
        assert_eq!(config.batch_size, 256);
        assert_eq!(config.max_chars, 2048);
        assert!(!config.show_download_progress);
    }

    #[test]
    fn test_model_dimensions() {
        assert_eq!(FastEmbedModel::NomicEmbedTextV15.dimensions(), 768);
        assert_eq!(FastEmbedModel::AllMiniLML6V2.dimensions(), 384);
    }
}
