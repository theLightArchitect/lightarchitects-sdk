//! Cloud embedding provider — OpenAI-compatible API.
//!
//! **Disabled by default.** Requires explicit `cloud_embedding.enabled = true` in config.
//! API key stored as `secrecy::SecretString` — never logged, never serialized.
//!
//! Privacy gate: Steps with `metadata.privacy == "local"` or `metadata.redacted == true`
//! are NEVER sent to cloud providers. This is enforced at the pipeline level.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tracing::{instrument, warn};

use super::{EmbeddingError, EmbeddingProvider, EmbeddingResult};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the cloud embedding provider.
#[derive(Clone)]
pub struct CloudConfig {
    /// API endpoint (OpenAI-compatible).
    pub endpoint: String,
    /// Model name.
    pub model: String,
    /// API key (secret — never logged).
    pub api_key: SecretString,
    /// Output dimensions.
    pub dimensions: usize,
    /// Maximum batch size.
    pub batch_size: usize,
    /// Request timeout.
    pub timeout: Duration,
    /// Whether cloud embedding is enabled.
    pub enabled: bool,
}

impl std::fmt::Debug for CloudConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudConfig")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("api_key", &"[REDACTED]")
            .field("dimensions", &self.dimensions)
            .field("batch_size", &self.batch_size)
            .field("timeout", &self.timeout)
            .field("enabled", &self.enabled)
            .finish()
    }
}

// ============================================================================
// OpenAI API types
// ============================================================================

#[derive(Serialize)]
struct EmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f64>,
}

// ============================================================================
// CloudEmbeddingProvider
// ============================================================================

/// Cloud-based embedding provider (OpenAI-compatible API).
///
/// **Disabled by default.** The privacy gate at the pipeline level ensures
/// local-only steps never reach this provider.
pub struct CloudEmbeddingProvider {
    config: CloudConfig,
    client: reqwest::Client,
}

impl CloudEmbeddingProvider {
    /// Create a new cloud embedding provider.
    ///
    /// # Errors
    /// Returns error if disabled or if the HTTP client cannot be built.
    pub fn new(config: CloudConfig) -> EmbeddingResult<Self> {
        if !config.enabled {
            return Err(EmbeddingError::Provider(
                "cloud embedding disabled — set enabled=true to use".into(),
            ));
        }

        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| EmbeddingError::Provider(format!("HTTP client: {e}")))?;

        Ok(Self { config, client })
    }

    /// Check if cloud embedding requires explicit opt-in.
    #[must_use]
    pub fn requires_explicit_opt_in() -> bool {
        true
    }
}

#[async_trait]
impl EmbeddingProvider for CloudEmbeddingProvider {
    #[instrument(skip(self, texts), fields(count = texts.len(), model = %self.config.model))]
    async fn embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        for text in texts {
            if text.is_empty() {
                return Err(EmbeddingError::InvalidInput("empty text in batch".into()));
            }
        }

        let request = EmbedRequest {
            model: self.config.model.clone(),
            input: texts.iter().map(|&t| t.to_owned()).collect(),
        };

        let resp = self
            .client
            .post(&self.config.endpoint)
            .bearer_auth(self.config.api_key.expose_secret())
            .json(&request)
            .send()
            .await
            .map_err(|e| EmbeddingError::Provider(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!(status = %status, "Cloud embed failed");
            return Err(EmbeddingError::Provider(format!(
                "cloud API {status}: {body}"
            )));
        }

        let parsed: EmbedResponse = resp
            .json()
            .await
            .map_err(|e| EmbeddingError::Provider(format!("JSON parse: {e}")))?;

        let embeddings: Vec<Vec<f32>> = parsed
            .data
            .into_iter()
            .map(|d| {
                d.embedding
                    .into_iter()
                    .map(|v| {
                        #[allow(clippy::cast_possible_truncation)]
                        let val = v as f32;
                        val
                    })
                    .collect()
            })
            .collect();

        if embeddings.len() != texts.len() {
            return Err(EmbeddingError::Provider(format!(
                "expected {} embeddings, got {}",
                texts.len(),
                embeddings.len()
            )));
        }

        Ok(embeddings)
    }

    fn dimensions(&self) -> usize {
        self.config.dimensions
    }

    fn name(&self) -> &'static str {
        "cloud"
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
    fn test_disabled_by_default() {
        let config = CloudConfig {
            endpoint: "https://api.openai.com/v1/embeddings".to_owned(),
            model: "text-embedding-3-small".to_owned(),
            api_key: SecretString::from("test-key".to_owned()),
            dimensions: 1536,
            batch_size: 32,
            timeout: Duration::from_secs(30),
            enabled: false,
        };
        let result = CloudEmbeddingProvider::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_requires_explicit_opt_in() {
        assert!(CloudEmbeddingProvider::requires_explicit_opt_in());
    }

    #[test]
    fn test_enabled_config() {
        let config = CloudConfig {
            endpoint: "https://api.openai.com/v1/embeddings".to_owned(),
            model: "text-embedding-3-small".to_owned(),
            api_key: SecretString::from("test-key".to_owned()),
            dimensions: 1536,
            batch_size: 32,
            timeout: Duration::from_secs(30),
            enabled: true,
        };
        let result = CloudEmbeddingProvider::new(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_debug_redacts_key() {
        let config = CloudConfig {
            endpoint: "https://api.example.com".to_owned(),
            model: "test".to_owned(),
            api_key: SecretString::from("super-secret-key".to_owned()),
            dimensions: 768,
            batch_size: 16,
            timeout: Duration::from_secs(10),
            enabled: true,
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("super-secret-key"));
    }
}
