//! `HuggingFace` Text Embeddings Inference (TEI) provider.
//!
//! TEI is a high-throughput embedding server — GPU-accelerated, batch-optimized,
//! and significantly faster than Ollama for large batches.
//!
//! # Setup (one command)
//!
//! ```bash
//! # CPU (benchmark / dev):
//! docker run -p 8080:80 \
//!   ghcr.io/huggingface/text-embeddings-inference:cpu-1.2 \
//!   --model-id nomic-ai/nomic-embed-text-v1.5
//!
//! # GPU (production):
//! docker run --gpus all -p 8080:80 \
//!   ghcr.io/huggingface/text-embeddings-inference:1.2 \
//!   --model-id nomic-ai/nomic-embed-text-v1.5
//! ```
//!
//! Set `TEI_BASE_URL=http://localhost:8080` to point lightarchitects-helix at the container.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{instrument, warn};

use super::{EmbeddingError, EmbeddingProvider, EmbeddingResult};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the TEI embedding provider.
#[derive(Debug, Clone)]
pub struct TeiConfig {
    /// Base URL for the TEI server (default: `http://localhost:8080`).
    pub base_url: String,
    /// Maximum texts per request (TEI handles large batches efficiently).
    pub batch_size: usize,
    /// Request timeout.
    pub timeout: Duration,
    /// Retries on transient failure.
    pub max_retries: u32,
    /// Dimensionality of vectors produced by the loaded model.
    ///
    /// Must match the model started in the Docker container.
    /// `nomic-embed-text-v1.5` → 768. `all-MiniLM-L6-v2` → 384.
    pub dimensions: usize,
}

impl Default for TeiConfig {
    fn default() -> Self {
        Self {
            base_url: std::env::var("TEI_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_owned()),
            batch_size: 512,
            timeout: Duration::from_secs(30),
            max_retries: 3,
            dimensions: 768,
        }
    }
}

// ============================================================================
// TEI API types
// ============================================================================

/// Batch embed request — TEI native `/embed` endpoint.
#[derive(Serialize)]
struct TeiEmbedRequest {
    inputs: Vec<String>,
}

/// Batch embed response — returns `Vec<Vec<f32>>` directly.
#[derive(Deserialize)]
#[serde(untagged)]
enum TeiEmbedResponse {
    /// Direct float array (TEI native format).
    Embeddings(Vec<Vec<f32>>),
}

// ============================================================================
// TeiEmbeddingProvider
// ============================================================================

/// GPU-accelerated embedding provider via `HuggingFace` TEI.
///
/// Uses TEI's native `/embed` endpoint which returns `Vec<Vec<f32>>` directly —
/// no f64→f32 conversion needed unlike Ollama.
pub struct TeiEmbeddingProvider {
    config: TeiConfig,
    client: reqwest::Client,
}

impl TeiEmbeddingProvider {
    /// Create a new TEI embedding provider.
    ///
    /// # Errors
    /// Returns error if the HTTP client cannot be built.
    pub fn new(config: TeiConfig) -> EmbeddingResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| EmbeddingError::Provider(format!("HTTP client: {e}")))?;

        Ok(Self { config, client })
    }

    /// Create with default configuration (`http://localhost:8080`, 768-dim).
    ///
    /// # Errors
    /// Returns error if the HTTP client cannot be built.
    pub fn with_defaults() -> EmbeddingResult<Self> {
        Self::new(TeiConfig::default())
    }

    #[instrument(skip(self, texts), fields(count = texts.len()))]
    async fn embed_batch(&self, texts: &[String]) -> EmbeddingResult<Vec<Vec<f32>>> {
        let url = format!("{}/embed", self.config.base_url);
        let request = TeiEmbedRequest {
            inputs: texts.to_vec(),
        };

        let mut last_err = None;
        let mut delay = Duration::from_millis(100);

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                tokio::time::sleep(delay).await;
                delay = delay.saturating_mul(4);
            }

            match self.client.post(&url).json(&request).send().await {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        last_err = Some(EmbeddingError::Provider(format!("TEI {status}: {body}")));
                        continue;
                    }

                    let TeiEmbedResponse::Embeddings(embeddings) = resp
                        .json()
                        .await
                        .map_err(|e| EmbeddingError::Provider(format!("JSON parse: {e}")))?;

                    return Ok(embeddings);
                }
                Err(e) => {
                    warn!(attempt, error = %e, "TEI embed request failed");
                    last_err = Some(EmbeddingError::Provider(format!("request failed: {e}")));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| EmbeddingError::Provider("all retries exhausted".into())))
    }
}

#[async_trait]
impl EmbeddingProvider for TeiEmbeddingProvider {
    #[instrument(skip(self, texts), fields(provider = "tei", count = texts.len()))]
    async fn embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Truncate long texts.
        let owned: Vec<String> = texts
            .iter()
            .map(|&t| {
                if t.chars().count() > 2048 {
                    t.chars().take(2048).collect()
                } else {
                    t.to_owned()
                }
            })
            .collect();

        // Chunk into batches — TEI handles large batches well but has a server-side limit.
        let mut all_results = Vec::with_capacity(owned.len());
        for chunk in owned.chunks(self.config.batch_size) {
            let batch = self.embed_batch(chunk).await?;
            all_results.extend(batch);
        }

        Ok(all_results)
    }

    fn dimensions(&self) -> usize {
        self.config.dimensions
    }

    fn name(&self) -> &'static str {
        "tei"
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
    fn test_default_config() {
        let config = TeiConfig::default();
        assert_eq!(config.batch_size, 512);
        assert_eq!(config.dimensions, 768);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_provider_metadata() {
        let provider = TeiEmbeddingProvider::with_defaults().unwrap();
        assert_eq!(provider.dimensions(), 768);
        assert_eq!(provider.name(), "tei");
        assert_eq!(provider.max_batch_size(), 512);
    }
}
