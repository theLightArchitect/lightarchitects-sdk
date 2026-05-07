//! Markdown vault ingestor — recursively walks `{soul_home}/helix/{sibling}/` for all `.md` files.
//!
//! Parses YAML frontmatter, extracts wikilinks, assigns strands,
//! creates shared experiences from convergence references, and
//! discovers co-located attachments.
//!
//! # Multi-root scanning
//!
//! Use [`MarkdownVaultIngester::with_extra_roots`] to scan additional root directories
//! beyond the default sibling directory. Each extra root declares its [`ScopeTier`]
//! explicitly. Symlinks are skipped entirely during recursive walk; inode-based dedup
//! prevents duplicate scanning of top-level roots only.
//!
//! # Recursion-termination invariant
//!
//! The ingester satisfies two independent termination conditions: (1) top-level roots
//! are deduplicated by `(device_id, inode)` tuple so symlink cycles and hard-link
//! aliases are never double-scanned; (2) the filesystem helix-root search is bounded
//! by `MAX_FS_HELIX_DEPTH = 7` in [`crate::helix::helix_toml`]. Full specification:
//! `helix/user/standards/recursion-termination-invariant.md` in the SOUL vault.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use tracing::instrument;

use crate::helix::db::{HelixDb, HelixDbError};
use crate::helix::types::{
    DiscoveryMethod, HelixLink, HelixOrderingMode, LinkType, ScopeTier, SharedExperience, Step,
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
///
/// # Multi-root mode
///
/// Call [`Self::with_extra_roots`] to scan additional root directories with
/// explicit scope tiers. Inode-based deduplication prevents cycles from symlinks.
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
    /// Optional additional roots beyond the default sibling dir.
    /// Each entry is (path, `scope_tier`). Used for platform bundle scanning.
    roots: Vec<(PathBuf, ScopeTier)>,
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
            roots: Vec::new(),
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

    /// Add extra root directories to scan beyond the default sibling dir.
    ///
    /// Each root declares its scope tier explicitly (overrides `helix.toml` lookup).
    /// Roots are processed after the default sibling directory.
    /// Inode-based deduplication prevents visiting the same directory twice
    /// even when symlinks create directory aliases.
    #[must_use]
    pub fn with_extra_roots(mut self, roots: Vec<(PathBuf, ScopeTier)>) -> Self {
        self.roots = roots;
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
                // Re-run wikilink resolution + typed output edges — both are
                // idempotent (MERGE) so re-invocation is safe on skipped Steps.
                // Strand/convergence/attachment writes stay untouched.
                self.create_wikilinks(db, &step_id, body, &fm.links, report)
                    .await;
                self.create_typed_output_edges(db, &step_id, &fm, report)
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
        self.create_typed_output_edges(db, &step_id, &fm, report)
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
    ///
    /// Phase 11.5 follow-up (AYIN): the function is span-instrumented and
    /// classifies `create_link` failures so telemetry can quantify
    /// resolution success without scanning the `errors` vec:
    ///
    /// * `Ok(_)` → [`IngestionReport::wikilinks_resolved`] += 1
    /// * [`HelixDbError::NotFound`] → [`IngestionReport::wikilinks_unresolved`] += 1
    ///   (benign — target not ingested yet, or typo)
    /// * any other `Err` → pushed to [`IngestionReport::errors`] (real failure)
    #[instrument(
        skip(self, db, body, fm_links, report),
        fields(
            step_id = %step_id,
            inline_wikilinks = wikilink::extract(body).len(),
            fm_links = fm_links.len(),
        )
    )]
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
            record_link_outcome(
                db.create_link(&link).await,
                LinkSource::Inline,
                step_id,
                &wl.target,
                report,
            );
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
            record_link_outcome(
                db.create_link(&link).await,
                LinkSource::Frontmatter,
                step_id,
                &lr.target,
                report,
            );
        }
    }

    /// Phase 14.2 — materialise typed sibling-output edges from front-matter.
    ///
    /// Maps vault front-matter fields to Neo4j typed relationships:
    ///
    /// | Entry kind                    | Front-matter field   | Edge type             |
    /// |-------------------------------|----------------------|-----------------------|
    /// | review / scrum-assessment     | `plan_ids: [..]`     | `REVIEWS_PLAN`        |
    /// | lesson                        | `source_entry_id`    | `LESSON_FROM_ENTRY`   |
    /// | plan                          | `build_id`           | `PLAN_FOR_BUILD`      |
    ///
    /// Targets are resolved via [`HelixDb::create_typed_relationship`]'s
    /// two-stage lookup (UUID → `vault_path` suffix), so callers can use
    /// slugs like `unified-forging-vault` that resolve once a corresponding
    /// plan Step is ingested. Unresolved targets are logged but not fatal —
    /// `MERGE (a)-[r]->(b) WHERE b IS NOT NULL` is a silent no-op when the
    /// target doesn't exist yet.
    async fn create_typed_output_edges(
        &self,
        db: &dyn HelixDb,
        step_id: &str,
        fm: &frontmatter::Frontmatter,
        report: &mut IngestionReport,
    ) {
        let kind = fm.entry_type.as_deref().unwrap_or("");

        // REVIEWS_PLAN — scrum assessments + reviews point at their reviewed plans.
        if matches!(kind, "review" | "scrum-assessment" | "scrum") {
            if let Some(plan_ids) = fm.extra.get("plan_ids").and_then(|v| v.as_array()) {
                for pid in plan_ids.iter().filter_map(|v| v.as_str()) {
                    if let Err(e) = db
                        .create_typed_relationship(step_id, pid, "REVIEWS_PLAN")
                        .await
                    {
                        report
                            .errors
                            .push(format!("REVIEWS_PLAN failed ({pid}): {e}"));
                    }
                }
            }
        }

        // LESSON_FROM_ENTRY — lessons carry a pointer to the source entry
        // they were extracted from. `source_entry_id` is the target UUID or
        // vault-path slug.
        if kind == "lesson" {
            if let Some(src) = fm.extra.get("source_entry_id").and_then(|v| v.as_str()) {
                if let Err(e) = db
                    .create_typed_relationship(step_id, src, "LESSON_FROM_ENTRY")
                    .await
                {
                    report
                        .errors
                        .push(format!("LESSON_FROM_ENTRY failed ({src}): {e}"));
                }
            }
        }

        // PLAN_FOR_BUILD — build plans reference the build they plan for.
        if kind == "plan" {
            if let Some(build_id) = fm.extra.get("build_id").and_then(|v| v.as_str()) {
                if let Err(e) = db
                    .create_typed_relationship(step_id, build_id, "PLAN_FOR_BUILD")
                    .await
                {
                    report
                        .errors
                        .push(format!("PLAN_FOR_BUILD failed ({build_id}): {e}"));
                }
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

    #[instrument(skip(self, db), fields(sibling = %self.sibling))]
    async fn ingest(&self, db: &dyn HelixDb) -> Result<IngestionReport, IngestionError> {
        let sibling_dir = self.sibling_dir();
        if !sibling_dir.exists() {
            return Err(IngestionError::SourceNotFound(
                sibling_dir.display().to_string(),
            ));
        }

        // Primary sibling root is always User-tier; extra roots carry their own tier
        // via `with_extra_roots` and are processed by `process_extra_root`.
        let helix_id = db
            .ensure_helix(
                &self.sibling,
                &self.sibling,
                HelixOrderingMode::Temporal,
                ScopeTier::User,
            )
            .await
            .map_err(|e| IngestionError::Parse(format!("ensure_helix failed: {e}")))?;

        let mut report = IngestionReport::default();

        // Track visited inodes (device_id, inode) to prevent cycles via symlinks.
        let mut visited: HashSet<(u64, u64)> = HashSet::new();
        if let Some(key) = inode_key(&sibling_dir) {
            visited.insert(key);
        }

        Box::pin(self.walk_recursive(&sibling_dir, db, &helix_id, &mut report)).await?;

        // Process each extra root with its declared scope tier.
        for (root, scope_tier) in &self.roots {
            // Skip roots that have already been visited (inode dedup).
            let key = inode_key(root);
            if let Some(k) = key {
                if visited.contains(&k) {
                    continue;
                }
                visited.insert(k);
            }
            self.process_extra_root(root, *scope_tier, db, &mut report)
                .await?;
        }

        Ok(report)
    }
}

impl MarkdownVaultIngester {
    /// Process a single extra root: ensure its helix node and walk it.
    ///
    /// Called from [`Self::ingest`] after the inode-dedup check, so the
    /// visited-set management stays co-located with the loop.
    /// Returns `Ok(())` (with an error pushed to `report`) when the root
    /// directory does not exist. All other error paths (`ensure_helix`,
    /// `walk_recursive`) propagate as [`IngestionError`] and abort the
    /// enclosing `ingest()` call.
    #[instrument(skip(self, db, report), fields(root = %root.display()))]
    async fn process_extra_root(
        &self,
        root: &Path,
        scope_tier: ScopeTier,
        db: &dyn HelixDb,
        report: &mut IngestionReport,
    ) -> Result<(), IngestionError> {
        if !root.exists() {
            report
                .errors
                .push(format!("extra root not found: {}", root.display()));
            return Ok(());
        }

        let root_name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("extra_root");

        let extra_helix_id = db
            .ensure_helix(
                root_name,
                root_name,
                HelixOrderingMode::Temporal,
                scope_tier,
            )
            .await
            .map_err(|e| {
                IngestionError::Parse(format!("ensure_helix failed for {}: {e}", root.display()))
            })?;

        Box::pin(self.walk_recursive(root, db, &extra_helix_id, report)).await
    }

    /// Recursively walk a directory tree and ingest all markdown files.
    #[instrument(skip(self, db, report), fields(dir = %dir.display(), helix_id))]
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

/// Return a platform-specific `(device_id, inode)` key for `path`, or `None` if
/// metadata cannot be read (permission error, non-existent path, etc.).
///
/// Used to build a visited set in [`MarkdownVaultIngester::ingest`] so that
/// symlinked directories are never traversed twice.
fn inode_key(path: &Path) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // Follow symlinks so two aliases to the same directory produce the same
        // (dev, ino) pair and dedup fires correctly.
        let meta = std::fs::metadata(path).ok()?;
        Some((meta.dev(), meta.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let meta = std::fs::metadata(path).ok()?;
        // Windows: volume_serial_number + file_index give a stable identity.
        let serial = u64::from(meta.volume_serial_number().unwrap_or(0));
        let index = meta.file_index().unwrap_or(0);
        Some((serial, index))
    }
    #[cfg(not(any(unix, windows)))]
    {
        // Fallback: no inode support; return None (no dedup on exotic targets).
        let _ = path;
        None
    }
}

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

/// Where a link came from — shapes the telemetry warning emitted on
/// unresolved targets so logs differentiate inline `[[slug]]` references
/// from declared `links:` front-matter entries.
#[derive(Debug, Clone, Copy)]
enum LinkSource {
    /// Inline `[[target]]` wikilink parsed from the markdown body.
    Inline,
    /// Entry in the front-matter `links:` list.
    Frontmatter,
}

/// Phase 11.5 follow-up (AYIN) — classify a `create_link` result and update
/// the ingestion report accordingly.
///
/// Split out as a pure function so it can be unit-tested without
/// implementing the full `HelixDb` trait. See `mod tests` below.
fn record_link_outcome(
    result: Result<String, HelixDbError>,
    source: LinkSource,
    step_id: &str,
    target: &str,
    report: &mut IngestionReport,
) {
    match result {
        Ok(_) => {
            report.wikilinks_resolved = report.wikilinks_resolved.saturating_add(1);
        }
        Err(HelixDbError::NotFound(_)) => {
            let kind = match source {
                LinkSource::Inline => "inline",
                LinkSource::Frontmatter => "frontmatter",
            };
            tracing::warn!(
                target: "helix.wikilink",
                source_id = %step_id,
                target_slug = %target,
                source_kind = kind,
                "link target not found — left unresolved"
            );
            report.wikilinks_unresolved = report.wikilinks_unresolved.saturating_add(1);
        }
        Err(e) => {
            let kind = match source {
                LinkSource::Inline => "wikilink",
                LinkSource::Frontmatter => "fm link",
            };
            report
                .errors
                .push(format!("{kind} create failed ({target}): {e}"));
        }
    }
}

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
        // No extension — early-return false branch.
        assert!(!is_attachment(Path::new("no_extension")));
    }

    #[test]
    fn test_parse_link_type() {
        assert_eq!(parse_link_type(Some("wikilink")), LinkType::Wikilink);
        assert_eq!(parse_link_type(Some("reference")), LinkType::Reference);
        assert_eq!(parse_link_type(Some("dependency")), LinkType::Dependency);
        assert_eq!(parse_link_type(None), LinkType::Reference);
        assert_eq!(parse_link_type(Some("unknown")), LinkType::Reference);
        // Cover all remaining named arms.
        assert_eq!(parse_link_type(Some("inspired_by")), LinkType::InspiredBy);
        assert_eq!(parse_link_type(Some("contradicts")), LinkType::Contradicts);
        assert_eq!(parse_link_type(Some("extends")), LinkType::Extends);
        assert_eq!(parse_link_type(Some("converges")), LinkType::Converges);
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

    // ── Phase 11.5 follow-up — wikilink outcome classifier tests ─────────

    #[test]
    fn record_link_outcome_resolved_increments_resolved() {
        let mut report = IngestionReport::default();
        record_link_outcome(
            Ok("rel-uuid".to_owned()),
            LinkSource::Inline,
            "src-1",
            "eva/identity",
            &mut report,
        );
        assert_eq!(report.wikilinks_resolved, 1);
        assert_eq!(report.wikilinks_unresolved, 0);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn record_link_outcome_not_found_is_benign_unresolved() {
        // NotFound is the "target doesn't exist yet" / "typo in slug" path —
        // it MUST bump `wikilinks_unresolved` and NOT push to `errors`,
        // otherwise every forward reference in the vault looks like a bug.
        let mut report = IngestionReport::default();
        record_link_outcome(
            Err(HelixDbError::NotFound(
                "no matching target for 'eva/missing'".to_owned(),
            )),
            LinkSource::Inline,
            "src-1",
            "eva/missing",
            &mut report,
        );
        assert_eq!(report.wikilinks_unresolved, 1);
        assert_eq!(report.wikilinks_resolved, 0);
        assert!(
            report.errors.is_empty(),
            "NotFound must not fall through to errors vec"
        );
    }

    #[test]
    fn record_link_outcome_other_error_pushes_to_errors() {
        // Genuine failures (connection error, validation, etc.) still land
        // in the errors vec so they surface in the IngestionReport summary.
        let mut report = IngestionReport::default();
        record_link_outcome(
            Err(HelixDbError::Validation("bad link spec".to_owned())),
            LinkSource::Frontmatter,
            "src-1",
            "target",
            &mut report,
        );
        assert_eq!(report.wikilinks_resolved, 0);
        assert_eq!(report.wikilinks_unresolved, 0);
        assert_eq!(report.errors.len(), 1, "Validation must surface as error");
        assert!(
            report.errors[0].contains("fm link create failed"),
            "frontmatter source labelled correctly: {:?}",
            report.errors
        );
    }

    #[test]
    fn record_link_outcome_source_kind_labels_errors_distinctly() {
        // Inline wikilink failures and frontmatter link failures are labelled
        // differently so operators can attribute bad slugs back to their
        // source in the vault (body prose vs explicit `links:`).
        let mut report = IngestionReport::default();
        record_link_outcome(
            Err(HelixDbError::Validation("x".into())),
            LinkSource::Inline,
            "s",
            "t",
            &mut report,
        );
        record_link_outcome(
            Err(HelixDbError::Validation("y".into())),
            LinkSource::Frontmatter,
            "s",
            "t",
            &mut report,
        );
        assert_eq!(report.errors.len(), 2);
        assert!(report.errors[0].contains("wikilink create failed"));
        assert!(report.errors[1].contains("fm link create failed"));
    }

    #[test]
    fn record_link_outcome_saturates_on_counter_overflow() {
        // Defence in depth: even at u64::MAX the counter stays well-defined.
        let mut report = IngestionReport {
            wikilinks_resolved: u64::MAX,
            ..Default::default()
        };
        record_link_outcome(Ok("rel".into()), LinkSource::Inline, "s", "t", &mut report);
        assert_eq!(
            report.wikilinks_resolved,
            u64::MAX,
            "saturating_add caps at max"
        );
    }
}
