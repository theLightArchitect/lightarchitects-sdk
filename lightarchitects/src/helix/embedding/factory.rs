//! Embedding provider factory — creates a concrete [`EmbeddingProvider`] from config.
//!
//! Centralises backend selection so call sites (gateway, tests) don't need
//! `#[cfg(feature = "fastembed")]` branches. The `backend` string is recorded
//! in AYIN `soul.helix.retrieve` spans for observability.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{EmbeddingError, EmbeddingProvider};

// ============================================================================
// EmbeddingConfig
// ============================================================================

/// Selects and configures the embedding backend.
///
/// Serialises as the `embedding` sub-field of `HybridRetrieverConfig` so
/// the chosen backend is visible in plan YAML, gateway config, and AYIN spans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Backend identifier: `"fastembed"` | `"ollama"` | `"cloud"` | `"mock"`.
    ///
    /// Recorded verbatim in `soul.helix.retrieve` AYIN spans (F11 observability).
    #[serde(default = "EmbeddingConfig::default_backend")]
    pub backend: String,
    /// Model name or URL, interpreted by the chosen backend.
    ///
    /// - `fastembed`: `"nomic-embed-text-v1.5"` or `"all-minilm-l6-v2"`
    /// - `ollama`: model tag, e.g. `"nomic-embed-text"`
    /// - `cloud`: API endpoint URL
    /// - `mock`: ignored
    #[serde(default = "EmbeddingConfig::default_model")]
    pub model: String,
    /// Expected embedding dimensionality (used for validation at startup).
    ///
    /// Mismatches between the chosen model and the Neo4j HNSW index are caught
    /// at `PlatformState::new()` time rather than on the first query.
    #[serde(default = "EmbeddingConfig::default_dim")]
    pub dim: usize,
}

impl EmbeddingConfig {
    fn default_backend() -> String {
        "fastembed".into()
    }

    fn default_model() -> String {
        "nomic-embed-text-v1.5".into()
    }

    fn default_dim() -> usize {
        768
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            backend: Self::default_backend(),
            model: Self::default_model(),
            dim: Self::default_dim(),
        }
    }
}

// ============================================================================
// Factory
// ============================================================================

/// Create an [`EmbeddingProvider`] from `config`.
///
/// Backend selection:
/// - `"fastembed"` — in-process ONNX via `fastembed` crate (ARM64-native; requires
///   `fastembed` feature). `embed()` offloads to `spawn_blocking` — safe on async runtimes.
/// - `"ollama"` — HTTP to a running Ollama server at `http://localhost:11434`.
/// - `"mock"` — deterministic zero vectors; for tests and CI.
/// - `"cloud"` — reserved; returns `EmbeddingError::Config` with a hint.
///
/// # Errors
///
/// Returns [`EmbeddingError::Config`] for an unknown `backend` string, or
/// [`EmbeddingError::Model`] if fastembed model initialisation fails.
pub fn create_embedding_provider(
    config: &EmbeddingConfig,
) -> Result<Arc<dyn EmbeddingProvider>, EmbeddingError> {
    match config.backend.as_str() {
        "fastembed" => create_fastembed(config),
        "ollama" => create_ollama(config),
        "mock" => Ok(Arc::new(super::mock::MockEmbeddingProvider::new(
            config.dim,
        ))),
        "cloud" => Err(EmbeddingError::Provider(
            "cloud backend requires explicit CloudEmbeddingProvider construction".into(),
        )),
        other => Err(EmbeddingError::Provider(format!(
            "unknown embedding backend {other:?}; expected fastembed|ollama|mock|cloud"
        ))),
    }
}

// ── fastembed ─────────────────────────────────────────────────────────────────

#[cfg(feature = "fastembed")]
fn create_fastembed(
    config: &EmbeddingConfig,
) -> Result<Arc<dyn EmbeddingProvider>, EmbeddingError> {
    use super::fastembed_provider::{FastEmbedConfig, FastEmbedModel, FastEmbedProvider};

    let model = match config.model.as_str() {
        "nomic-embed-text-v1.5" | "nomic-embed-text" | "" => FastEmbedModel::NomicEmbedTextV15,
        "all-minilm-l6-v2" | "all-MiniLM-L6-v2" => FastEmbedModel::AllMiniLML6V2,
        other => {
            return Err(EmbeddingError::Model(format!(
                "unknown fastembed model {other:?}"
            )));
        }
    };

    let provider = FastEmbedProvider::new(FastEmbedConfig {
        model,
        ..FastEmbedConfig::default()
    })?;
    Ok(Arc::new(provider))
}

#[cfg(not(feature = "fastembed"))]
fn create_fastembed(
    _config: &EmbeddingConfig,
) -> Result<Arc<dyn EmbeddingProvider>, EmbeddingError> {
    Err(EmbeddingError::Provider(
        "fastembed backend requires the 'fastembed' Cargo feature".into(),
    ))
}

// ── ollama ────────────────────────────────────────────────────────────────────

fn create_ollama(config: &EmbeddingConfig) -> Result<Arc<dyn EmbeddingProvider>, EmbeddingError> {
    use super::ollama::{OllamaConfig, OllamaEmbeddingProvider};

    let model = if config.model.is_empty() {
        "nomic-embed-text".to_owned()
    } else {
        config.model.clone()
    };

    let ollama_cfg = OllamaConfig {
        model,
        ..OllamaConfig::default()
    };
    Ok(Arc::new(OllamaEmbeddingProvider::new(ollama_cfg)?))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_default_backend_is_fastembed() {
        let cfg = EmbeddingConfig::default();
        assert_eq!(cfg.backend, "fastembed");
        assert_eq!(cfg.dim, 768);
    }

    #[test]
    fn test_mock_backend_returns_provider() {
        let cfg = EmbeddingConfig {
            backend: "mock".into(),
            model: String::new(),
            dim: 768,
        };
        let provider = create_embedding_provider(&cfg).unwrap();
        assert_eq!(provider.dimensions(), 768);
    }

    #[test]
    fn test_unknown_backend_returns_error() {
        let cfg = EmbeddingConfig {
            backend: "unknown-backend".into(),
            model: String::new(),
            dim: 768,
        };
        let err = create_embedding_provider(&cfg)
            .err()
            .expect("expected Err for unknown backend");
        assert!(matches!(err, EmbeddingError::Provider(_)));
    }

    #[test]
    fn test_cloud_returns_helpful_error() {
        let cfg = EmbeddingConfig {
            backend: "cloud".into(),
            model: String::new(),
            dim: 768,
        };
        let err = create_embedding_provider(&cfg)
            .err()
            .expect("expected Err for cloud backend");
        assert!(matches!(err, EmbeddingError::Provider(_)));
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let cfg = EmbeddingConfig {
            backend: "ollama".into(),
            model: "nomic-embed-text".into(),
            dim: 768,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let rt: EmbeddingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.backend, "ollama");
        assert_eq!(rt.dim, 768);
    }
}
