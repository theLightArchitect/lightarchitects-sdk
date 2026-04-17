//! Ingestion framework — universal trait for importing data into the helix graph.
//!
//! Every data source (markdown vault, chat transcripts, directories, JSON, etc.)
//! implements [`IngestionSource`]. The framework handles watermarking, dedup,
//! and error collection.
//!
//! # Watermarking
//!
//! Each source is tracked via a [`SourceWatermark`](lightarchitects_helix::types::SourceWatermark)
//! node in Neo4j. Incremental re-runs only process new/modified content.
//! SHA-256 content hashes prevent re-ingestion of unchanged files.

pub mod chat_transcript;
pub mod coordinator;
pub mod directory;
// frontmatter stays in soul-helix by design (different YAML schema).
// This shim re-exports the soul_helix types so ingestion sub-modules can use
// `use super::frontmatter` unchanged.
pub mod frontmatter;
pub mod graphrag;
pub mod log;
pub mod markdown_vault;
pub mod plan;
pub mod wikilink;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::db::HelixDb;

// Re-export ingestors for convenience.
pub use chat_transcript::ChatTranscriptIngester;
pub use coordinator::IngestionCoordinator;
pub use directory::{DirectoryConfig, DirectoryIngester};
pub use graphrag::{
    CompletionProvider, DocumentFormat, DocumentIngestor, DocumentIngestorConfig, DocumentParser,
    Entity, EntityExtractor, Extraction, GraphBuildError, GraphBuilder, IngestSource,
    ParsedDocument, Relation, SegmentExtraction,
};
pub use log::{JsonFieldMapping, JsonIngester, LogIngester};
pub use markdown_vault::MarkdownVaultIngester;
pub use plan::PlanIngester;

// ============================================================================
// IngestionSource Trait
// ============================================================================

/// A data source that can be ingested into the helix graph.
///
/// Implementations handle source-specific parsing (markdown frontmatter,
/// JSON records, chat transcripts, etc.) and produce steps, strands,
/// and links in the target helix.
///
/// # Watermarking
///
/// Implementations should check the source watermark before processing
/// and update it after successful completion. This enables incremental
/// ingestion — only new/modified content is processed on re-runs.
///
/// # Error Handling
///
/// Partial ingestion is acceptable. Errors on individual records should
/// be collected in [`IngestionReport::errors`] rather than aborting the
/// entire source. The watermark should NOT be updated if errors prevent
/// complete ingestion.
#[async_trait]
pub trait IngestionSource: Send + Sync {
    /// Human-readable name for this source (e.g., "`MarkdownVault`", "`ChatTranscript`").
    fn name(&self) -> &'static str;

    /// Ingest data from this source into the helix graph.
    ///
    /// Returns a report of what was processed.
    ///
    /// # Errors
    ///
    /// Returns an error only for fatal failures (cannot connect to source,
    /// cannot write to graph). Individual record errors are collected in
    /// the report.
    async fn ingest(&self, db: &dyn HelixDb) -> Result<IngestionReport, IngestionError>;
}

// ============================================================================
// IngestionReport
// ============================================================================

/// Report from an ingestion run.
///
/// Tracks counts of records added, updated, skipped, and any errors
/// encountered during processing.
///
/// # Graph-specific counters
///
/// [`nodes_added`] and [`edges_added`] are populated by [`GraphBuilder`] to
/// distinguish newly created entity steps from newly created relation links.
/// For non-graph ingestors, only [`records_added`] is used and the graph
/// counters remain zero.
///
/// [`GraphBuilder`]: crate::ingestion::graphrag::GraphBuilder
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IngestionReport {
    /// Number of new records (steps, strands, links) created.
    ///
    /// For graph ingestors, prefer [`nodes_added`] + [`edges_added`] for
    /// precise per-type counts. `records_added` remains populated for
    /// backward compatibility with non-graph callers.
    pub records_added: u64,
    /// Number of existing records updated (content changed).
    pub records_updated: u64,
    /// Number of records skipped (unchanged content hash).
    pub records_skipped: u64,
    /// Non-fatal errors encountered during ingestion.
    pub errors: Vec<String>,
    /// Number of entity nodes (Steps) newly created by the graph builder.
    ///
    /// Only incremented when a `MERGE` results in a new node; re-ingestion
    /// of an existing entity is not counted.
    pub nodes_added: u64,
    /// Number of relation edges (`HelixLink`s) created by the graph builder.
    pub edges_added: u64,
}

impl IngestionReport {
    /// Total records processed (added + updated + skipped).
    #[must_use]
    pub fn total_processed(&self) -> u64 {
        self.records_added
            .saturating_add(self.records_updated)
            .saturating_add(self.records_skipped)
    }

    /// Whether the ingestion completed without errors.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }

    /// Merge another report into this one.
    pub fn merge(&mut self, other: &IngestionReport) {
        self.records_added = self.records_added.saturating_add(other.records_added);
        self.records_updated = self.records_updated.saturating_add(other.records_updated);
        self.records_skipped = self.records_skipped.saturating_add(other.records_skipped);
        self.nodes_added = self.nodes_added.saturating_add(other.nodes_added);
        self.edges_added = self.edges_added.saturating_add(other.edges_added);
        self.errors.extend(other.errors.iter().cloned());
    }
}

// ============================================================================
// IngestionError
// ============================================================================

/// Fatal ingestion error — prevents the entire source from being processed.
///
/// Non-fatal per-record errors go into [`IngestionReport::errors`] instead.
#[derive(Debug, thiserror::Error)]
pub enum IngestionError {
    /// Source path does not exist or is not accessible.
    #[error("Source not found: {0}")]
    SourceNotFound(String),

    /// Graph database operation failed.
    #[error("Graph error: {0}")]
    Graph(#[from] crate::graph::GraphError),

    /// Source-specific parsing error.
    #[error("Parse error: {0}")]
    Parse(String),

    /// I/O error reading source data.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_report_default_is_clean() {
        let report = IngestionReport::default();
        assert!(report.is_clean());
        assert_eq!(report.total_processed(), 0);
    }

    #[test]
    fn test_report_total_processed() {
        let report = IngestionReport {
            records_added: 10,
            records_updated: 5,
            records_skipped: 3,
            errors: vec![],
            ..Default::default()
        };
        assert_eq!(report.total_processed(), 18);
    }

    #[test]
    fn test_report_with_errors() {
        let report = IngestionReport {
            records_added: 10,
            records_updated: 0,
            records_skipped: 0,
            errors: vec!["bad frontmatter in file.md".into()],
            ..Default::default()
        };
        assert!(!report.is_clean());
    }

    #[test]
    fn test_report_merge() {
        let mut a = IngestionReport {
            records_added: 10,
            records_updated: 2,
            records_skipped: 1,
            nodes_added: 8,
            edges_added: 3,
            errors: vec!["err1".into()],
        };
        let b = IngestionReport {
            records_added: 5,
            records_updated: 1,
            nodes_added: 4,
            edges_added: 2,
            errors: vec!["err2".into()],
            ..Default::default()
        };
        a.merge(&b);
        assert_eq!(a.records_added, 15);
        assert_eq!(a.records_updated, 3);
        assert_eq!(a.records_skipped, 1);
        assert_eq!(a.nodes_added, 12);
        assert_eq!(a.edges_added, 5);
        assert_eq!(a.errors.len(), 2);
    }
}
