//! Ingestion coordinator — parallel multi-source ingestion with dedup.
//!
//! Orchestrates multiple [`IngestionSource`] implementations, merging
//! their reports and updating source watermarks after completion.

use chrono::Utc;
use tracing::instrument;

use crate::db::HelixDb;
use crate::types::SourceWatermark;

use super::{IngestionError, IngestionReport, IngestionSource};

// ============================================================================
// IngestionCoordinator
// ============================================================================

/// Coordinates parallel ingestion from multiple sources.
///
/// Dedup is handled at the step level by `upsert_step` (SHA-256 content hash).
/// All writes go to a single Neo4j database — ACID transactional, no dual-write.
pub struct IngestionCoordinator {
    /// Registered ingestion sources.
    sources: Vec<Box<dyn IngestionSource>>,
    /// Whether to skip database writes (preview mode).
    dry_run: bool,
}

impl IngestionCoordinator {
    /// Create a new coordinator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            dry_run: false,
        }
    }

    /// Register an ingestion source.
    pub fn add_source(&mut self, source: impl IngestionSource + 'static) {
        self.sources.push(Box::new(source));
    }

    /// Enable dry-run mode (no database writes).
    #[must_use]
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Run all registered sources and merge reports.
    ///
    /// Sources are run sequentially to avoid Neo4j connection contention.
    /// Dedup is handled per-step by `upsert_step` content-hash matching.
    ///
    /// # Errors
    ///
    /// Returns the merged report even if individual sources fail.
    /// Fatal source errors are recorded in the report's error list.
    #[instrument(skip(self, db))]
    pub async fn run(&self, db: &dyn HelixDb) -> IngestionReport {
        let mut merged = IngestionReport::default();

        for source in &self.sources {
            tracing::info!(source = source.name(), "Starting ingestion");

            match source.ingest(db).await {
                Ok(report) => {
                    tracing::info!(
                        source = source.name(),
                        added = report.records_added,
                        updated = report.records_updated,
                        skipped = report.records_skipped,
                        errors = report.errors.len(),
                        "Ingestion complete"
                    );
                    merged.merge(&report);
                }
                Err(e) => {
                    tracing::error!(source = source.name(), error = %e, "Ingestion failed");
                    merged.errors.push(format!("{} FATAL: {e}", source.name()));
                }
            }
        }

        merged
    }

    /// Run all sources and update watermarks for successful ones.
    ///
    /// # Errors
    ///
    /// Returns [`IngestionError`] only if watermark updates fail.
    /// Source-level errors are captured in the report.
    #[instrument(skip(self, db))]
    pub async fn run_with_watermarks(
        &self,
        db: &dyn HelixDb,
    ) -> Result<IngestionReport, IngestionError> {
        let mut merged = IngestionReport::default();

        for source in &self.sources {
            tracing::info!(source = source.name(), "Starting ingestion");

            match source.ingest(db).await {
                Ok(report) => {
                    let added = report.records_added;
                    merged.merge(&report);

                    // Update watermark if records were actually added
                    if added > 0 && !self.dry_run {
                        let watermark = SourceWatermark {
                            id: uuid::Uuid::new_v4().to_string(),
                            source_type: source.name().to_owned(),
                            path: source.name().to_owned(),
                            last_ingested_at: Utc::now(),
                            content_hash: None,
                            record_count: added,
                        };
                        if let Err(e) = db.register_source(&watermark).await {
                            merged
                                .errors
                                .push(format!("{} watermark: {e}", source.name()));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(source = source.name(), error = %e, "Ingestion failed");
                    merged.errors.push(format!("{} FATAL: {e}", source.name()));
                }
            }
        }

        Ok(merged)
    }

    /// Number of registered sources.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }
}

impl Default for IngestionCoordinator {
    fn default() -> Self {
        Self::new()
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
    fn test_coordinator_default() {
        let coord = IngestionCoordinator::new();
        assert_eq!(coord.source_count(), 0);
        assert!(!coord.dry_run);
    }

    #[test]
    fn test_coordinator_dry_run() {
        let coord = IngestionCoordinator::new().with_dry_run(true);
        assert!(coord.dry_run);
    }
}
