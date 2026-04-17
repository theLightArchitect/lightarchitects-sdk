//! Ollama embedding provider — re-export from `lightarchitects-soul`.
//!
//! The canonical implementation lives in `lightarchitects-soul` (feature `embedding-ollama`).
//! This module re-exports it so that the path
//! `lightarchitects_helix::embedding::ollama::{OllamaConfig, OllamaEmbeddingProvider}`
//! resolves correctly for callers.

pub use lightarchitects_soul::embedding::ollama::{OllamaConfig, OllamaEmbeddingProvider};
