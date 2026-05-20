//! SOUL vault grounding for the copilot prompt prelude.
//!
//! Queries the local `SQLite` FTS5 index for top-5 BM25-ranked entries
//! relevant to the current request, then formats them into a
//! `[Knowledge]` block for injection.  Each block is wrapped in
//! nonce-prefixed structural delimiters to bound indirect prompt
//! injection per OWASP LLM02 (SCR13 — vault delimiter hardening).

use std::fmt::Write as _;

use crate::memory::persistence::SoulPersistence;

/// A single vault entry extracted for grounding.
#[derive(Debug, Clone)]
pub struct GroundingEntry {
    /// Display title (from `StorageEntry.title` or path-derived fallback).
    pub title: String,
    /// First 200 bytes of the entry body (truncated, UTF-8 safe).
    pub excerpt: String,
}

const EXCERPT_BYTES: usize = 200;

/// Query the SOUL `SQLite` FTS5 index for top-5 entries matching `query`.
///
/// Returns an empty `Vec` when `soul` has no `SQLite` backend, when the
/// query returns no results, or on storage error.  The caller must
/// wrap this in a `tokio::time::timeout` for the 400 ms deadline.
pub async fn search(soul: &SoulPersistence, query: &str) -> Vec<GroundingEntry> {
    let Some(result) = soul.search_sqlite(query, 5).await else {
        return Vec::new();
    };
    match result {
        Err(e) => {
            tracing::debug!(error = %e, "soul_grounding: sqlite search error — skipping block");
            Vec::new()
        }
        Ok(entries) => entries
            .into_iter()
            .map(|e| {
                let title = e
                    .title
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| path_tail(&e.path));
                let excerpt = truncate_to_bytes(&e.content, EXCERPT_BYTES);
                GroundingEntry { title, excerpt }
            })
            .collect(),
    }
}

/// Format a slice of grounding entries into a `[Knowledge]` block string.
///
/// Each block is wrapped in a per-request nonce to prevent adversarial
/// vault content from forging block boundaries (OWASP LLM02 SCR13).
/// The nonce is 8 hex chars generated from the first 4 bytes of a
/// random UUID — cheap, non-cryptographic, sufficient for structural
/// disambiguation.
pub fn format_block(nonce: &str, entries: &[GroundingEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(512);
    let _ = writeln!(out, "[VAULT-DATA::{nonce}]");
    for entry in entries {
        out.push_str("- ");
        out.push_str(&entry.title);
        out.push_str(": ");
        out.push_str(&entry.excerpt);
        if !entry.excerpt.ends_with('\n') {
            out.push('\n');
        }
    }
    let _ = writeln!(out, "[/VAULT-DATA::{nonce}]");
    out
}

/// Generate an 8-char hex nonce for vault delimiter hardening.
pub fn vault_nonce() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let mut h = DefaultHasher::new();
    SystemTime::now().hash(&mut h);
    std::thread::current().id().hash(&mut h);
    format!("{:08x}", h.finish() & 0xffff_ffff)
}

/// Extract the last path component without extension as a fallback title.
fn path_tail(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_owned()
}

/// Truncate `s` at `max_bytes` on a UTF-8 character boundary.
fn truncate_to_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_owned();
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_block_empty_returns_empty() {
        assert!(format_block("abc12345", &[]).is_empty());
    }

    #[test]
    fn format_block_wraps_with_nonce() {
        let entries = vec![GroundingEntry {
            title: "Gate failure".to_owned(),
            excerpt: "QUAL gate failed on clippy warning".to_owned(),
        }];
        let out = format_block("a1b2c3d4", &entries);
        assert!(out.starts_with("[VAULT-DATA::a1b2c3d4]"));
        assert!(out.contains("[/VAULT-DATA::a1b2c3d4]"));
        assert!(out.contains("Gate failure: QUAL gate failed"));
    }

    #[test]
    fn format_block_truncates_at_200_chars() {
        let long_content = "x".repeat(300);
        let entry = GroundingEntry {
            title: "Big".to_owned(),
            excerpt: truncate_to_bytes(&long_content, EXCERPT_BYTES),
        };
        let out = format_block("deadbeef", &[entry]);
        // excerpt itself is 200 bytes + "…" — not the raw 300-char string
        assert!(!out.contains(&"x".repeat(201)));
    }

    #[test]
    fn truncate_to_bytes_exact_boundary() {
        let s = "hello world";
        // When truncated, ellipsis is appended to signal the cut.
        assert_eq!(truncate_to_bytes(s, 5), "hello\u{2026}");
        // When content fits within the limit, returned verbatim (no ellipsis).
        assert_eq!(truncate_to_bytes(s, 20), "hello world");
    }

    #[test]
    fn path_tail_extracts_last_segment() {
        assert_eq!(path_tail("helix/eva/entries/genesis.md"), "genesis.md");
        assert_eq!(path_tail("noslash"), "noslash");
    }

    #[test]
    fn vault_nonce_is_8_hex_chars() {
        let n = vault_nonce();
        assert_eq!(n.len(), 8);
        assert!(n.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
