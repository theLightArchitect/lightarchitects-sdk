//! Mock embedding provider — deterministic vectors for testing.
//!
//! Generates reproducible embeddings from a hash of the input text.
//! No network calls, no model loading — instant, deterministic, testable.
//!
//! # Examples
//!
//! ```rust
//! use crate::soul::embedding::mock::MockEmbeddingProvider;
//! use crate::soul::embedding::EmbeddingProvider;
//!
//! let provider = MockEmbeddingProvider::nomic();
//! assert_eq!(provider.dimensions(), 768);
//! assert_eq!(provider.name(), "mock");
//! ```

use async_trait::async_trait;

use super::{EmbeddingError, EmbeddingProvider, EmbeddingResult};

// ============================================================================
// MockEmbeddingProvider
// ============================================================================

/// Deterministic embedding provider for tests and offline development.
///
/// Produces reproducible vectors by hashing the input text with FNV-1a
/// then seeding a simple LCG to fill the dimension vector.
/// Same text → same vector. Different text → different vector.
pub struct MockEmbeddingProvider {
    dims: usize,
}

impl MockEmbeddingProvider {
    /// Create a mock provider with the specified dimensions.
    #[must_use]
    pub fn new(dims: usize) -> Self {
        Self { dims }
    }

    /// Create a mock provider matching Ollama `nomic-embed-text` (768 dims).
    #[must_use]
    pub fn nomic() -> Self {
        Self::new(768)
    }

    /// Create a mock provider matching structural `Node2Vec` embeddings (128 dims).
    #[must_use]
    pub fn structural() -> Self {
        Self::new(128)
    }

    /// Generate a deterministic vector from text using FNV-1a hash + LCG.
    ///
    /// The vector is L2-normalised for cosine similarity compatibility.
    fn hash_to_vector(&self, text: &str) -> Vec<f32> {
        // FNV-1a over input bytes.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in text.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }

        // LCG seeded from hash to fill `dims` f32 values in [-1.0, 1.0].
        let mut state = hash;
        let mut vec = Vec::with_capacity(self.dims);
        for _ in 0..self.dims {
            state = state.wrapping_mul(0x5851_f42d_4c95_7f2d).wrapping_add(1);
            #[allow(clippy::cast_precision_loss)]
            let val = ((state >> 33) as f32 / (u32::MAX as f32)) * 2.0 - 1.0;
            vec.push(val);
        }

        // L2-normalise for cosine similarity compatibility.
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }

        vec
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        for text in texts {
            if text.is_empty() {
                return Err(EmbeddingError::InvalidInput("empty text in batch".into()));
            }
        }
        Ok(texts.iter().map(|t| self.hash_to_vector(t)).collect())
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn name(&self) -> &'static str {
        "mock"
    }

    fn max_batch_size(&self) -> usize {
        // In-process — no practical limit.
        usize::MAX
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_deterministic_embeddings() {
        let provider = MockEmbeddingProvider::nomic();
        let v1 = provider.embed(&["hello world"]).await.unwrap();
        let v2 = provider.embed(&["hello world"]).await.unwrap();
        assert_eq!(v1, v2, "same input must produce same output");
    }

    #[tokio::test]
    async fn test_different_inputs_differ() {
        let provider = MockEmbeddingProvider::nomic();
        let v1 = provider.embed(&["hello"]).await.unwrap();
        let v2 = provider.embed(&["world"]).await.unwrap();
        assert_ne!(v1, v2, "different inputs should produce different vectors");
    }

    #[tokio::test]
    async fn test_correct_nomic_dimensions() {
        let provider = MockEmbeddingProvider::nomic();
        let results = provider.embed(&["test"]).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].len(), 768);
    }

    #[tokio::test]
    async fn test_correct_structural_dimensions() {
        let provider = MockEmbeddingProvider::structural();
        let results = provider.embed(&["test"]).await.unwrap();
        assert_eq!(results[0].len(), 128);
    }

    #[tokio::test]
    async fn test_batch_embedding_count() {
        let provider = MockEmbeddingProvider::nomic();
        let texts = ["one", "two", "three"];
        let results = provider.embed(&texts).await.unwrap();
        assert_eq!(results.len(), 3);
        for vec in &results {
            assert_eq!(vec.len(), 768);
        }
    }

    #[tokio::test]
    async fn test_empty_batch_returns_empty() {
        let provider = MockEmbeddingProvider::nomic();
        let results = provider.embed(&[]).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_empty_text_rejected() {
        let provider = MockEmbeddingProvider::nomic();
        let result = provider.embed(&[""]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_l2_normalized() {
        let provider = MockEmbeddingProvider::nomic();
        let results = provider.embed(&["normalize me"]).await.unwrap();
        let norm: f32 = results[0].iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "vector should be L2-normalised, got norm={norm}"
        );
    }

    #[test]
    fn test_provider_metadata() {
        let provider = MockEmbeddingProvider::nomic();
        assert_eq!(provider.dimensions(), 768);
        assert_eq!(provider.name(), "mock");
        assert_eq!(provider.max_batch_size(), usize::MAX);
    }
}
