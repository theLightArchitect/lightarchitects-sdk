//! In-process ONNX embedding via [FastEmbed](https://github.com/Anush008/fastembed-rs).
//!
//! [`FastEmbedProvider`] runs inference locally with no server, no port, and
//! no configuration. The model is downloaded once to `~/.cache/fastembed_cache/`
//! on first use and cached permanently.
//!
//! # Models
//!
//! | [`FastEmbedModel`] | Dimensions | Size | Notes |
//! |---|---|---|---|
//! | `Default` (all-MiniLM-L6-v2) | 384 | ~90 MB | Zero-config, recommended |
//! | `NomicEmbedText` (nomic-embed-text-v1.5) | 768 | ~274 MB | Higher quality |
//!
//! # Usage
//!
//! ```rust,no_run
//! # #[cfg(feature = "embedding-fastembed")]
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use crate::soul::embedding::fastembed::{FastEmbedModel, FastEmbedProvider};
//!
//! let provider = FastEmbedProvider::try_new(FastEmbedModel::Default)?;
//! # Ok(())
//! # }
//! ```

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use tokio::task;

use crate::soul::embedding::{EmbeddingError, EmbeddingProvider, EmbeddingResult};

// ============================================================================
// FastEmbedModel
// ============================================================================

/// Embedding model selection for [`FastEmbedProvider`].
///
/// Covers the most useful tradeoffs; for other models use
/// [`FastEmbedProvider::with_fastembed_model`] directly.
#[derive(Debug, Clone, Copy, Default)]
pub enum FastEmbedModel {
    /// all-MiniLM-L6-v2 — 384-dim, ~90 MB, fast. Recommended default.
    #[default]
    Default,
    /// nomic-embed-text-v1.5 — 768-dim, ~274 MB, higher quality.
    NomicEmbedText,
}

impl FastEmbedModel {
    fn to_fastembed(self) -> (EmbeddingModel, usize) {
        match self {
            Self::Default => (EmbeddingModel::AllMiniLML6V2, 384),
            Self::NomicEmbedText => (EmbeddingModel::NomicEmbedTextV15, 768),
        }
    }
}

// ============================================================================
// FastEmbedProvider
// ============================================================================

/// In-process ONNX embedding provider — zero server, zero configuration.
///
/// Downloads the model to `~/.cache/fastembed_cache/` on first use.
/// Subsequent calls use the cached model with no network access.
///
/// `TextEmbedding::embed` requires `&mut self` — a `Mutex` serialises concurrent
/// calls. For high-throughput use-cases consider a dedicated embedding server.
pub struct FastEmbedProvider {
    /// Mutex because `TextEmbedding::embed` takes `&mut self`.
    model: Arc<Mutex<TextEmbedding>>,
    dims: usize,
    model_name: &'static str,
}

impl FastEmbedProvider {
    /// Create a provider for the given model.
    ///
    /// Downloads the model on first use if not already cached.
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddingError::Model`] if the model download or ONNX
    /// initialisation fails.
    pub fn try_new(model: FastEmbedModel) -> Result<Self, EmbeddingError> {
        let (fastembed_model, dims) = model.to_fastembed();
        let model_name = match model {
            FastEmbedModel::Default => "fastembed/all-minilm-l6-v2",
            FastEmbedModel::NomicEmbedText => "fastembed/nomic-embed-text-v1.5",
        };
        let te = TextEmbedding::try_new(
            InitOptions::new(fastembed_model).with_show_download_progress(false),
        )
        .map_err(|e| EmbeddingError::Model(format!("FastEmbed init failed: {e}")))?;

        Ok(Self {
            model: Arc::new(Mutex::new(te)),
            dims,
            model_name,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    async fn embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        let texts_owned: Vec<String> = texts.iter().map(|s| (*s).to_owned()).collect();
        let model = Arc::clone(&self.model);

        task::spawn_blocking(move || {
            let refs: Vec<&str> = texts_owned.iter().map(String::as_str).collect();
            let mut guard = model
                .lock()
                .map_err(|_| EmbeddingError::Provider("FastEmbed model mutex poisoned".into()))?;
            guard
                .embed(refs, None)
                .map_err(|e| EmbeddingError::Provider(format!("FastEmbed embed failed: {e}")))
        })
        .await
        .map_err(|e| EmbeddingError::Provider(format!("spawn_blocking join error: {e}")))?
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn name(&self) -> &'static str {
        self.model_name
    }

    fn max_batch_size(&self) -> usize {
        256
    }
}
