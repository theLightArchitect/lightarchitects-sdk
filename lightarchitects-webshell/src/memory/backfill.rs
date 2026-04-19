//! Filesystem → `SOUL` `SQLite` backfill.
//!
//! Walks the helix vault (`~/lightarchitects/soul/helix/{sibling}/entries/*.md`)
//! and writes every entry into the `SQLite` `helix_entries` table. The `FTS5`
//! shadow is auto-maintained by triggers, so after backfill the `SOUL` `MCP`
//! plugin's BM25 search finds everything.
//!
//! # When this runs
//!
//! - **On startup** (fire-and-forget) when `helix_entries` is empty and the
//!   filesystem has ≥1 entry. Prevents a boot-stall on a fresh `SQLite` DB.
//! - **On demand** via `POST /api/soul/reindex` — forces a full rewalk,
//!   useful after bulk filesystem-only writes.
//!
//! # Why not `soul migrate`?
//!
//! The `SOUL` CLI's `migrate` subcommand targets `Neo4j` exclusively — it
//! populates the graph backend, not the `SQLite` row store. The row store is
//! normally kept in sync by the `SOUL` `MCP` plugin's `ingest` action when
//! Claude Code writes an entry. Filesystem-authored entries (e.g. manual
//! editor writes, or the historical pre-`MCP` vault population) never hit
//! `SQLite` without a routine like this one.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use lightarchitects::soul::sqlite::SqliteBackend;
use lightarchitects::soul::storage::{StorageBackend as _, StorageEntry, StorageError};
use tokio::io::AsyncReadExt;
use tracing::{info, warn};

use super::frontmatter;

/// How many entries to stage per transaction batch.
const BATCH_SIZE: usize = 100;

/// Result of a backfill run — reported on the `/api/soul/reindex` response
/// and in startup logs.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackfillReport {
    /// Files discovered on disk.
    pub scanned: usize,
    /// Files successfully projected into `StorageEntry`.
    pub parsed: usize,
    /// Rows written (upserted) to `SQLite`.
    pub written: usize,
    /// Per-sibling counts for parity verification.
    pub by_sibling: std::collections::BTreeMap<String, usize>,
}

/// Walk the helix root + backfill every `{sibling}/entries/*.md` into `SQLite`.
///
/// Never errors — individual file parse failures are logged at WARN and the
/// backfill continues with the remaining entries.
pub async fn run(helix_root: &Path, backend: &Arc<SqliteBackend>) -> BackfillReport {
    let mut report = BackfillReport {
        scanned: 0,
        parsed: 0,
        written: 0,
        by_sibling: std::collections::BTreeMap::new(),
    };

    let Ok(mut siblings) = tokio::fs::read_dir(helix_root).await else {
        warn!(target: "soul", root = %helix_root.display(), "backfill: helix root unreadable");
        return report;
    };

    let mut pending: Vec<StorageEntry> = Vec::with_capacity(BATCH_SIZE);

    while let Ok(Some(sibling_entry)) = siblings.next_entry().await {
        let sibling_path = sibling_entry.path();
        if !sibling_path.is_dir() {
            continue;
        }
        let Some(sibling_name) = sibling_path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if sibling_name.starts_with('.') || sibling_name.starts_with('_') {
            continue;
        }
        let entries_dir = sibling_path.join("entries");

        let Ok(mut files) = tokio::fs::read_dir(&entries_dir).await else {
            continue;
        };
        while let Ok(Some(file)) = files.next_entry().await {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            report.scanned = report.scanned.saturating_add(1);

            if let Some(entry) = project_file(&path, sibling_name).await {
                report.parsed = report.parsed.saturating_add(1);
                *report
                    .by_sibling
                    .entry(sibling_name.to_owned())
                    .or_insert(0) += 1;
                pending.push(entry);
            }

            if pending.len() >= BATCH_SIZE {
                flush(backend, &mut pending, &mut report).await;
            }
        }
    }

    // Flush any trailing partial batch.
    if !pending.is_empty() {
        flush(backend, &mut pending, &mut report).await;
    }

    info!(
        target: "soul",
        scanned = report.scanned,
        parsed = report.parsed,
        written = report.written,
        "backfill complete"
    );
    report
}

async fn flush(
    backend: &Arc<SqliteBackend>,
    pending: &mut Vec<StorageEntry>,
    report: &mut BackfillReport,
) {
    // Try the whole batch first (transactional — fast path).
    match backend.write_entries_batch(pending).await {
        Ok(n) => {
            report.written = report.written.saturating_add(n);
            pending.clear();
            return;
        }
        Err(e) => warn!(
            target: "soul",
            error = %e,
            count = pending.len(),
            "backfill batch write failed — retrying per-row"
        ),
    }

    // Per-row fallback: a single malformed entry shouldn't take out 99 others.
    for entry in pending.drain(..) {
        match backend.write_entry(&entry).await {
            Ok(()) => report.written = report.written.saturating_add(1),
            Err(e) => warn!(
                target: "soul",
                error = %e,
                path = %entry.path,
                "backfill single-row failure"
            ),
        }
    }
}

async fn project_file(abs_path: &Path, sibling: &str) -> Option<StorageEntry> {
    let mut file = tokio::fs::File::open(abs_path).await.ok()?;
    let mut raw = String::new();
    file.read_to_string(&mut raw).await.ok()?;
    let (fields, excerpt) = frontmatter::parse(&raw);

    // Path is stored relative to helix root as `{sibling}/entries/{file.md}`
    // to match the MCP plugin's path convention. Use the full rel_path as
    // the primary key too — file stems collide across siblings (e.g. the
    // same SCRUM note gets cross-posted to user/ and eva/).
    let rel_path = format!(
        "{sibling}/entries/{}",
        abs_path.file_name()?.to_string_lossy()
    );
    let id = rel_path.clone();

    // Significance: front-matter is 0–10; StorageEntry is 0–10 too.
    // Our FrontMatterFields normalises to 0–1 for the UI; reverse for DB.
    let significance = fields.significance.map_or(0.0, |s| f64::from(s) * 10.0);

    let frontmatter_raw = match fields.raw {
        serde_json::Value::Null => None,
        v => Some(v),
    };

    // Parse `self_defining: true` from front-matter if present.
    let self_defining = frontmatter_raw
        .as_ref()
        .and_then(|v| v.get("self_defining"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // entry_type from front-matter (falls back to "experience" — the common case).
    let entry_type = frontmatter_raw
        .as_ref()
        .and_then(|v| v.get("type").or_else(|| v.get("entry_type")))
        .and_then(|v| v.as_str())
        .map(str::to_owned);

    let epoch = frontmatter_raw
        .as_ref()
        .and_then(|v| v.get("epoch"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);

    let resonance = frontmatter_raw
        .as_ref()
        .and_then(|v| v.get("resonance"))
        .and_then(parse_string_list)
        .unwrap_or_default();
    let themes = frontmatter_raw
        .as_ref()
        .and_then(|v| v.get("themes"))
        .and_then(parse_string_list)
        .unwrap_or_default();

    let title = frontmatter_raw
        .as_ref()
        .and_then(|v| v.get("title"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);

    // Best-effort date parse from front-matter `date: YYYY-MM-DD`.
    let date = fields
        .created_at
        .as_deref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(&s[..10.min(s.len())], "%Y-%m-%d").ok());

    let now = Utc::now();
    Some(StorageEntry {
        id,
        path: rel_path,
        sibling: sibling.to_owned(),
        date,
        entry_type: entry_type.or_else(|| Some("experience".to_owned())),
        significance,
        self_defining,
        epoch,
        strands: fields.strands,
        resonance,
        themes,
        title,
        content: excerpt.unwrap_or_default(),
        frontmatter: frontmatter_raw,
        created_at: now,
        updated_at: now,
    })
}

/// Accept either a YAML list of strings or a single-string value and coerce
/// to `Vec<String>`. Tolerates YAML "strands: methodical" (no dash) and
/// legacy "themes: consciousness, trust" (comma-split string).
fn parse_string_list(v: &serde_json::Value) -> Option<Vec<String>> {
    if let Some(arr) = v.as_array() {
        return Some(
            arr.iter()
                .filter_map(|e| e.as_str().map(str::to_lowercase))
                .collect(),
        );
    }
    if let Some(s) = v.as_str() {
        return Some(
            s.split(',')
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .collect(),
        );
    }
    None
}

/// Check whether `SQLite` currently has fewer rows than the filesystem.
/// Used by the startup auto-trigger.
#[allow(clippy::missing_errors_doc)]
pub async fn sqlite_needs_backfill(backend: &Arc<SqliteBackend>, helix_root: &Path) -> bool {
    let Ok(sql_rows) = count_sqlite(backend).await else {
        return false;
    };
    let fs_rows = count_filesystem(helix_root).await;
    fs_rows > sql_rows
}

async fn count_sqlite(backend: &Arc<SqliteBackend>) -> Result<usize, StorageError> {
    use lightarchitects::soul::storage::EntryFilter;
    // Upper-bound a query; backend has no dedicated count() on the trait.
    let entries = backend.query(&EntryFilter::default()).await?;
    Ok(entries.len())
}

async fn count_filesystem(helix_root: &Path) -> usize {
    let Ok(mut siblings) = tokio::fs::read_dir(helix_root).await else {
        return 0;
    };
    let mut total = 0usize;
    while let Ok(Some(s)) = siblings.next_entry().await {
        let p = s.path();
        if !p.is_dir() {
            continue;
        }
        let entries_dir = p.join("entries");
        let Ok(mut files) = tokio::fs::read_dir(&entries_dir).await else {
            continue;
        };
        while let Ok(Some(f)) = files.next_entry().await {
            if f.path().extension().and_then(|e| e.to_str()) == Some("md") {
                total = total.saturating_add(1);
            }
        }
    }
    total
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;

    async fn write_entry(path: &Path, content: &str) {
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        let mut f = tokio::fs::File::create(path).await.unwrap();
        f.write_all(content.as_bytes()).await.unwrap();
        f.flush().await.unwrap();
    }

    #[tokio::test]
    async fn backfill_writes_entries_from_filesystem() {
        let tmp = tempdir().unwrap();
        let helix = tmp.path();
        write_entry(
            &helix.join("eva/entries/a.md"),
            "---\nsibling: eva\ndate: 2026-04-19\nsignificance: 8.0\nstrands:\n  - Methodical\nthemes:\n  - consciousness\n---\nFirst entry",
        )
        .await;
        write_entry(
            &helix.join("corso/entries/b.md"),
            "---\nsibling: corso\ndate: 2026-04-18\nsignificance: 7.0\nself_defining: true\n---\nSecond entry",
        )
        .await;

        let db_path = tmp.path().join("helix.db");
        let backend = Arc::new(SqliteBackend::open(&db_path).unwrap());

        let report = run(helix, &backend).await;
        assert_eq!(report.scanned, 2);
        assert_eq!(report.parsed, 2);
        assert_eq!(report.written, 2);
        assert_eq!(report.by_sibling.get("eva"), Some(&1));
        assert_eq!(report.by_sibling.get("corso"), Some(&1));

        // Verify via round-trip read.
        let eva = backend.read_entry("eva/entries/a.md").await.unwrap();
        assert_eq!(eva.sibling, "eva");
        assert!((eva.significance - 8.0).abs() < 1e-6);
        assert_eq!(eva.strands, vec!["methodical"]);
        assert_eq!(eva.themes, vec!["consciousness"]);

        let corso = backend.read_entry("corso/entries/b.md").await.unwrap();
        assert!(corso.self_defining);
    }

    #[tokio::test]
    async fn backfill_skips_non_md_and_dot_dirs() {
        let tmp = tempdir().unwrap();
        let helix = tmp.path();
        write_entry(
            &helix.join("eva/entries/ok.md"),
            "---\nsibling: eva\n---\nok",
        )
        .await;
        write_entry(&helix.join("eva/entries/skip.txt"), "not a helix entry").await;
        write_entry(
            &helix.join(".obsidian/entries/ignored.md"),
            "---\n---\nignore",
        )
        .await;

        let db_path = tmp.path().join("h.db");
        let backend = Arc::new(SqliteBackend::open(&db_path).unwrap());
        let report = run(helix, &backend).await;
        assert_eq!(report.scanned, 1);
        assert_eq!(report.parsed, 1);
    }

    #[tokio::test]
    async fn sqlite_needs_backfill_true_when_fs_has_more() {
        let tmp = tempdir().unwrap();
        let helix = tmp.path();
        write_entry(&helix.join("eva/entries/x.md"), "---\n---\nx").await;
        let db_path = tmp.path().join("h.db");
        let backend = Arc::new(SqliteBackend::open(&db_path).unwrap());
        assert!(sqlite_needs_backfill(&backend, helix).await);
        run(helix, &backend).await;
        assert!(!sqlite_needs_backfill(&backend, helix).await);
    }
}
