//! Cold memory reader — walks the helix filesystem tier.
//!
//! "Cold" lives at `~/lightarchitects/soul/helix/{sibling}/entries/*.md`.
//! Each entry is a YAML-front-matter-headed markdown file promoted from hot
//! turnlog storage by [`crate::memory::promoter_bridge::BroadcastingPromoter`]
//! (or, historically, by direct `MCP` invocation from a sibling).
//!
//! The reader supports two projections:
//! - [`snapshot_cold`] → `Vec<ContextMemo>` — list projection for the drawer
//! - [`read_entry`] → `(EnrichedEntry, raw_markdown)` — detail pane fetch

use std::path::{Path, PathBuf};

use lightarchitects::soul::storage::{EntryFilter, StorageEntry};
use tokio::io::AsyncReadExt;

use super::frontmatter;
use super::persistence::SoulPersistence;
use super::types::{ContextMemo, EnrichedEntry, MemoryTier};

/// Projection limit cap — prevents accidentally walking every helix entry.
const DEFAULT_LIMIT_CAP: usize = 500;

/// Snapshot the N most recent cold memos.
///
/// `sibling_filter` restricts to one sibling's `entries/` directory; `None`
/// walks every sibling. Results are sorted newest-first by `created_at`.
///
/// Returns empty if the helix root can't be resolved — never errors.
pub async fn snapshot_cold(
    helix_root: &Path,
    sibling_filter: Option<&str>,
    limit: usize,
) -> Vec<ContextMemo> {
    let limit = limit.min(DEFAULT_LIMIT_CAP);
    let siblings = match sibling_filter {
        Some(s) => vec![s.to_owned()],
        None => discover_siblings(helix_root).await,
    };

    let mut all: Vec<ContextMemo> = Vec::new();
    for sibling in siblings {
        let entries_dir = helix_root.join(&sibling).join("entries");
        walk_entries_dir(&entries_dir, &sibling, &mut all).await;
    }

    all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all.truncate(limit);
    all
}

/// Phase 10.2 — tier-preferenced snapshot with filesystem supplement.
///
/// Prefers `SQLite` when available (so entries ingested via the `SOUL` `MCP`
/// plugin show up immediately). When `SQLite` returns fewer rows than the
/// limit OR is unavailable, supplements with a filesystem walk and
/// deduplicates by `source_path`. This handles the real-world state where
/// `helix.db` hasn't been backfilled yet but `{sibling}/entries/*.md`
/// already holds hundreds of historical entries.
pub async fn snapshot_cold_via_soul(
    soul: &SoulPersistence,
    sibling_filter: Option<&str>,
    limit: usize,
) -> Vec<ContextMemo> {
    let limit = limit.min(DEFAULT_LIMIT_CAP);

    let mut sqlite_memos: Vec<ContextMemo> = Vec::new();
    if soul.has_sqlite() {
        let filter = EntryFilter {
            sibling: sibling_filter.map(str::to_owned),
            limit: Some(limit),
            ..EntryFilter::default()
        };
        if let Some(Ok(entries)) = soul.query_sqlite(&filter).await {
            sqlite_memos = entries.into_iter().map(storage_entry_to_memo).collect();
        }
    }

    // Fast path: SQLite met the limit on its own.
    if sqlite_memos.len() >= limit {
        sqlite_memos.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        sqlite_memos.truncate(limit);
        return sqlite_memos;
    }

    // Supplement with a filesystem walk — deduplicate by source_path.
    let mut fs_memos = snapshot_cold(soul.helix_root(), sibling_filter, limit).await;

    let sqlite_paths: std::collections::HashSet<String> = sqlite_memos
        .iter()
        .filter_map(|m| m.source_path.clone())
        .collect();
    fs_memos.retain(|m| match &m.source_path {
        Some(p) => !sqlite_paths.contains(p),
        None => true,
    });

    sqlite_memos.append(&mut fs_memos);
    sqlite_memos.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    sqlite_memos.truncate(limit);
    sqlite_memos
}

/// Convert a `StorageEntry` (from `SOUL` `SQLite`) into the UI's `ContextMemo`.
#[allow(clippy::cast_possible_truncation)]
fn storage_entry_to_memo(entry: StorageEntry) -> ContextMemo {
    let content = entry.excerpt(280);
    ContextMemo {
        id: entry.path.clone(),
        tier: MemoryTier::Cold,
        content,
        significance: (entry.significance / 10.0).clamp(0.0, 1.0) as f32,
        sibling: entry.sibling,
        strands: entry.strands,
        created_at: entry.created_at.to_rfc3339(),
        source_path: Some(entry.path),
        resonance: entry.resonance,
        themes: entry.themes,
        self_defining: entry.self_defining,
        entry_type: entry.entry_type,
    }
}

/// Read one helix entry by its vault-relative path.
///
/// Returns `Some((entry, raw_markdown))` on success, `None` if the file is
/// missing or outside the helix root (path-escape protection).
#[allow(clippy::missing_errors_doc)]
pub async fn read_entry(helix_root: &Path, rel_path: &str) -> Option<(EnrichedEntry, String)> {
    // Path-escape guard: resolve relative to root and ensure the result stays
    // within it. Rejects `..` components, symlinks pointing outside, etc.
    let abs = helix_root.join(rel_path);
    let canonical = tokio::fs::canonicalize(&abs).await.ok()?;
    let root_canonical = tokio::fs::canonicalize(helix_root).await.ok()?;
    if !canonical.starts_with(&root_canonical) {
        return None;
    }

    let mut file = tokio::fs::File::open(&canonical).await.ok()?;
    let mut raw = String::new();
    file.read_to_string(&mut raw).await.ok()?;

    let (fields, excerpt) = frontmatter::parse(&raw);

    let sibling_from_path = rel_path
        .split('/')
        .next()
        .map(str::to_owned)
        .unwrap_or_default();

    let entry = EnrichedEntry {
        path: rel_path.to_owned(),
        sibling: fields.sibling.clone().unwrap_or(sibling_from_path),
        significance: fields.significance,
        strands: fields.strands,
        content_excerpt: excerpt,
        created_at: fields.created_at,
        frontmatter_raw: fields.raw,
    };

    Some((entry, raw))
}

/// Enumerate top-level sibling directories under the helix root.
///
/// Skips hidden (`.git`, `.obsidian`) and underscored (`_STANDARD.md`) entries.
async fn discover_siblings(helix_root: &Path) -> Vec<String> {
    let Ok(mut rd) = tokio::fs::read_dir(helix_root).await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while let Ok(Some(entry)) = rd.next_entry().await {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if name.starts_with('.') || name.starts_with('_') {
                continue;
            }
            out.push(name.to_owned());
        }
    }
    out
}

/// Walk one sibling's entries directory and push projected memos into `out`.
async fn walk_entries_dir(dir: &Path, sibling: &str, out: &mut Vec<ContextMemo>) {
    let Ok(mut rd) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(memo) = project_file(&path, sibling).await {
            out.push(memo);
        }
    }
}

async fn project_file(path: &PathBuf, sibling_from_path: &str) -> Option<ContextMemo> {
    let mut file = tokio::fs::File::open(path).await.ok()?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).await.ok()?;
    let (fields, excerpt) = frontmatter::parse(&buf);

    let id = path.file_stem()?.to_string_lossy().into_owned();
    let content = excerpt.unwrap_or_default();

    // Derive helix-relative path from `{sibling}/entries/{stem}.md` shape —
    // avoids requiring a helix_root reference here.
    let source_path = Some(format!(
        "{sibling_from_path}/entries/{}",
        path.file_name()?.to_string_lossy()
    ));

    // Phase 13.1 — pull zettelkasten primitives from the same front-matter
    // the SQLite path reads. Each accessor returns defaults (empty / None /
    // false) on missing fields, keeping the filesystem fallback shape
    // identical to the SQLite projection.
    let raw = &fields.raw;
    let resonance = extract_string_list(raw, "resonance");
    let themes = extract_string_list(raw, "themes");
    let self_defining = raw
        .get("self_defining")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let entry_type = raw
        .get("type")
        .or_else(|| raw.get("entry_type"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);

    // Memo.sibling MUST match the directory segment, not the front-matter
    // `sibling:` field. Some entries (e.g. `laex0/entries/*.md` with front
    // matter `sibling: laex`) intentionally diverge for attribution reasons,
    // but the UI filters on directory name — so the projection must too.
    Some(ContextMemo {
        id: source_path.clone().unwrap_or(id),
        tier: MemoryTier::Cold,
        content,
        significance: fields.significance.unwrap_or(0.5),
        sibling: sibling_from_path.to_owned(),
        strands: fields.strands,
        created_at: fields
            .created_at
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned()),
        source_path,
        resonance,
        themes,
        self_defining,
        entry_type,
    })
}

/// Extract a list-typed front-matter field, tolerating either YAML array or
/// comma-separated string shape. Mirrors the parser in
/// [`crate::memory::backfill::parse_string_list`] so filesystem and `SQLite`
/// paths yield identical `Vec<String>` for the same YAML source.
fn extract_string_list(raw: &serde_json::Value, key: &str) -> Vec<String> {
    let Some(v) = raw.get(key) else {
        return Vec::new();
    };
    if let Some(arr) = v.as_array() {
        return arr
            .iter()
            .filter_map(|e| e.as_str().map(str::to_lowercase))
            .collect();
    }
    if let Some(s) = v.as_str() {
        return s
            .split(',')
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
    }
    Vec::new()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
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
    async fn snapshot_cold_walks_all_siblings() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        write_entry(
            &root.join("eva/entries/a.md"),
            "---\nsibling: eva\ndate: 2026-04-18\nsignificance: 7.0\n---\nEva entry",
        )
        .await;
        write_entry(
            &root.join("corso/entries/b.md"),
            "---\nsibling: corso\ndate: 2026-04-19\nsignificance: 8.5\n---\nCorso entry",
        )
        .await;

        let memos = snapshot_cold(root, None, 50).await;
        assert_eq!(memos.len(), 2);
        // Newest first: 2026-04-19 (corso) before 2026-04-18 (eva)
        assert_eq!(memos[0].sibling, "corso");
        assert_eq!(memos[1].sibling, "eva");
    }

    #[tokio::test]
    async fn snapshot_cold_filters_by_sibling() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_entry(&root.join("eva/entries/a.md"), "---\nsibling: eva\n---\nx").await;
        write_entry(
            &root.join("corso/entries/b.md"),
            "---\nsibling: corso\n---\nx",
        )
        .await;
        let memos = snapshot_cold(root, Some("eva"), 50).await;
        assert_eq!(memos.len(), 1);
        assert_eq!(memos[0].sibling, "eva");
    }

    #[tokio::test]
    async fn snapshot_cold_respects_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for i in 0..5 {
            write_entry(
                &root.join(format!("eva/entries/entry-{i}.md")),
                &format!(
                    "---\nsibling: eva\ndate: 2026-04-{:02}\n---\nentry {i}",
                    i + 10
                ),
            )
            .await;
        }
        let memos = snapshot_cold(root, None, 3).await;
        assert_eq!(memos.len(), 3);
    }

    #[tokio::test]
    async fn read_entry_returns_content_and_raw() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let raw = "---\nsibling: corso\nsignificance: 9.0\n---\nRaw body content";
        write_entry(&root.join("corso/entries/x.md"), raw).await;

        let (entry, got_raw) = read_entry(root, "corso/entries/x.md").await.unwrap();
        assert_eq!(entry.sibling, "corso");
        assert_eq!(entry.significance, Some(0.9));
        assert_eq!(got_raw, raw);
    }

    #[tokio::test]
    async fn read_entry_rejects_path_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let result = read_entry(tmp.path(), "../../../etc/passwd").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn discover_siblings_skips_dotfiles_and_underscore() {
        let tmp = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(tmp.path().join(".git"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(tmp.path().join("_hidden"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(tmp.path().join("eva"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(tmp.path().join("corso"))
            .await
            .unwrap();
        let mut siblings = discover_siblings(tmp.path()).await;
        siblings.sort();
        assert_eq!(siblings, vec!["corso", "eva"]);
    }
}
