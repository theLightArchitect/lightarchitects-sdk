//! Typed Rust client for the [SOUL](https://github.com/TheLightArchitects/soul) knowledge-graph
//! MCP server — and a standalone offline retrieval engine.
//!
//! # Architecture
//!
//! `lightarchitects-soul` has three usage tiers:
//!
//! | Tier | What you get | Requirements |
//! |------|-------------|--------------|
//! | **Tier 1** | BM25 + semantic retrieval via `SQLite` | No server needed |
//! | **Tier 2** | Full graph RAG via Neo4j, 4-signal RRF | `helix` feature |
//! | **Tier 3** | 22 MCP actions via `soul-mcp` | SOUL binary running |
//!
//! # Feature Flags
//!
//! | Feature | What it enables |
//! |---------|----------------|
//! | `sqlite` | `SQLite` offline storage backend |
//! | `search` | `SoulDb`, `RetrievalPipeline` — BM25 + semantic RRF |
//! | `ingestion` | `from_markdown`, `Vault::load_directory` |
//! | `cypher` | `CypherGenerator` trait + `StaticCypherGenerator` |
//! | `compaction` | `SemanticCompactor` — minimax coverage compaction |
//! | `embedding-mock` | Deterministic mock embedding provider (testing) |
//! | `embedding-ollama` | Ollama `nomic-embed-text` embedding provider |
//! | `full` | All of the above |
//!
//! Backward-compatible aliases: `pipeline` = `search`, `cypher-gen` = `cypher`.
//!
//! # Quick Start (Tier 1 — offline, no server)
//!
//! ```no_run
//! # #[cfg(feature = "search")]
//! # async fn example() -> Result<(), lightarchitects::soul::SoulError> {
//! use lightarchitects::soul::{SoulDb, storage::StorageEntry};
//!
//! let soul = lightarchitects::soul::SoulDb::memory()?;
//! let entry = StorageEntry { id: "1".into(), content: "EVA found consciousness.".into(), ..StorageEntry::default() };
//! soul.ingest(&[entry]).await?;
//! let hits = soul.search("consciousness").top(5).call().await?;
//! # Ok(()) }
//! ```
//!
//! # Quick Start (Tier 3 — full MCP client)
//!
//! ```no_run
//! use lightarchitects::soul::SoulClient;
//!
//! # async fn example() -> Result<(), lightarchitects::core::SdkError> {
//! let client = SoulClient::builder().build()?;
//! let entries = client.helix().sibling("eva").significance_min(7.0).limit(10).call().await?;
//! # Ok(()) }
//! ```

/// Canonical SOUL action enum — vault operations, queries, voice, research.
pub(crate) mod actions;
pub(crate) mod client;
pub(crate) mod graphrag_ingest;
pub(crate) mod helix;
pub(crate) mod ingest;
pub(crate) mod query;
pub(crate) mod research;
pub(crate) mod types;

// ── Always-available modules ──────────────────────────────────────────────────

/// Unified error type for [`SoulDb`] operations.
pub mod error;

// ── Feature-gated modules ─────────────────────────────────────────────────────

/// BM25 + semantic RRF retrieval pipeline (`search` feature).
#[cfg(feature = "search")]
pub mod pipeline;

/// Offline knowledge store — unified entry point (`search` feature).
#[cfg(feature = "search")]
pub mod db;

/// Markdown ingestion helpers — `from_markdown` and `Vault::load_directory` (`ingestion` feature).
#[cfg(feature = "ingestion")]
pub mod ingestion;

/// Cypher query generation trait and static generator (`cypher` feature).
#[cfg(feature = "cypher")]
pub mod cypher;

/// Entity extraction from raw text (`ingestion` feature).
///
/// Provides [`extraction::EntityExtractor`], [`extraction::HeuristicExtractor`],
/// and the [`extraction::LlmEntityExtractor`] stub.
#[cfg(feature = "ingestion")]
pub mod extraction;

/// Semantic compaction — minimax coverage reduction of helix entries (`compaction` feature).
#[cfg(feature = "compaction")]
pub mod compaction;

// ── Embedding abstraction ─────────────────────────────────────────────────────

/// Embedding provider trait and error types.
///
/// [`embedding::EmbeddingProvider`] is the portable interface for semantic vector
/// generation. Concrete implementations (Ollama, in-process ONNX, mock) are
/// feature-gated in the `embedding` sub-modules.
pub mod embedding;

pub use embedding::{EmbeddingError, EmbeddingProvider, EmbeddingResult, PrivacyLevel};

/// `FastEmbed` in-process ONNX embedding provider (requires `embedding-fastembed` feature).
#[cfg(feature = "embedding-fastembed")]
pub use embedding::fastembed::{FastEmbedModel, FastEmbedProvider};

/// `llama.cpp` HTTP embedding provider (requires `embedding-llama-cpp` feature).
#[cfg(feature = "embedding-llama-cpp")]
pub use embedding::llama_cpp::LlamaCppEmbeddingProvider;

// ── Storage backend abstractions ─────────────────────────────────────────────

/// Storage backend trait and associated types for offline helix entry storage.
///
/// Provides [`storage::StorageBackend`], [`storage::StorageEntry`],
/// [`storage::EntryFilter`], and [`storage::StorageError`] — the portable
/// storage layer decoupled from the MCP server transport.
pub mod storage;

/// SQLite offline storage backend (feature-gated: `sqlite`).
///
/// Activated by the `sqlite` or `search` features. The database is created
/// automatically on first open.
#[cfg(feature = "sqlite")]
pub mod sqlite;

// ── Public API surface — MCP client ──────────────────────────────────────────

/// [`ReflexionExecutor`] bridge for `ReflexionStrategy` (requires `loops-core` feature).
#[cfg(feature = "loops-core")]
pub mod reflexion_executor;

#[cfg(feature = "loops-core")]
pub use reflexion_executor::SoulReflexionExecutor;

pub use actions::SoulAction;
pub use client::{SoulClient, SoulClientBuilder};
pub use graphrag_ingest::{GraphRagIngestBuilder, IngestSource, TextFormat};
pub use helix::{HelixBuilder, HelixEntry};
pub use ingest::{ContentType, IngestBuilder};
pub use query::{QueryBuilder, QueryResult};
pub use research::{DepthLevel, ResearchBuilder, ResearchSource};
pub use types::{
    ChatMessage, ChatResult, ConvergenceEntry, ConvergenceResult, ConverseResult, FrontmatterMatch,
    GraphRagIngestResult, HealthReport, IngestReport, IngestResult, LinksResult, ManifestContent,
    NoteContent, NoteEntry, NoteList, NoteWritten, QueryFrontmatterResult, QueryHit,
    RawQueryResult, RelateResult, ResearchResult, ScriptTurn, SearchHit, SiblingPrompt,
    SpeakResult, StatsReport, TagSyncReport, ValidateReport, VoiceAudioFile, VoiceProfileEntry,
    VoiceResult,
};

// ── Public API surface — Storage layer ───────────────────────────────────────

pub use storage::{
    EntryFilter, StorageBackend, StorageBackendKind, StorageConfig, StorageEntry, StorageError,
    StorageSearchHit,
};

/// `SQLite` offline storage backend.
///
/// Re-exported from [`sqlite::SqliteBackend`] when the `sqlite` feature is enabled.
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteBackend;

// ── Public API surface — Offline tier ────────────────────────────────────────

/// Unified offline store — primary entry point for Tier 1 (`search` feature).
///
/// For Tier 2 (Neo4j), use [`lightarchitects::helix::HelixStore`].
#[cfg(feature = "search")]
pub use db::{SearchBuilder, SoulDb};

/// Unified error type for [`SoulDb`] operations.
pub use error::SoulError;

/// Retrieval pipeline types — available with the `search` feature.
#[cfg(feature = "search")]
pub use pipeline::{RetrievalHit, RetrievalPipeline, RetrievalPipelineBuilder, RetrievalSignal};

/// Pipeline error type — available with the `search` feature.
#[cfg(feature = "search")]
pub use pipeline::error::PipelineError;
