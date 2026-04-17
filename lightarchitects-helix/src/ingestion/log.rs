//! Log ingestor — NDJSON or plain-text windowed chunks.
//!
//! - NDJSON: one Step per JSON line (timestamp, severity, message)
//! - Plain-text: windowed chunks (configurable window size)

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use tracing::instrument;

use crate::db::HelixDb;
use crate::types::{HelixOrderingMode, Step};

use super::{IngestionError, IngestionReport, IngestionSource};

// ============================================================================
// LogIngester
// ============================================================================

/// Ingests log files into the helix graph.
///
/// Auto-detects NDJSON vs plain-text. NDJSON lines are parsed for
/// timestamp/severity/message. Plain-text is chunked into windows.
pub struct LogIngester {
    /// Path to the log file.
    path: PathBuf,
    /// Owner/sibling name.
    owner: String,
    /// Window size for plain-text chunking (lines per chunk).
    window_size: usize,
}

impl LogIngester {
    /// Create a new log ingestor.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, owner: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            owner: owner.into(),
            window_size: 50,
        }
    }

    /// Set the window size for plain-text chunking.
    #[must_use]
    pub fn with_window_size(mut self, size: usize) -> Self {
        self.window_size = size.max(1);
        self
    }

    /// Derive significance from log severity.
    fn severity_significance(severity: &str) -> f64 {
        match severity.to_uppercase().as_str() {
            "ERROR" | "FATAL" | "CRITICAL" => 7.0,
            "WARN" | "WARNING" => 5.0,
            "DEBUG" | "TRACE" => 2.0,
            _ => 3.0,
        }
    }

    /// Try to parse a line as NDJSON.
    #[must_use]
    pub fn parse_ndjson_line(line: &str) -> Option<(Option<String>, String, String)> {
        let obj: serde_json::Value = serde_json::from_str(line).ok()?;
        let msg = obj
            .get("message")
            .or_else(|| obj.get("msg"))
            .and_then(serde_json::Value::as_str)?
            .to_owned();
        let severity = obj
            .get("severity")
            .or_else(|| obj.get("level"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("INFO")
            .to_owned();
        let timestamp = obj
            .get("timestamp")
            .or_else(|| obj.get("ts"))
            .or_else(|| obj.get("time"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        Some((timestamp, severity, msg))
    }

    /// Ingest NDJSON lines.
    async fn ingest_ndjson(
        &self,
        lines: &[&str],
        db: &dyn HelixDb,
        helix_id: &str,
        report: &mut IngestionReport,
    ) {
        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Some((_ts, severity, msg)) = Self::parse_ndjson_line(line) else {
                report
                    .errors
                    .push(format!("Invalid NDJSON: {}", &line[..line.len().min(80)]));
                continue;
            };
            let step = Step {
                id: uuid::Uuid::new_v4().to_string(),
                helix_id: helix_id.to_owned(),
                title: None,
                content: msg,
                significance: Self::severity_significance(&severity),
                step_date: None,
                step_index: None,
                community_id: None,
                expires: None,
                created_at: Utc::now(),
                metadata: serde_json::json!({"severity": severity, "source_type": "log"}),
            };
            match db.upsert_step(&step).await {
                Ok((_, true)) => report.records_added += 1,
                Ok((_, false)) => report.records_skipped += 1,
                Err(e) => report.errors.push(format!("log step: {e}")),
            }
        }
    }

    /// Ingest plain-text with windowed chunks.
    async fn ingest_plain(
        &self,
        lines: &[&str],
        db: &dyn HelixDb,
        helix_id: &str,
        report: &mut IngestionReport,
    ) {
        for (chunk_idx, chunk) in lines.chunks(self.window_size).enumerate() {
            let content = chunk.join("\n");
            if content.trim().is_empty() {
                continue;
            }
            let step = Step {
                id: uuid::Uuid::new_v4().to_string(),
                helix_id: helix_id.to_owned(),
                title: Some(format!("chunk-{}", chunk_idx + 1)),
                content,
                significance: 3.0,
                step_date: None,
                step_index: i64::try_from(chunk_idx).ok(),
                community_id: None,
                expires: None,
                created_at: Utc::now(),
                metadata: serde_json::json!({"source_type": "log", "chunk": chunk_idx + 1}),
            };
            match db.upsert_step(&step).await {
                Ok((_, true)) => report.records_added += 1,
                Ok((_, false)) => report.records_skipped += 1,
                Err(e) => report.errors.push(format!("chunk {}: {e}", chunk_idx + 1)),
            }
        }
    }
}

#[async_trait]
impl IngestionSource for LogIngester {
    fn name(&self) -> &'static str {
        "Log"
    }

    #[instrument(skip(self, db))]
    async fn ingest(&self, db: &dyn HelixDb) -> Result<IngestionReport, IngestionError> {
        if !self.path.exists() {
            return Err(IngestionError::SourceNotFound(
                self.path.display().to_string(),
            ));
        }

        let raw = tokio::fs::read_to_string(&self.path).await?;
        let helix_name = log_name_from_path(&self.path);

        let helix_id = db
            .ensure_helix(&self.owner, &helix_name, HelixOrderingMode::Temporal)
            .await
            .map_err(|e| IngestionError::Parse(format!("ensure_helix: {e}")))?;

        let mut report = IngestionReport::default();
        let lines: Vec<&str> = raw.lines().collect();

        // Auto-detect: if first non-empty line is valid JSON, treat as NDJSON
        let is_ndjson = lines
            .iter()
            .find(|l| !l.trim().is_empty())
            .is_some_and(|l| serde_json::from_str::<serde_json::Value>(l).is_ok());

        if is_ndjson {
            self.ingest_ndjson(&lines, db, &helix_id, &mut report).await;
        } else {
            self.ingest_plain(&lines, db, &helix_id, &mut report).await;
        }

        Ok(report)
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn log_name_from_path(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "log".to_owned(), |s| s.to_string_lossy().into_owned())
}

// ============================================================================
// JsonIngester
// ============================================================================

/// Configuration for JSON array ingestion.
#[derive(Debug, Clone)]
pub struct JsonFieldMapping {
    /// Field name for step content.
    pub content_field: String,
    /// Field name for step title (optional).
    pub title_field: Option<String>,
    /// Field name for significance score (optional).
    pub significance_field: Option<String>,
    /// Field name for timestamp (optional).
    pub timestamp_field: Option<String>,
}

impl Default for JsonFieldMapping {
    fn default() -> Self {
        Self {
            content_field: "content".to_owned(),
            title_field: Some("title".to_owned()),
            significance_field: None,
            timestamp_field: None,
        }
    }
}

/// Ingests a JSON array file — one Step per element.
pub struct JsonIngester {
    /// Path to the JSON file.
    path: PathBuf,
    /// Owner/sibling name.
    owner: String,
    /// Field mapping configuration.
    mapping: JsonFieldMapping,
}

impl JsonIngester {
    /// Create a new JSON ingestor.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, owner: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            owner: owner.into(),
            mapping: JsonFieldMapping::default(),
        }
    }

    /// Set custom field mapping.
    #[must_use]
    pub fn with_mapping(mut self, mapping: JsonFieldMapping) -> Self {
        self.mapping = mapping;
        self
    }

    /// Extract a string field from a JSON object.
    fn get_str<'a>(obj: &'a serde_json::Value, field: &str) -> Option<&'a str> {
        obj.get(field).and_then(serde_json::Value::as_str)
    }
}

#[async_trait]
impl IngestionSource for JsonIngester {
    fn name(&self) -> &'static str {
        "Json"
    }

    #[instrument(skip(self, db))]
    async fn ingest(&self, db: &dyn HelixDb) -> Result<IngestionReport, IngestionError> {
        if !self.path.exists() {
            return Err(IngestionError::SourceNotFound(
                self.path.display().to_string(),
            ));
        }

        let raw = tokio::fs::read_to_string(&self.path).await?;
        let arr: Vec<serde_json::Value> = serde_json::from_str(&raw)
            .map_err(|e| IngestionError::Parse(format!("JSON parse: {e}")))?;

        let helix_name = log_name_from_path(&self.path);
        let helix_id = db
            .ensure_helix(&self.owner, &helix_name, HelixOrderingMode::Temporal)
            .await
            .map_err(|e| IngestionError::Parse(format!("ensure_helix: {e}")))?;

        let mut report = IngestionReport::default();

        for (idx, obj) in arr.iter().enumerate() {
            let Some(content) = Self::get_str(obj, &self.mapping.content_field) else {
                report.errors.push(format!(
                    "element {idx}: missing field '{}'",
                    self.mapping.content_field
                ));
                continue;
            };

            let title = self
                .mapping
                .title_field
                .as_deref()
                .and_then(|f| Self::get_str(obj, f))
                .map(str::to_owned);

            let significance = self
                .mapping
                .significance_field
                .as_deref()
                .and_then(|f| obj.get(f))
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(3.0);

            let step_date = self
                .mapping
                .timestamp_field
                .as_deref()
                .and_then(|f| Self::get_str(obj, f))
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());

            let step = Step {
                id: uuid::Uuid::new_v4().to_string(),
                helix_id: helix_id.clone(),
                title,
                content: content.to_owned(),
                significance,
                step_date,
                step_index: i64::try_from(idx).ok(),
                community_id: None,
                expires: None,
                created_at: Utc::now(),
                metadata: serde_json::json!({"source_type": "json", "index": idx}),
            };

            match db.upsert_step(&step).await {
                Ok((_, true)) => report.records_added += 1,
                Ok((_, false)) => report.records_skipped += 1,
                Err(e) => report.errors.push(format!("element {idx}: {e}")),
            }
        }

        Ok(report)
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
    fn test_severity_significance() {
        assert!((LogIngester::severity_significance("ERROR") - 7.0).abs() < f64::EPSILON);
        assert!((LogIngester::severity_significance("WARN") - 5.0).abs() < f64::EPSILON);
        assert!((LogIngester::severity_significance("INFO") - 3.0).abs() < f64::EPSILON);
        assert!((LogIngester::severity_significance("DEBUG") - 2.0).abs() < f64::EPSILON);
        assert!((LogIngester::severity_significance("unknown") - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_ndjson_line() {
        let line =
            r#"{"timestamp":"2026-01-15T10:00:00Z","severity":"ERROR","message":"disk full"}"#;
        let (ts, sev, msg) = LogIngester::parse_ndjson_line(line).unwrap();
        assert!(ts.is_some());
        assert_eq!(sev, "ERROR");
        assert_eq!(msg, "disk full");
    }

    #[test]
    fn test_parse_ndjson_line_alt_fields() {
        let line = r#"{"level":"warn","msg":"low memory"}"#;
        let (ts, sev, msg) = LogIngester::parse_ndjson_line(line).unwrap();
        assert!(ts.is_none());
        assert_eq!(sev, "warn");
        assert_eq!(msg, "low memory");
    }

    #[test]
    fn test_parse_ndjson_line_invalid() {
        assert!(LogIngester::parse_ndjson_line("not json").is_none());
    }

    #[test]
    fn test_json_field_mapping_default() {
        let m = JsonFieldMapping::default();
        assert_eq!(m.content_field, "content");
        assert_eq!(m.title_field.as_deref(), Some("title"));
    }

    #[test]
    fn test_log_name_from_path() {
        assert_eq!(log_name_from_path(Path::new("/var/log/app.log")), "app");
    }
}
