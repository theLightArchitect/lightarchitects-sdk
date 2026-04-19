//! Markdown vault ingestor — recursively walks `{soul_home}/helix/{sibling}/` for all `.md` files.
//!
//! Parses YAML frontmatter, extracts wikilinks, assigns strands,
//! creates shared experiences from convergence references, and
//! discovers co-located attachments.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use tracing::instrument;

use crate::helix::db::HelixDb;
use crate::helix::types::{
    DiscoveryMethod, HelixLink, HelixOrderingMode, LinkType, SharedExperience, Step,
    StrandMembership,
};

use super::frontmatter;
use super::wikilink;
use super::{IngestionError, IngestionReport, IngestionSource};

// ============================================================================
// MarkdownVaultIngester
// ============================================================================

/// Ingests helix entries from the SOUL vault markdown directory.
///
/// Recursively walks `{soul_home}/helix/{sibling}/` and creates
/// graph nodes for every `.md` file with YAML frontmatter: Helix → Step →
/// Strand assignments, wikilinks, attachments, and shared experiences.
/// Files without valid frontmatter are silently skipped.
pub struct MarkdownVaultIngester {
    /// Path to the vault root (e.g., `~/lightarchitects/soul/helix`).
    vault_root: PathBuf,
    /// Sibling name (e.g., "eva", "corso").
    sibling: String,
    /// Cap on total entries processed (for incremental testing).
    max_entries: Option<usize>,
    /// Re-run wikilink resolution on content-hash-skipped Steps.
    ///
    /// When `false` (default), an unchanged entry short-circuits the
    /// per-file pipeline — skipping strand, wikilink, and attachment writes.
    /// Set `true` to re-invoke only `create_wikilinks` on skipped Steps so
    /// previously unresolvable `[[wikilinks]]` can be materialised without
    /// wiping the graph. Useful after a deploy that added wikilink
    /// resolution logic (e.g. Phase 11.5 `vault_path` fallback).
    force_wikilinks: bool,
}

impl MarkdownVaultIngester {
    /// Create a new vault ingestor for a specific sibling.
    #[must_use]
    pub fn new(vault_root: impl Into<PathBuf>, sibling: impl Into<String>) -> Self {
        Self {
            vault_root: vault_root.into(),
            sibling: sibling.into(),
            max_entries: None,
            force_wikilinks: false,
        }
    }

    /// Cap the number of markdown entries processed (useful before a full 800-entry run).
    #[must_use]
    pub fn with_max_entries(mut self, n: usize) -> Self {
        self.max_entries = Some(n);
        self
    }

    /// Re-run wikilink resolution on Steps skipped by content-hash dedup.
    ///
    /// Leaves Step content, strands, and attachments untouched. Only
    /// `create_wikilinks` is re-invoked — which is idempotent because
    /// [`HelixDb::create_link`] uses a single MERGE under the hood.
    #[must_use]
    pub fn with_force_wikilinks(mut self, force: bool) -> Self {
        self.force_wikilinks = force;
        self
    }

    /// Returns the sibling root directory path (walks entire subtree).
    fn sibling_dir(&self) -> PathBuf {
        self.vault_root.join(&self.sibling)
    }

    /// Process a single markdown file into graph operations.
    #[instrument(skip(self, db), fields(path = %path.display()))]
    async fn ingest_file(
        &self,
        path: &Path,
        db: &dyn HelixDb,
        helix_id: &str,
        report: &mut IngestionReport,
    ) -> Result<(), IngestionError> {
        let content = tokio::fs::read_to_string(path).await?;
        let (fm, body) = frontmatter::parse(&content);

        // Hub nodes (strands, epochs, resonance hubs) are structural catalog nodes.
        // Ingesting them as :Step nodes would pollute the graph; they're already
        // represented via :Strand and :Helix relationships.
        if fm.entry_type.as_deref() == Some("hub") {
            report.records_skipped = report.records_skipped.saturating_add(1);
            return Ok(());
        }

        let vault_path = path
            .strip_prefix(&self.vault_root)
            .ok()
            .and_then(|rel| rel.to_str())
            .map(str::to_owned);
        let step = Self::build_step(&fm, body, helix_id, vault_path);
        let (step_id, was_created) = db.upsert_step(&step).await.map_err(|e| {
            IngestionError::Parse(format!(
                "Failed to upsert step from {}: {e}",
                path.display()
            ))
        })?;

        if was_created {
            report.records_added += 1;
        } else {
            report.records_skipped += 1;
            if self.force_wikilinks {
                // Step body is unchanged (content-hash match), but wikilinks may
                // have been added/modified or previously failed to resolve.
                // Re-run wikilink resolution only; strand/convergence/attachment
                // writes stay idempotent so they don't need re-execution.
                self.create_wikilinks(db, &step_id, body, &fm.links, report)
                    .await;
            }
            return Ok(());
        }

        self.assign_strands(db, &step_id, &fm.strands, helix_id, report)
            .await;
        self.create_wikilinks(db, &step_id, body, &fm.links, report)
            .await;
        self.create_convergences(db, &step_id, &fm.convergence, report)
            .await;
        self.scan_attachments(db, &step_id, path, report).await;

        Ok(())
    }

    /// Build a `Step` from parsed frontmatter, body, and vault-relative path.
    ///
    /// `vault_path` is the file path relative to the vault root, used for
    /// wikilink resolution in `create_link` (path-slug fallback).
    fn build_step(
        fm: &frontmatter::Frontmatter,
        body: &str,
        helix_id: &str,
        vault_path: Option<String>,
    ) -> Step {
        let step_date = fm
            .date
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());

        Step {
            id: uuid::Uuid::new_v4().to_string(),
            helix_id: helix_id.to_owned(),
            title: fm.title.clone(),
            content: body.to_owned(),
            significance: fm.significance.unwrap_or(5.0),
            step_date,
            step_index: fm.entry_number,
            community_id: None,
            expires: None,
            created_at: Utc::now(),
            metadata: Self::build_metadata(fm),
            vault_path,
        }
    }

    /// Build metadata JSON from frontmatter extra fields.
    fn build_metadata(fm: &frontmatter::Frontmatter) -> serde_json::Value {
        let mut meta = serde_json::Map::new();
        if !fm.resonance.is_empty() {
            meta.insert("resonance".into(), serde_json::json!(fm.resonance));
        }
        if !fm.themes.is_empty() {
            meta.insert("themes".into(), serde_json::json!(fm.themes));
        }
        if let Some(epoch) = &fm.epoch {
            meta.insert("epoch".into(), serde_json::json!(epoch));
        }
        if let Some(sd) = fm.self_defining {
            meta.insert("self_defining".into(), serde_json::json!(sd));
        }
        // Privacy field: propagated into metadata so the embedding pipeline's
        // PrivacyLevel::from_metadata gate can read it.
        if let Some(privacy) = &fm.privacy {
            meta.insert("privacy".into(), serde_json::json!(privacy));
        }
        // entry_type: mirrored into metadata so upsert_step can write it as a
        // first-class Neo4j property without requiring a Step struct field change.
        if let Some(et) = &fm.entry_type {
            meta.insert("entry_type".into(), serde_json::json!(et));
        }
        serde_json::Value::Object(meta)
    }

    /// Assign strands from frontmatter to the step.
    async fn assign_strands(
        &self,
        db: &dyn HelixDb,
        step_id: &str,
        strands: &[String],
        helix_id: &str,
        report: &mut IngestionReport,
    ) {
        for strand_name in strands {
            let strand_result = db.ensure_strand(helix_id, strand_name).await;
            let Ok(strand_id) = strand_result else {
                report
                    .errors
                    .push(format!("ensure_strand failed: {strand_name}"));
                continue;
            };
            let membership = StrandMembership {
                step_id: step_id.to_owned(),
                strand_id,
                weight: 1.0,
            };
            if let Err(e) = db.assign_to_strand(&membership).await {
                report.errors.push(format!("assign_to_strand failed: {e}"));
            }
        }
    }

    /// Create wikilinks from content and frontmatter links.
    async fn create_wikilinks(
        &self,
        db: &dyn HelixDb,
        step_id: &str,
        body: &str,
        fm_links: &[frontmatter::LinkRef],
        report: &mut IngestionReport,
    ) {
        // Inline wikilinks from content
        for wl in wikilink::extract(body) {
            let link = HelixLink {
                source_id: step_id.to_owned(),
                target_id: wl.target.clone(),
                link_type: LinkType::Wikilink,
                strength: 1.0,
                raw_wikilink: Some(wl.raw),
                metadata: serde_json::Value::Object(serde_json::Map::new()),
            };
            if let Err(e) = db.create_link(&link).await {
                report
                    .errors
                    .push(format!("wikilink create failed ({}): {e}", wl.target));
            }
        }

        // Typed links from frontmatter
        for lr in fm_links {
            let link_type = parse_link_type(lr.link_type.as_deref());
            let link = HelixLink {
                source_id: step_id.to_owned(),
                target_id: lr.target.clone(),
                link_type,
                strength: lr.strength.unwrap_or(1.0),
                raw_wikilink: None,
                metadata: serde_json::Value::Object(serde_json::Map::new()),
            };
            if let Err(e) = db.create_link(&link).await {
                report
                    .errors
                    .push(format!("fm link create failed ({}): {e}", lr.target));
            }
        }
    }

    /// Create shared experiences from convergence references.
    async fn create_convergences(
        &self,
        db: &dyn HelixDb,
        step_id: &str,
        convergences: &[frontmatter::ConvergenceRef],
        report: &mut IngestionReport,
    ) {
        for conv in convergences {
            let mut all_ids = conv.step_ids.clone();
            if !all_ids.contains(&step_id.to_owned()) {
                all_ids.push(step_id.to_owned());
            }
            let experience = SharedExperience {
                id: uuid::Uuid::new_v4().to_string(),
                weight: conv.weight.unwrap_or(1.0),
                participant_count: all_ids.len(),
                discovered_by: DiscoveryMethod::Explicit,
                label: conv.label.clone(),
                created_at: Utc::now(),
            };
            if let Err(e) = db.create_shared_experience(&experience, &all_ids).await {
                report
                    .errors
                    .push(format!("convergence create failed: {e}"));
            }
        }
    }

    /// Scan for co-located attachments (images, PDFs, audio).
    async fn scan_attachments(
        &self,
        _db: &dyn HelixDb,
        _step_id: &str,
        path: &Path,
        report: &mut IngestionReport,
    ) {
        let Some(parent) = path.parent() else {
            return;
        };
        // Check for co-located non-markdown files
        let Ok(mut entries) = tokio::fs::read_dir(parent).await else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let entry_path = entry.path();
            if entry_path == path || is_markdown(&entry_path) {
                continue;
            }
            if is_attachment(&entry_path) {
                // Attachment node creation deferred — tracked in report
                report.records_added += 1;
            }
        }
    }
}

#[async_trait]
impl IngestionSource for MarkdownVaultIngester {
    fn name(&self) -> &'static str {
        "MarkdownVault"
    }

    #[instrument(skip(self, db))]
    async fn ingest(&self, db: &dyn HelixDb) -> Result<IngestionReport, IngestionError> {
        let sibling_dir = self.sibling_dir();
        if !sibling_dir.exists() {
            return Err(IngestionError::SourceNotFound(
                sibling_dir.display().to_string(),
            ));
        }

        let helix_id = db
            .ensure_helix(&self.sibling, &self.sibling, HelixOrderingMode::Temporal)
            .await
            .map_err(|e| IngestionError::Parse(format!("ensure_helix failed: {e}")))?;

        let mut report = IngestionReport::default();
        Box::pin(self.walk_recursive(&sibling_dir, db, &helix_id, &mut report)).await?;

        Ok(report)
    }
}

impl MarkdownVaultIngester {
    /// Recursively walk a directory tree and ingest all markdown files.
    async fn walk_recursive(
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
                continue;
            }

            if file_type.is_dir() {
                // Skip hidden directories
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with('.') {
                    continue;
                }
                Box::pin(self.walk_recursive(&path, db, helix_id, report)).await?;
            } else if file_type.is_file() && is_markdown(&path) {
                // Honour max_entries cap: stop once added+skipped reaches the limit.
                if self.max_entries.is_some_and(|max| {
                    report.records_added.saturating_add(report.records_skipped)
                        >= u64::try_from(max).unwrap_or(u64::MAX)
                }) {
                    return Ok(());
                }
                if let Err(e) = self.ingest_file(&path, db, helix_id, report).await {
                    report.errors.push(format!("{}: {e}", path.display()));
                }
            }
        }
        Ok(())
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext == "md" || ext == "markdown")
}

fn is_attachment(path: &Path) -> bool {
    let Some(ext) = path.extension() else {
        return false;
    };
    let ext = ext.to_string_lossy().to_lowercase();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "pdf" | "mp3" | "wav" | "ogg" | "mp4"
    )
}

fn parse_link_type(s: Option<&str>) -> LinkType {
    match s {
        Some("wikilink") => LinkType::Wikilink,
        Some("dependency") => LinkType::Dependency,
        Some("inspired_by") => LinkType::InspiredBy,
        Some("contradicts") => LinkType::Contradicts,
        Some("extends") => LinkType::Extends,
        Some("converges") => LinkType::Converges,
        _ => LinkType::Reference,
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
    fn test_is_markdown() {
        assert!(is_markdown(Path::new("entry.md")));
        assert!(is_markdown(Path::new("entry.markdown")));
        assert!(!is_markdown(Path::new("entry.txt")));
        assert!(!is_markdown(Path::new("entry.rs")));
    }

    #[test]
    fn test_is_attachment() {
        assert!(is_attachment(Path::new("photo.png")));
        assert!(is_attachment(Path::new("doc.pdf")));
        assert!(is_attachment(Path::new("voice.mp3")));
        assert!(!is_attachment(Path::new("code.rs")));
        assert!(!is_attachment(Path::new("readme.md")));
    }

    #[test]
    fn test_parse_link_type() {
        assert_eq!(parse_link_type(Some("wikilink")), LinkType::Wikilink);
        assert_eq!(parse_link_type(Some("reference")), LinkType::Reference);
        assert_eq!(parse_link_type(Some("dependency")), LinkType::Dependency);
        assert_eq!(parse_link_type(None), LinkType::Reference);
        assert_eq!(parse_link_type(Some("unknown")), LinkType::Reference);
    }

    #[test]
    fn test_new_ingestor() {
        let ing = MarkdownVaultIngester::new("/home/user/.soul/helix", "eva");
        assert_eq!(ing.sibling, "eva");
        assert_eq!(
            ing.sibling_dir(),
            PathBuf::from("/home/user/.soul/helix/eva")
        );
    }
}
