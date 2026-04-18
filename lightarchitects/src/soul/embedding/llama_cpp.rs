//! `llama.cpp` HTTP embedding provider.
//!
//! [`LlamaCppEmbeddingProvider`] calls the `/embedding` endpoint on a running
//! `llama.cpp` server. Compatible with any GGUF embedding model loaded in
//! the server — including models shipped with the SOUL binary.
//!
//! Default base URL: `http://localhost:8080` (llama.cpp default port).
//!
//! # Usage
//!
//! ```rust,no_run
//! # #[cfg(feature = "embedding-llama-cpp")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use lightarchitects::soul::embedding::llama_cpp::LlamaCppEmbeddingProvider;
//!
//! let provider = LlamaCppEmbeddingProvider::new("http://localhost:8080");
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::soul::embedding::{EmbeddingError, EmbeddingProvider, EmbeddingResult};

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct EmbedRequest<'a> {
    content: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

// ============================================================================
// LlamaCppEmbeddingProvider
// ============================================================================

/// HTTP embedding provider for a running `llama.cpp` server.
///
/// Calls `POST {base_url}/embedding` for each text. Dimensions depend on
/// the GGUF model loaded in the server — check the server logs or model
/// card for the expected dimension count.
pub struct LlamaCppEmbeddingProvider {
    client: Client,
    embed_url: String,
    /// Dimension inferred from the first successful embed call.
    dims: std::sync::OnceLock<usize>,
}

impl LlamaCppEmbeddingProvider {
    /// Create a provider pointing at the given `base_url`.
    ///
    /// The trailing `/embedding` path is appended automatically.
    ///
    /// # Default
    ///
    /// `LlamaCppEmbeddingProvider::new("http://localhost:8080")` connects to
    /// the default llama.cpp port.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        let base = base_url.into();
        let base = base.trim_end_matches('/').to_owned();
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            embed_url: format!("{base}/embedding"),
            dims: std::sync::OnceLock::new(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for LlamaCppEmbeddingProvider {
    async fn embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            let resp = self
                .client
                .post(&self.embed_url)
                .json(&EmbedRequest { content: text })
                .send()
                .await
                .map_err(|e| EmbeddingError::Provider(format!("llama.cpp request failed: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(EmbeddingError::Provider(format!(
                    "llama.cpp returned HTTP {status}: {body}"
                )));
            }

            let parsed: EmbedResponse = resp
                .json()
                .await
                .map_err(|e| EmbeddingError::Provider(format!("llama.cpp parse error: {e}")))?;

            // Infer dimensions from the first successful response.
            let _ = self.dims.set(parsed.embedding.len());

            results.push(parsed.embedding);
        }

        Ok(results)
    }

    fn dimensions(&self) -> usize {
        // Unknown until the first embed call — return 0 as sentinel.
        // Callers that need the dimension before the first call should
        // make a probe request or configure the dimension explicitly.
        self.dims.get().copied().unwrap_or(0)
    }

    fn name(&self) -> &'static str {
        "llama.cpp"
    }

    fn max_batch_size(&self) -> usize {
        // llama.cpp processes one embedding at a time via /embedding.
        // The loop in embed() handles batching by iterating.
        1
    }
}
