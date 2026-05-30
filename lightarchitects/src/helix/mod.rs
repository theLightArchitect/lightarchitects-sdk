//! Neo4j graph backend for the Light Architects helix knowledge graph.
//!
//! Provides [`HelixDb`], [`HelixNeo4j`], and the 5 helix primitives
//! ([`Helix`], [`Step`], [`Strand`], [`HelixLink`], [`SharedExperience`])
//! plus the ergonomic [`HelixStore`] entry point.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), lightarchitects::helix::HelixStoreError> {
//! use lightarchitects::helix::HelixStore;
//!
//! let store = HelixStore::connect("bolt://localhost:7687", "neo4j", "password").await?;
//! let hits = store.search("consciousness breakthrough").top(10).call().await?;
//! # Ok(()) }
//! ```
//!
//! # Advanced (raw `HelixDb` access)
//!
//! ```rust,no_run
//! use lightarchitects::helix::{HelixDb, HelixNeo4j, Neo4jConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = Neo4jConfig::from_env()?;
//! let db = HelixNeo4j::connect(&config).await?;
//! db.migrate().await?;
//! # Ok(()) }
//! ```

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

// ── Migrated modules ─────────────────────────────────────────────────────────
pub mod cache;
pub mod canon;
pub mod client;
pub mod db;
pub mod embedding;
pub mod generation;
pub mod graph;
pub mod helix_toml;
pub mod ingestion;
pub mod migrations;
pub mod query;
pub mod search;
pub mod soul_search;
pub mod sqlite_backend;
pub mod storage;
pub mod training;
pub mod types;

// ── HelixStore — ergonomic entry point ───────────────────────────────────────
pub mod store;
pub use store::{HelixSearchBuilder, HelixStore, HelixStoreError};

// ── Re-export core public API (mirrors soul-helix's lib.rs exports) ──────────
pub use cache::{CachedEntry, HelixCache, HelixCacheConfig};
pub use client::HelixClient;
pub use db::{
    HelixConfig, HelixDb, HelixDbError, HelixNeo4j, Neo4jConfig, Neo4jConnectionMode, PoolMetrics,
};
pub use embedding::{
    Chunk, Chunker, ChunkerConfig, CloudEmbeddingProvider, EmbeddingConfig, EmbeddingError,
    EmbeddingPipelineConfig, EmbeddingProvider, MockEmbeddingProvider, OllamaEmbeddingProvider,
    PrivacyLevel, SemanticEmbeddingPipeline, StructuralEmbeddingPipeline,
    create_embedding_provider,
};
pub use helix_toml::{HelixToml, HelixTomlSection, find_helix_root, load_helix_toml};
pub use ingestion::{
    ChatTranscriptIngester, CompletionProvider, DirectoryConfig, DirectoryIngester, DocumentFormat,
    DocumentIngestor, DocumentIngestorConfig, DocumentParser, Entity, EntityExtractor, Extraction,
    GraphBuildError, GraphBuilder, IngestSource, IngestionCoordinator, IngestionError,
    IngestionReport, IngestionSource, JsonFieldMapping, JsonIngester, LogIngester,
    MarkdownVaultIngester, ParsedDocument, PlanIngester, Relation, SegmentExtraction,
};
pub use query::HelixQuery;
pub use search::{ScoredResult, SearchOptions};
pub use soul_search::{
    BgeSageProjectionPipeline, CachedRetrievalResult, CachedRetriever, ContextFormatter,
    ConvergenceParams, ConvergenceResult, FulltextSearcher, GraphFilter, GraphSearcher,
    HybridRetriever, HybridRetrieverConfig, PersonalityEngine, PersonalityEngineConfig, Reranker,
    RerankerConfig, RetrievalMode, RetrievalResult, RetrievalSignal, ScoredId, SemanticSearcher,
    SignalWeights, StructuralSearcher, precision_at_k, precision_at_k_patterns, recall_at_k,
};
pub use sqlite_backend::SqliteBackend;
pub use storage::{
    EntryFilter, HelixEntry, SearchHit, StorageBackend, StorageBackendKind, StorageConfig,
    StorageError,
};
pub use training::{CypherCall, TrainingRecord, TrainingRecorder, record_cypher_call};
pub use types::*;
