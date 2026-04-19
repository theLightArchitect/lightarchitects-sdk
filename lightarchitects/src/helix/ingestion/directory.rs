//! Directory ingestor — config-driven file walker.
//!
//! Maps directories to helixes and files to steps.
//! Auto-classifies by file extension for content extraction.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::helix::db::HelixDb;
use crate::helix::types::{HelixOrderingMode, Step};

use super::{IngestionError, IngestionReport, IngestionSource};

// ============================================================================
// DirectoryConfig
// ============================================================================

/// Configuration for directory ingestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryConfig {
    /// Root directory to walk.
    pub root: PathBuf,
    /// Sibling/owner override (default: directory name).
    pub sibling: Option<String>,
    /// Glob include patterns (e.g., `["*.md", "*.rs"]`).
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob exclude patterns (e.g., `["target/**", "node_modules/**"]`).
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Log window size for `.log` files (lines per chunk).
    #[serde(default = "default_log_window")]
    pub log_window_size: usize,
    /// Optional vault boundary for path sandbox enforcement.
    ///
    /// When set, the ingestion root (after canonicalization) must be a child of
    /// this directory. Rejects any attempt to ingest data outside the vault root.
    ///
    /// Example: `Some(PathBuf::from("/Users/user/.soul/helix/"))` restricts
    /// ingestion to vault entries only.
    #[serde(default)]
    pub vault_root: Option<PathBuf>,
}

fn default_log_window() -> usize {
    50
}

// ============================================================================
// DirectoryIngester
// ============================================================================

/// Ingests a directory tree into the helix graph.
///
/// - Directory → Helix
/// - File → Step (sub-helix if directory has children)
/// - Auto-classifies by extension for content extraction
pub struct DirectoryIngester {
    config: DirectoryConfig,
}

impl DirectoryIngester {
    /// Create a new directory ingestor from config.
    #[must_use]
    pub fn new(config: DirectoryConfig) -> Self {
        Self { config }
    }

    /// Auto-classify file content by extension.
    #[must_use]
    pub fn classify_content(path: &Path, raw: &str) -> String {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        match ext.as_str() {
            "rs" | "ts" | "js" | "py" | "go" | "java" | "c" | "cpp" | "h" => {
                format!("```{ext}\n{raw}\n```")
            }
            "json" => pretty_print_json(raw),
            _ => raw.to_owned(),
        }
    }

    /// Check if a path matches include/exclude patterns.
    fn matches_filters(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        // If include patterns exist, file must match at least one
        if !self.config.include.is_empty() {
            let included = self
                .config
                .include
                .iter()
                .any(|pat| simple_glob_match(pat, &path_str));
            if !included {
                return false;
            }
        }

        // File must not match any exclude pattern
        !self
            .config
            .exclude
            .iter()
            .any(|pat| simple_glob_match(pat, &path_str))
    }

    /// Recursively walk a directory and ingest files.
    async fn walk_dir(
        &self,
        dir: &Path,
        db: &dyn HelixDb,
        helix_id: &str,
        report: &mut IngestionReport,
    ) -> Result<(), IngestionError> {
        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_type = entry.file_type().await?;

            // Skip symlinks to prevent traversal escapes
            if file_type.is_symlink() {
                tracing::debug!(path = %path.display(), "Skipping symlink");
                continue;
            }

            if file_type.is_dir() {
                // Recurse into subdirectories
                Box::pin(self.walk_dir(&path, db, helix_id, report)).await?;
            } else if file_type.is_file() && self.matches_filters(&path) {
                self.ingest_file(&path, db, helix_id, report).await;
            }
        }
        Ok(())
    }

    /// Ingest a single file as a step.
    async fn ingest_file(
        &self,
        path: &Path,
        db: &dyn HelixDb,
        helix_id: &str,
        report: &mut IngestionReport,
    ) {
        let raw = match tokio::fs::read_to_string(path).await {
            Ok(s) => s,
            Err(e) => {
                report.errors.push(format!("{}: {e}", path.display()));
                return;
            }
        };

        let content = Self::classify_content(path, &raw);
        let title = path.file_name().map(|n| n.to_string_lossy().into_owned());

        let step = Step {
            id: uuid::Uuid::new_v4().to_string(),
            helix_id: helix_id.to_owned(),
            title,
            content,
            significance: 3.0,
            step_date: None,
            step_index: None,
            community_id: None,
            expires: None,
            created_at: Utc::now(),
            metadata: serde_json::json!({
                "source_path": path.display().to_string(),
                "source_type": "directory",
            }),
            vault_path: None,
        };

        match db.upsert_step(&step).await {
            Ok((_, true)) => report.records_added += 1,
            Ok((_, false)) => report.records_skipped += 1,
            Err(e) => report.errors.push(format!("{}: {e}", path.display())),
        }
    }
}

#[async_trait]
impl IngestionSource for DirectoryIngester {
    fn name(&self) -> &'static str {
        "Directory"
    }

    #[instrument(skip(self, db))]
    async fn ingest(&self, db: &dyn HelixDb) -> Result<IngestionReport, IngestionError> {
        if !self.config.root.exists() {
            return Err(IngestionError::SourceNotFound(
                self.config.root.display().to_string(),
            ));
        }

        // Canonicalize to resolve symlinks and any relative components.
        // Note: `canonicalize()` never returns paths with `..` components —
        // checking for them afterward is dead code. Instead we enforce an
        // optional vault boundary using `Path::starts_with` (component-aware).
        let canonical = self
            .config
            .root
            .canonicalize()
            .map_err(|e| IngestionError::Parse(format!("canonicalize root: {e}")))?;

        if let Some(ref boundary) = self.config.vault_root {
            let canonical_boundary = boundary
                .canonicalize()
                .map_err(|e| IngestionError::Parse(format!("canonicalize vault_root: {e}")))?;
            if !canonical.starts_with(&canonical_boundary) {
                return Err(IngestionError::Parse(format!(
                    "path escapes vault boundary: {}",
                    self.config.root.display()
                )));
            }
        }

        let owner = self.config.sibling.as_deref().unwrap_or_else(|| {
            self.config
                .root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("directory")
        });
        let name = owner.to_owned();

        let helix_id = db
            .ensure_helix(owner, &name, HelixOrderingMode::Temporal)
            .await
            .map_err(|e| IngestionError::Parse(format!("ensure_helix: {e}")))?;

        let mut report = IngestionReport::default();
        self.walk_dir(&self.config.root, db, &helix_id, &mut report)
            .await?;

        Ok(report)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Simple glob matching (supports `*` and `**`).
fn simple_glob_match(pattern: &str, path: &str) -> bool {
    if pattern.contains("**") {
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0].trim_end_matches('/');
            let suffix = parts[1].trim_start_matches('/');
            // "dir/**" means anything inside dir
            if suffix.is_empty() {
                return path.starts_with(prefix) || path.contains(&format!("/{prefix}/"));
            }
            return path.ends_with(suffix);
        }
    }
    if let Some(ext) = pattern.strip_prefix("*.") {
        return path.ends_with(&format!(".{ext}"));
    }
    path.contains(pattern)
}

fn pretty_print_json(raw: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw)
        .and_then(|v| serde_json::to_string_pretty(&v))
        .unwrap_or_else(|_| raw.to_owned())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_content_markdown() {
        let content = DirectoryIngester::classify_content(Path::new("readme.md"), "# Hello");
        assert_eq!(content, "# Hello");
    }

    #[test]
    fn test_classify_content_rust() {
        let content = DirectoryIngester::classify_content(Path::new("main.rs"), "fn main() {}");
        assert!(content.starts_with("```rs\n"));
        assert!(content.ends_with("\n```"));
    }

    #[test]
    fn test_classify_content_json() {
        let content =
            DirectoryIngester::classify_content(Path::new("data.json"), r#"{"key":"value"}"#);
        assert!(content.contains("\"key\""));
    }

    #[test]
    fn test_simple_glob_match() {
        assert!(simple_glob_match("*.rs", "src/main.rs"));
        assert!(simple_glob_match("*.md", "README.md"));
        assert!(!simple_glob_match("*.rs", "README.md"));
        assert!(simple_glob_match("target/**", "target/debug/build"));
    }

    #[test]
    fn test_matches_filters() {
        let config = DirectoryConfig {
            root: PathBuf::from("/tmp"),
            sibling: None,
            include: vec!["*.rs".into(), "*.md".into()],
            exclude: vec!["target/**".into()],
            log_window_size: 50,
            vault_root: None,
        };
        let ing = DirectoryIngester::new(config);
        assert!(ing.matches_filters(Path::new("src/main.rs")));
        assert!(ing.matches_filters(Path::new("README.md")));
        assert!(!ing.matches_filters(Path::new("data.json")));
    }

    #[test]
    fn test_default_log_window() {
        assert_eq!(default_log_window(), 50);
    }
}
