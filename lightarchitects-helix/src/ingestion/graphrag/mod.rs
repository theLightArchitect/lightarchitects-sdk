//! `GraphRAG` ingestion pipeline — document parsing + entity extraction + graph writing.
//!
//! # Architecture
//!
//! ```text
//! DocumentParser  →  EntityExtractor  →  GraphBuilder
//!   (segments)        (triples)            (Neo4j steps + links)
//! ```
//!
//! The three stages are composable independently, but [`DocumentIngestor`]
//! wires them together as a single [`IngestionSource`] implementation.
//!
//! # Usage
//!
//! ```no_run
//! use lightarchitects_helix::ingestion::graphrag::{
//!     DocumentIngestor, DocumentIngestorConfig, IngestSource, DocumentFormat,
//! };
//! use lightarchitects_helix::ingestion::IngestionSource;
//!
//! # async fn example(db: &impl lightarchitects_helix::HelixDb) -> Result<(), Box<dyn std::error::Error>> {
//! let config = DocumentIngestorConfig {
//!     source: IngestSource::File("/path/to/paper.md".into()),
//!     owner: "user".into(),
//!     domain: Some("research".into()),
//!     ..Default::default()
//! };
//! let ingestor = DocumentIngestor::new(config, None);
//! let report = ingestor.ingest(db).await?;
//! println!("{} entities extracted", report.records_added);
//! # Ok(())
//! # }
//! ```

pub mod document_parser;
pub mod entity_extractor;
pub mod graph_builder;

// ─── Public re-exports ────────────────────────────────────────────────────────
//
// These are declared BEFORE the module body so they serve as the `use` source
// for all names used below. No separate `use` imports needed.

pub use document_parser::{ChunkerConfig, DocumentFormat, DocumentParser, ParsedDocument, Segment};
pub use entity_extractor::{
    CompletionProvider, Entity, EntityExtractor, Extraction, Relation, SegmentExtraction,
};
pub use graph_builder::{GraphBuildError, GraphBuildResult, GraphBuilder, sanitize_entity_name};

use std::sync::Arc;

use async_trait::async_trait;

use crate::db::HelixDb;
use document_parser::ParseError;

use super::{IngestionError, IngestionReport, IngestionSource};

// ─── IngestSource ─────────────────────────────────────────────────────────────

/// Source specification for [`DocumentIngestor`].
///
/// Supports file paths and inline text. Inline text is assigned the
/// `source_id` provided in [`DocumentIngestorConfig`].
#[derive(Debug, Clone)]
pub enum IngestSource {
    /// Path to a file on disk.
    File(std::path::PathBuf),
    /// Inline text with an explicit source identifier.
    Inline {
        /// Unique identifier for this content (e.g. a title or slug).
        source_id: String,
        /// The text content to parse.
        text: String,
        /// Content format. Defaults to `Plaintext` when not specified.
        format: DocumentFormat,
    },
}

impl IngestSource {
    /// Derive a human-readable source identifier.
    #[must_use]
    pub fn source_id(&self) -> String {
        match self {
            Self::File(path) => path.file_stem().map_or_else(
                || "unknown".to_owned(),
                |s| s.to_string_lossy().into_owned(),
            ),
            Self::Inline { source_id, .. } => source_id.clone(),
        }
    }
}

// ─── DocumentIngestorConfig ───────────────────────────────────────────────────

/// Configuration for a [`DocumentIngestor`].
#[derive(Debug, Clone)]
pub struct DocumentIngestorConfig {
    /// Source to parse — file or inline text.
    pub source: IngestSource,
    /// Owner/sibling name (e.g., `"user"`, `"eva"`).
    pub owner: String,
    /// Optional domain tag (e.g., `"research"`, `"pharma"`).
    pub domain: Option<String>,
    /// Chunk size for the document parser (characters). Defaults to 2048.
    pub chunk_chars: usize,
    /// Overlap size for the document parser (characters). Defaults to 256.
    pub overlap_chars: usize,
}

impl Default for DocumentIngestorConfig {
    fn default() -> Self {
        Self {
            source: IngestSource::Inline {
                source_id: "inline".to_owned(),
                text: String::new(),
                format: DocumentFormat::Plaintext,
            },
            owner: "user".to_owned(),
            domain: None,
            chunk_chars: 2048,
            overlap_chars: 256,
        }
    }
}

// ─── DocumentIngestor ─────────────────────────────────────────────────────────

/// [`IngestionSource`] that runs the full `GraphRAG` pipeline.
///
/// 1. Parses the source into segments via [`DocumentParser`].
/// 2. Extracts entities + relations via [`EntityExtractor`].
/// 3. Writes steps + links to Neo4j via [`GraphBuilder`].
///
/// When `completion_provider` is `None`, falls back to the embedding-signal
/// heuristic extractor (no LLM call, lower recall but always available).
pub struct DocumentIngestor {
    config: DocumentIngestorConfig,
    completion_provider: Option<Arc<dyn CompletionProvider>>,
}

impl DocumentIngestor {
    /// Create a new ingestor with optional completion provider.
    ///
    /// Pass `Some(provider)` to enable LLM-backed extraction.
    /// Pass `None` to use the embedding-signal heuristic fallback.
    #[must_use]
    pub fn new(
        config: DocumentIngestorConfig,
        completion_provider: Option<Arc<dyn CompletionProvider>>,
    ) -> Self {
        Self {
            config,
            completion_provider,
        }
    }
}

#[async_trait]
impl IngestionSource for DocumentIngestor {
    fn name(&self) -> &'static str {
        "DocumentIngestor"
    }

    async fn ingest(&self, db: &dyn HelixDb) -> Result<IngestionReport, IngestionError> {
        // Cap: prevent unbounded LLM calls or graph writes on oversized documents.
        const MAX_SEGMENTS: usize = 512;

        let chunker_config = ChunkerConfig {
            target_chars: self.config.chunk_chars,
            overlap_chars: self.config.overlap_chars,
        };
        let parser = DocumentParser::new(chunker_config);

        // 1. Parse
        let parsed = parse_source(&parser, &self.config.source).map_err(|e| match e {
            ParseError::UnsupportedFormat(msg) => IngestionError::Parse(msg),
            ParseError::Io(io) => IngestionError::Io(io),
            ParseError::EmptyContent => {
                IngestionError::Parse("content is empty after normalisation".to_owned())
            }
        })?;

        let source_id = parsed.source_id.clone();
        let mut parsed = parsed;
        if parsed.segments.len() > MAX_SEGMENTS {
            tracing::warn!(
                count = parsed.segments.len(),
                limit = MAX_SEGMENTS,
                "segment count exceeds limit, truncating"
            );
            parsed.segments.truncate(MAX_SEGMENTS);
        }

        // 2. Extract — use fallback when no provider is configured
        let extractor = match &self.completion_provider {
            Some(p) => EntityExtractor::new(Arc::clone(p)),
            None => EntityExtractor::with_fallback_only(),
        };
        let extractions = extractor.extract_all(&parsed.segments).await;

        // 3. Build graph
        let mut builder = GraphBuilder::new(db, &self.config.owner);
        if let Some(ref domain) = self.config.domain {
            builder = builder.with_domain(domain);
        }

        builder
            .build(&source_id, &extractions)
            .await
            .map_err(|e| IngestionError::Parse(format!("graph build failed: {e}")))
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Parse the source via the document parser.
fn parse_source(
    parser: &DocumentParser,
    source: &IngestSource,
) -> Result<ParsedDocument, ParseError> {
    match source {
        IngestSource::File(path) => parser.parse_file(path),
        IngestSource::Inline {
            source_id,
            text,
            format,
        } => parser.parse_inline(text, source_id, *format),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn ingest_source_file_derives_id() {
        let src = IngestSource::File("/tmp/my_document.md".into());
        assert_eq!(src.source_id(), "my_document");
    }

    #[test]
    fn ingest_source_inline_uses_given_id() {
        let src = IngestSource::Inline {
            source_id: "paper-2024".to_owned(),
            text: "Hello world.".to_owned(),
            format: DocumentFormat::Plaintext,
        };
        assert_eq!(src.source_id(), "paper-2024");
    }

    #[test]
    fn default_config_has_sensible_defaults() {
        let config = DocumentIngestorConfig::default();
        assert_eq!(config.owner, "user");
        assert_eq!(config.chunk_chars, 2048);
        assert_eq!(config.overlap_chars, 256);
        assert!(config.domain.is_none());
    }

    #[test]
    fn document_ingestor_constructs_without_provider() {
        let config = DocumentIngestorConfig {
            source: IngestSource::Inline {
                source_id: "test".to_owned(),
                text: "Some content.".to_owned(),
                format: DocumentFormat::Plaintext,
            },
            owner: "user".to_owned(),
            ..Default::default()
        };
        let ingestor = DocumentIngestor::new(config, None);
        assert_eq!(ingestor.name(), "DocumentIngestor");
    }
}
