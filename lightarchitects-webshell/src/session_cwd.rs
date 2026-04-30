//! Derive the canonical cwd for a given Claude Code session UUID by
//! inspecting the on-disk session JSONL.
//!
//! Claude Code hashes the working directory into the per-project folder
//! name (`~/.claude/projects/<slash-replaced-by-hyphens>/`), and
//! `claude --resume <uuid>` only finds a session if the spawning
//! subprocess's cwd hashes to the SAME project folder. Callers of
//! the webshell can't reliably know the original cwd — the `/webshell`
//! slash command typically passes the user's current shell cwd, which
//! is usually NOT where the session was originally created.
//!
//! This module solves the mismatch by reading the session file's
//! `cwd` field (present in `attachment` records from line ~3 onward)
//! and returning the ground-truth cwd. The caller can then spawn
//! claude subprocesses from that exact directory.

use std::{
    fs::{File, read_dir},
    io::{BufRead, BufReader},
    path::PathBuf,
};

use serde_json::Value;

/// Return `true` when `id` is safe to use as a filename component.
///
/// Session IDs are UUIDs (hex digits + hyphens, ≤36 chars).  Rejects any
/// input containing path separators, null bytes, newlines, or non-ASCII
/// to prevent path traversal via a crafted session ID (HIGH H-89).
fn is_safe_session_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Walk `~/.claude/projects/*/<session_id>.jsonl` and read the first
/// `cwd` field we encounter. Returns `None` if the session file can't be
/// found or if no record contains a `cwd` field (fresh session, never
/// written to).
///
/// The first 1–2 records of a Claude session JSONL are typically
/// `custom-title` type with no cwd; `cwd` appears starting around the
/// `attachment` records. We cap reading at 50 lines to stay cheap.
#[must_use]
pub fn derive_cwd_for_claude_session(session_id: &str) -> Option<PathBuf> {
    if !is_safe_session_id(session_id) {
        tracing::warn!(session_id = %session_id, "derive_cwd: rejected unsafe session_id");
        return None;
    }
    let home = std::env::var_os("HOME")?;
    let projects_root = PathBuf::from(home).join(".claude").join("projects");
    let filename = format!("{session_id}.jsonl");

    let entries = read_dir(&projects_root).ok()?;
    for entry in entries.flatten() {
        if !entry.file_type().ok()?.is_dir() {
            continue;
        }
        let candidate = entry.path().join(&filename);
        if !candidate.is_file() {
            continue;
        }
        if let Some(cwd) = read_cwd_from_jsonl(&candidate) {
            return Some(cwd);
        }
    }
    None
}

/// Open a JSONL file and return the first `cwd` value we see across the
/// first 50 lines. Returns `None` if the file can't be read, no line
/// parses as JSON with a `cwd` field, or the field isn't a string.
fn read_cwd_from_jsonl(path: &std::path::Path) -> Option<PathBuf> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines().take(50) {
        let line = line.ok()?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if let Some(cwd) = obj.get("cwd").and_then(Value::as_str) {
            if !cwd.is_empty() {
                return Some(PathBuf::from(cwd));
            }
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn read_cwd_handles_empty_file() {
        let tmp = std::env::temp_dir().join("session-cwd-empty.jsonl");
        File::create(&tmp).unwrap();
        assert!(read_cwd_from_jsonl(&tmp).is_none());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn read_cwd_returns_first_cwd_seen() {
        let tmp = std::env::temp_dir().join("session-cwd-first.jsonl");
        let mut f = File::create(&tmp).unwrap();
        writeln!(f, r#"{{"type":"custom-title","customTitle":"x"}}"#).unwrap();
        writeln!(f, r#"{{"type":"attachment","cwd":"/Users/kft/Projects"}}"#).unwrap();
        writeln!(f, r#"{{"type":"attachment","cwd":"/other/path"}}"#).unwrap();
        drop(f);
        let derived = read_cwd_from_jsonl(&tmp).unwrap();
        assert_eq!(derived, PathBuf::from("/Users/kft/Projects"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn read_cwd_ignores_malformed_lines_until_finding_valid_one() {
        let tmp = std::env::temp_dir().join("session-cwd-mixed.jsonl");
        let mut f = File::create(&tmp).unwrap();
        writeln!(f, "not json").unwrap();
        writeln!(f, "{{").unwrap();
        writeln!(f, r#"{{"cwd":"/tmp/session"}}"#).unwrap();
        drop(f);
        let derived = read_cwd_from_jsonl(&tmp).unwrap();
        assert_eq!(derived, PathBuf::from("/tmp/session"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn derive_returns_none_for_nonexistent_session() {
        let result = derive_cwd_for_claude_session("00000000-0000-0000-0000-000000000000");
        assert!(result.is_none());
    }

    #[test]
    fn is_safe_session_id_accepts_valid_uuid() {
        assert!(is_safe_session_id("9a8b7c6d-1234-5678-abcd-ef0123456789"));
    }

    #[test]
    fn is_safe_session_id_rejects_path_traversal() {
        assert!(!is_safe_session_id("../../../etc/passwd"));
    }

    #[test]
    fn is_safe_session_id_rejects_newline() {
        assert!(!is_safe_session_id("abc\ndef"));
    }

    #[test]
    fn is_safe_session_id_rejects_null_byte() {
        assert!(!is_safe_session_id("abc\0def"));
    }

    #[test]
    fn is_safe_session_id_rejects_empty() {
        assert!(!is_safe_session_id(""));
    }

    #[test]
    fn is_safe_session_id_rejects_overlong() {
        let long = "a".repeat(37);
        assert!(!is_safe_session_id(&long));
    }

    #[test]
    fn derive_rejects_traversal_session_id() {
        let result = derive_cwd_for_claude_session("../../../etc/passwd");
        assert!(result.is_none());
    }
}
