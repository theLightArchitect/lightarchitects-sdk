//! Disk-quota enforcement and log rotation for Lightspace session logs.
//!
//! Caps total usage under `~/.lightarchitects/lightspace/` at
//! [`TOTAL_QUOTA_BYTES`] and per-session usage at [`SESSION_QUOTA_BYTES`].
//! When a quota is exceeded, the oldest session directory is removed first
//! (LRU eviction on `mtime`), then per-session eviction truncates the log
//! from the front.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use uuid::Uuid;

/// Total disk quota across all sessions: 100 MB.
pub const TOTAL_QUOTA_BYTES: u64 = 100 * 1024 * 1024;

/// Per-session quota: 10 MB.
pub const SESSION_QUOTA_BYTES: u64 = 10 * 1024 * 1024;

/// Enforce quotas under `root` (the `lightspace/` directory).
///
/// 1. Remove the oldest session directories until total usage ≤ [`TOTAL_QUOTA_BYTES`].
/// 2. For the target `session_id`, truncate its log if it exceeds [`SESSION_QUOTA_BYTES`].
///
/// Errors are non-fatal: logged as warnings, operation continues.
pub fn enforce(root: &Path, session_id: Uuid) {
    enforce_total(root);
    let session_dir = root.join(session_id.to_string());
    enforce_session(&session_dir);
}

fn enforce_total(root: &Path) {
    let total = dir_size(root);
    if total <= TOTAL_QUOTA_BYTES {
        return;
    }
    tracing::warn!(
        total_bytes = total,
        quota = TOTAL_QUOTA_BYTES,
        "lightspace total quota exceeded — evicting oldest sessions"
    );

    // Collect session directories with their mtime.
    let mut sessions: Vec<(PathBuf, SystemTime)> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((e.path(), mtime))
        })
        .collect();

    // Oldest first.
    sessions.sort_by_key(|(_, mtime)| *mtime);

    let mut remaining = total;
    for (dir, _) in sessions {
        if remaining <= TOTAL_QUOTA_BYTES {
            break;
        }
        let size = dir_size(&dir);
        if let Err(e) = fs::remove_dir_all(&dir) {
            tracing::warn!(path = %dir.display(), error = %e, "retention: failed to remove session dir");
        } else {
            tracing::info!(path = %dir.display(), freed = size, "retention: evicted session dir");
            remaining = remaining.saturating_sub(size);
        }
    }
}

fn enforce_session(session_dir: &Path) {
    let log_path = session_dir.join("events.ndjson");
    if !log_path.exists() {
        return;
    }
    let size = file_size(&log_path);
    if size <= SESSION_QUOTA_BYTES {
        return;
    }
    tracing::warn!(
        path = %log_path.display(),
        size,
        quota = SESSION_QUOTA_BYTES,
        "lightspace session quota exceeded — truncating log"
    );
    // Truncate by rewriting only the most-recent lines that fit.
    if let Err(e) = truncate_log(&log_path) {
        tracing::warn!(error = %e, "retention: truncate failed");
    }
}

/// Keep only the tail of the log that fits within [`SESSION_QUOTA_BYTES`].
fn truncate_log(path: &Path) -> std::io::Result<()> {
    use std::io::{BufRead, BufReader, Write};

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;

    // Walk backwards collecting lines until we hit the quota.
    let mut kept: Vec<&str> = Vec::new();
    let mut size: u64 = 0;
    for line in lines.iter().rev() {
        let len = line.len() as u64 + 1; // +1 for '\n'
        if size + len > SESSION_QUOTA_BYTES {
            break;
        }
        kept.push(line.as_str());
        size += len;
    }
    kept.reverse();

    let tmp = path.with_extension("ndjson.tmp");
    let mut out = fs::File::create(&tmp)?;
    for line in &kept {
        writeln!(out, "{line}")?;
    }
    drop(out);
    fs::rename(&tmp, path)?;
    Ok(())
}

fn dir_size(path: &Path) -> u64 {
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}
