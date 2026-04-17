//! Ollama embedding provider — local-first semantic embeddings.
//!
//! Uses `nomic-embed-text` (768 dimensions) via Ollama's `/api/embed` batch
//! endpoint. All data stays on the local machine — no cloud API calls.
//!
//! Texts exceeding 2048 characters are truncated before sending; a
//! `tracing::warn!` is emitted for each truncated text.
//!
//! # Feature Gate
//!
//! This module is only compiled when the `embedding-ollama` feature is enabled:
//!
//! ```toml
//! lightarchitects-soul = { version = "0.1", features = ["embedding-ollama"] }
//! ```

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{EmbeddingError, EmbeddingProvider, EmbeddingResult};

/// Maximum character count per text before truncation.
const MAX_CHARS: usize = 2048;

/// Initial retry backoff in milliseconds.
const INITIAL_BACKOFF_MS: u64 = 100;

/// Backoff multiplier applied on each retry.
const BACKOFF_MULTIPLIER: u32 = 4;

// ============================================================================
// OllamaConfig
// ============================================================================

/// Configuration for [`OllamaEmbeddingProvider`].
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    /// Base URL for the Ollama API (default: `http://localhost:11434`).
    ///
    /// Set to `https://api.ollama.com` for Ollama Cloud.
    pub base_url: String,
    /// Model name (default: `nomic-embed-text`).
    pub model: String,
    /// Maximum number of texts per request (default: 32).
    pub batch_size: usize,
    /// Per-request timeout (default: 10 seconds).
    pub timeout_secs: u64,
    /// Maximum number of retry attempts (default: 3).
    pub max_retries: u32,
    /// Optional API key sent as `Authorization: Bearer <key>`.
    ///
    /// Required for Ollama Cloud (`https://api.ollama.com`).
    /// Reads from `OLLAMA_API_KEY` environment variable when `None` is not
    /// explicitly set — i.e. the default picks it up automatically.
    pub api_key: Option<String>,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_owned()),
            model: "nomic-embed-text".to_owned(),
            batch_size: 32,
            timeout_secs: 10,
            max_retries: 3,
            api_key: std::env::var("OLLAMA_API_KEY").ok(),
        }
    }
}

// ============================================================================
// Wire types (serde only)
// ============================================================================

/// Batch embed request body sent to `/api/embed`.
#[derive(Serialize)]
struct EmbedBatchRequest {
    model: String,
    input: Vec<String>,
}

/// Batch embed response from `/api/embed`.
#[derive(Deserialize)]
struct EmbedBatchResponse {
    /// Fields populated by serde — accessed by consumers.
    #[allow(dead_code)]
    embeddings: Vec<Vec<f64>>,
}

// ============================================================================
// OllamaEmbeddingProvider
// ============================================================================

/// Local embedding provider via Ollama.
///
/// Generates 768-dimensional semantic embeddings using `nomic-embed-text`.
/// All computation happens locally — no data leaves the machine.
///
/// Texts longer than 2048 characters are silently truncated with a warning.
pub struct OllamaEmbeddingProvider {
    config: OllamaConfig,
    client: reqwest::Client,
}

impl OllamaEmbeddingProvider {
    /// Create a new provider with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddingError::Provider`] if the HTTP client cannot be built.
    pub fn new(config: OllamaConfig) -> EmbeddingResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| EmbeddingError::Provider(format!("HTTP client build failed: {e}")))?;
        Ok(Self { config, client })
    }

    /// Create with default configuration, reading `OLLAMA_BASE_URL` from the
    /// environment if set.
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddingError::Provider`] if the HTTP client cannot be built.
    pub fn from_env() -> EmbeddingResult<Self> {
        Self::new(OllamaConfig::default())
    }

    /// Send one batch of texts to `/api/embed` with exponential backoff retries.
    async fn embed_batch(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        let url = format!("{}/api/embed", self.config.base_url);
        let request = EmbedBatchRequest {
            model: self.config.model.clone(),
            input: texts.iter().map(|t| (*t).to_owned()).collect(),
        };

        let mut last_err: Option<EmbeddingError> = None;
        let mut delay = Duration::from_millis(INITIAL_BACKOFF_MS);

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                tokio::time::sleep(delay).await;
                delay = delay.saturating_mul(BACKOFF_MULTIPLIER);
            }

            let mut req = self.client.post(&url).json(&request);
            if let Some(key) = &self.config.api_key {
                req = req.bearer_auth(key);
            }
            match req.send().await {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        tracing::warn!(attempt, %status, "Ollama embed non-success response");
                        last_err = Some(EmbeddingError::Provider(format!(
                            "Ollama returned {status}: {body}"
                        )));
                        continue;
                    }

                    let parsed: EmbedBatchResponse = resp
                        .json()
                        .await
                        .map_err(|e| EmbeddingError::Provider(format!("JSON parse error: {e}")))?;

                    // Convert f64 → f32.
                    let embeddings: Vec<Vec<f32>> = parsed
                        .embeddings
                        .iter()
                        .map(|emb| {
                            emb.iter()
                                .map(|&v| {
                                    #[allow(clippy::cast_possible_truncation)]
                                    let val = v as f32;
                                    val
                                })
                                .collect()
                        })
                        .collect();

                    return Ok(embeddings);
                }
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "Ollama embed request failed");
                    last_err = Some(EmbeddingError::Provider(format!("request failed: {e}")));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| EmbeddingError::Provider("all retries exhausted".into())))
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbeddingProvider {
    async fn embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        for text in texts {
            if text.is_empty() {
                return Err(EmbeddingError::InvalidInput("empty text in batch".into()));
            }
        }

        // Truncate each text to MAX_CHARS, warning when truncation occurs.
        let truncated: Vec<String> = texts
            .iter()
            .map(|t| {
                let char_count = t.chars().count();
                if char_count > MAX_CHARS {
                    tracing::warn!(
                        original_chars = char_count,
                        max_chars = MAX_CHARS,
                        "Text truncated to {MAX_CHARS} chars before embedding"
                    );
                    t.chars().take(MAX_CHARS).collect()
                } else {
                    (*t).to_owned()
                }
            })
            .collect();
        let truncated_refs: Vec<&str> = truncated.iter().map(String::as_str).collect();

        // Chunk into batches of `batch_size`.
        let mut all_results: Vec<Vec<f32>> = Vec::with_capacity(truncated_refs.len());
        for chunk in truncated_refs.chunks(self.config.batch_size) {
            let batch = self.embed_batch(chunk).await?;
            all_results.extend(batch);
        }

        Ok(all_results)
    }

    fn dimensions(&self) -> usize {
        768
    }

    fn name(&self) -> &'static str {
        "ollama-nomic-embed-text"
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
    fn test_default_config_values() {
        let config = OllamaConfig::default();
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.batch_size, 32);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.timeout_secs, 10);
    }

    #[test]
    fn test_provider_builds_with_default_config() {
        let provider = OllamaEmbeddingProvider::from_env();
        assert!(provider.is_ok(), "should build without error");
    }

    #[test]
    fn test_provider_metadata() {
        let provider = OllamaEmbeddingProvider::from_env().unwrap();
        assert_eq!(provider.dimensions(), 768);
        assert_eq!(provider.name(), "ollama-nomic-embed-text");
        assert_eq!(provider.max_batch_size(), 32);
    }

    #[test]
    fn test_custom_config() {
        let config = OllamaConfig {
            base_url: "http://custom:9999".to_owned(),
            model: "other-model".to_owned(),
            batch_size: 16,
            timeout_secs: 30,
            max_retries: 5,
            api_key: None,
        };
        let provider = OllamaEmbeddingProvider::new(config);
        assert!(provider.is_ok());
        let p = provider.unwrap();
        assert_eq!(p.max_batch_size(), 16);
    }
}
