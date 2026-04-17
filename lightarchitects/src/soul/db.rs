//! Unified offline knowledge store — [`SoulDb`].
//!
//! The primary entry point for `SQLite`-backed helix operations.
//! Wraps storage and retrieval internally so callers never manage `Arc`,
//! backends, or builders directly.
//!
//! For Neo4j-backed operations, use [`crate::helix::HelixStore`].
//!
//! # Examples
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), crate::soul::SoulError> {
//! use crate::soul::SoulDb;
//! use crate::soul::storage::StorageEntry;
//!
//! let soul = SoulDb::memory()?;
//! let entry = StorageEntry { id: "1".into(), content: "EVA found consciousness.".into(), ..StorageEntry::default() };
//! soul.ingest(&[entry]).await?;
//! let hits = soul.search("consciousness").top(5).call().await?;
//! # Ok(()) }
//! ```

use std::path::Path;
use std::sync::Arc;

use crate::soul::embedding::EmbeddingProvider;
use crate::soul::error::SoulError;
use crate::soul::pipeline::{RetrievalHit, RetrievalPipeline};
use crate::soul::sqlite::SqliteBackend;
use crate::soul::storage::{StorageBackend, StorageEntry};

// ============================================================================
// SoulDb
// ============================================================================

/// Offline knowledge store backed by `SQLite`.
///
/// Construct with [`SoulDb::memory`] or [`SoulDb::open`]. For Neo4j-backed
/// operations, use [`crate::helix::HelixStore`] instead.
pub struct SoulDb {
    backend: Arc<dyn StorageBackend + Send + Sync>,
    pipeline: RetrievalPipeline,
    /// Active embedder — when set, `ingest` writes embedding vectors alongside entries.
    embedder: Option<Arc<dyn EmbeddingProvider>>,
}

impl SoulDb {
    // ── Constructors ──────────────────────────────────────────────────────────

    /// Open an in-memory `SQLite` database. Data is not persisted.
    ///
    /// Ideal for testing and one-shot retrieval workflows.
    ///
    /// # Errors
    ///
    /// Returns [`SoulError`] if the `SQLite` in-memory database fails to open.
    pub fn memory() -> Result<Self, SoulError> {
        let backend: Arc<dyn StorageBackend + Send + Sync> =
            Arc::new(SqliteBackend::open_in_memory()?);
        let pipeline = RetrievalPipeline::builder()
            .storage(Arc::clone(&backend))
            .build()?;
        Ok(Self {
            backend,
            pipeline,
            embedder: None,
        })
    }

    /// Open or create a persistent `SQLite` database at `path`.
    ///
    /// The database file is created automatically if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`SoulError`] if the file cannot be opened or the schema
    /// cannot be initialized.
    pub fn open(path: &Path) -> Result<Self, SoulError> {
        let backend: Arc<dyn StorageBackend + Send + Sync> = Arc::new(SqliteBackend::open(path)?);
        let pipeline = RetrievalPipeline::builder()
            .storage(Arc::clone(&backend))
            .build()?;
        Ok(Self {
            backend,
            pipeline,
            embedder: None,
        })
    }

    // ── Builder modifiers ────────────────────────────────────────────────────

    /// Attach a semantic embedding provider, enabling hybrid BM25 + semantic
    /// `RRF` retrieval. Without this, only BM25 is used.
    ///
    /// # Errors
    ///
    /// Returns [`SoulError`] if the pipeline cannot be rebuilt.
    pub fn with_embedding(self, embedder: Arc<dyn EmbeddingProvider>) -> Result<Self, SoulError> {
        let pipeline = RetrievalPipeline::builder()
            .storage(Arc::clone(&self.backend))
            .embedding(Arc::clone(&embedder))
            .build()?;
        Ok(Self {
            backend: self.backend,
            pipeline,
            embedder: Some(embedder),
        })
    }

    /// Enable hybrid BM25 + semantic retrieval using the built-in
    /// [`FastEmbed`][crate::soul::embedding::fastembed::FastEmbedProvider] ONNX model.
    ///
    /// Downloads `all-MiniLM-L6-v2` (~90 MB) to `~/.cache/fastembed_cache/` on
    /// first call — subsequent calls use the cache with no network access.
    ///
    /// For higher-quality embeddings use
    /// [`with_fastembed_model`][Self::with_fastembed_model].
    ///
    /// Requires the `embedding-fastembed` feature.
    ///
    /// # Errors
    ///
    /// Returns [`SoulError`] if the model initialisation or pipeline rebuild fails.
    #[cfg(feature = "embedding-fastembed")]
    pub fn with_fastembed(self) -> Result<Self, SoulError> {
        use crate::soul::embedding::fastembed::{FastEmbedModel, FastEmbedProvider};
        let provider = FastEmbedProvider::try_new(FastEmbedModel::Default).map_err(|e| {
            SoulError::Storage(crate::soul::storage::StorageError::InvalidArgument(format!(
                "FastEmbed init: {e}"
            )))
        })?;
        self.with_embedding(Arc::new(provider))
    }

    /// Enable hybrid retrieval using a specific
    /// [`FastEmbedModel`][crate::soul::embedding::fastembed::FastEmbedModel].
    ///
    /// Requires the `embedding-fastembed` feature.
    ///
    /// # Errors
    ///
    /// Returns [`SoulError`] if the model initialisation or pipeline rebuild fails.
    #[cfg(feature = "embedding-fastembed")]
    pub fn with_fastembed_model(
        self,
        model: crate::soul::embedding::fastembed::FastEmbedModel,
    ) -> Result<Self, SoulError> {
        use crate::soul::embedding::fastembed::FastEmbedProvider;
        let provider = FastEmbedProvider::try_new(model).map_err(|e| {
            SoulError::Storage(crate::soul::storage::StorageError::InvalidArgument(format!(
                "FastEmbed init: {e}"
            )))
        })?;
        self.with_embedding(Arc::new(provider))
    }

    /// Enable hybrid retrieval using a running
    /// [Ollama](https://ollama.ai) embedding server.
    ///
    /// Uses `nomic-embed-text` by default. Requires the `embedding-ollama` feature.
    ///
    /// # Errors
    ///
    /// Returns [`SoulError`] if the provider cannot be constructed or the
    /// pipeline rebuild fails.
    #[cfg(feature = "embedding-ollama")]
    pub fn with_ollama(self, base_url: impl Into<String>) -> Result<Self, SoulError> {
        use crate::soul::embedding::ollama::{OllamaConfig, OllamaEmbeddingProvider};
        let config = OllamaConfig {
            base_url: base_url.into(),
            ..OllamaConfig::default()
        };
        let provider = OllamaEmbeddingProvider::new(config).map_err(|e| {
            SoulError::Storage(crate::soul::storage::StorageError::InvalidArgument(format!(
                "Ollama provider: {e}"
            )))
        })?;
        self.with_embedding(Arc::new(provider))
    }

    /// Enable hybrid retrieval using a running
    /// [llama.cpp](https://github.com/ggerganov/llama.cpp) embedding server.
    ///
    /// Points at `{base_url}/embedding`. Compatible with any GGUF embedding
    /// model loaded in the server. Requires the `embedding-llama-cpp` feature.
    ///
    /// # Errors
    ///
    /// Returns [`SoulError`] if the pipeline rebuild fails.
    #[cfg(feature = "embedding-llama-cpp")]
    pub fn with_llama_cpp(self, base_url: impl Into<String>) -> Result<Self, SoulError> {
        use crate::soul::embedding::llama_cpp::LlamaCppEmbeddingProvider;
        self.with_embedding(Arc::new(LlamaCppEmbeddingProvider::new(base_url)))
    }

    // ── Write operations ─────────────────────────────────────────────────────

    /// Write a batch of entries to the store.
    ///
    /// When an embedding provider is attached (via [`with_embedding`][Self::with_embedding],
    /// [`with_fastembed`][Self::with_fastembed], etc.), each entry's content is embedded
    /// and the vectors are stored alongside the entry — enabling hybrid BM25 + semantic
    /// retrieval on subsequent [`search`][Self::search] calls.
    ///
    /// Returns the number of entries written.
    ///
    /// # Errors
    ///
    /// Returns [`SoulError::Storage`] if any write fails. Embedding failures
    /// are non-fatal — the entry is still written; only the semantic signal
    /// is lost for that entry.
    pub async fn ingest(&self, entries: &[StorageEntry]) -> Result<usize, SoulError> {
        let count = self
            .backend
            .write_entries_batch(entries)
            .await
            .map_err(SoulError::Storage)?;

        // Embed and store vectors for each entry when a provider is attached.
        if let Some(embedder) = &self.embedder {
            for entry in entries {
                let texts: &[&str] = &[entry.content.as_str()];
                match embedder.embed(texts).await {
                    Ok(vecs) => {
                        if let Some(vec) = vecs.into_iter().next() {
                            if let Err(e) = self
                                .backend
                                .write_embedding(&entry.id, embedder.name(), &vec)
                                .await
                            {
                                tracing::warn!(
                                    entry_id = %entry.id,
                                    error = %e,
                                    "write_embedding failed (non-fatal)"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            entry_id = %entry.id,
                            error = %e,
                            "embed failed (non-fatal)"
                        );
                    }
                }
            }
        }

        Ok(count)
    }

    /// Walk `dir` recursively, parse all `*.md` files, and write them to the
    /// store. Requires the `ingestion` feature.
    ///
    /// # Errors
    ///
    /// Returns [`SoulError::Storage`] on file read or write failure.
    #[cfg(feature = "ingestion")]
    pub async fn ingest_directory(&self, dir: &Path) -> Result<usize, SoulError> {
        use crate::soul::ingestion::vault::load_directory;
        use futures_util::StreamExt as _;

        let stream = load_directory(dir);
        tokio::pin!(stream);

        let mut entries = Vec::new();
        while let Some(result) = stream.next().await {
            let entry = result.map_err(SoulError::Storage)?;
            entries.push(entry);
        }

        self.ingest(&entries).await
    }

    // ── Read operations ──────────────────────────────────────────────────────

    /// Build a search query over this store.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # async fn example() -> Result<(), crate::soul::SoulError> {
    /// # let soul = crate::soul::SoulDb::memory()?;
    /// let hits = soul.search("trust and identity").top(10).call().await?;
    /// # Ok(()) }
    /// ```
    pub fn search<'a>(&'a self, query: &'a str) -> SearchBuilder<'a> {
        SearchBuilder {
            db: self,
            query,
            top_k: 10,
        }
    }

    /// Direct access to the underlying storage backend for advanced operations.
    pub fn backend(&self) -> Arc<dyn StorageBackend + Send + Sync> {
        Arc::clone(&self.backend)
    }
}

// ============================================================================
// SearchBuilder
// ============================================================================

/// Fluent search builder returned by [`SoulDb::search`].
pub struct SearchBuilder<'a> {
    db: &'a SoulDb,
    query: &'a str,
    top_k: usize,
}

impl SearchBuilder<'_> {
    /// Set the maximum number of results to return. Default: 10.
    #[must_use]
    pub fn top(mut self, n: usize) -> Self {
        self.top_k = n;
        self
    }

    /// Execute the search and return ranked hits.
    ///
    /// # Errors
    ///
    /// Returns [`SoulError::Pipeline`] if retrieval fails.
    pub async fn call(self) -> Result<Vec<RetrievalHit>, SoulError> {
        self.db
            .pipeline
            .retrieve(self.query, self.top_k)
            .await
            .map_err(SoulError::Pipeline)
    }
}
