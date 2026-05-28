//! SOUL vault grounding for the copilot prompt prelude.
//!
//! Two retrieval tiers:
//! - **4-signal RRF** via [`SoulClient`] (primary): BM25 + semantic HNSW +
//!   graph traversal + structural `Node2Vec`. Used when the SOUL MCP client
//!   is initialised in [`AppState`].
//! - **BM25 `SQLite`** via [`SoulPersistence`] (fallback): FTS5 fulltext only,
//!   400 ms timeout. Used when the MCP client is not yet ready.
//!
//! Each block is wrapped in nonce-prefixed structural delimiters to bound
//! indirect prompt injection per OWASP LLM02 (SCR13 — vault delimiter
//! hardening).
//!
//! **Write-back** — after each copilot turn the Q&A pair is persisted to
//! the SOUL vault via [`spawn_write_turn`].  SOUL's async write-through
//! indexes the entry in `SQLite` FTS5 and HNSW so future turns can retrieve
//! it via either tier.

use std::fmt::Write as _;
use std::sync::Arc;

use lightarchitects::{core::StdioTransport, soul::SoulClient};
use uuid::Uuid;

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

// ── 4-signal RRF retrieval ────────────────────────────────────────────────────

/// 4-signal RRF query via the SOUL MCP client.
///
/// Returns `(block, result_count, timed_out, elapsed_ms)`.
/// The block is either empty or a nonce-wrapped `[VAULT-DATA::]` section
/// ready for injection into the copilot prelude.
///
/// Timeout: 600 ms (higher than the BM25 path's 400 ms because the MCP
/// round-trip adds latency, but hybrid retrieval quality justifies the
/// extra budget).
pub async fn query_rrf(
    client: &SoulClient<StdioTransport>,
    build_id: Uuid,
    message: &str,
) -> (String, usize, bool, u64) {
    let t0 = std::time::Instant::now();
    let query = format!(
        "{} {}",
        build_id,
        message.chars().take(150).collect::<String>()
    );

    let outcome = tokio::time::timeout(
        std::time::Duration::from_millis(600),
        client.query(&query).top_k(5).token_budget(1500).call(),
    )
    .await;

    let ms = u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX);
    match outcome {
        Ok(Ok(r)) if !r.context.is_empty() => {
            let n = usize::try_from(r.total_found).unwrap_or(0);
            let nonce = vault_nonce();
            let block = format!(
                "[VAULT-DATA::{nonce}]\n{}\n[/VAULT-DATA::{nonce}]\n",
                r.context
            );
            (block, n, false, ms)
        }
        Ok(Ok(_)) => (String::new(), 0, false, ms),
        Ok(Err(e)) => {
            tracing::debug!(error = %e, "soul_grounding: rrf query error");
            (String::new(), 0, false, ms)
        }
        Err(_timeout) => (String::new(), 0, true, ms),
    }
}

// ── Turn write-back ───────────────────────────────────────────────────────────

/// Persist a completed copilot Q&A turn to the SOUL vault (fire-and-forget).
///
/// SOUL's async write-through indexes the entry in `SQLite` FTS5 and the HNSW
/// embedding store, making it retrievable by both [`search`] and [`query_rrf`]
/// on subsequent turns.  Errors are logged at `debug` level — the copilot
/// response has already been returned to the user, so write failures are
/// non-critical.
pub fn spawn_write_turn(
    client: Arc<SoulClient<StdioTransport>>,
    build_id: Uuid,
    question: String,
    answer: String,
) {
    tokio::spawn(async move {
        write_turn_inner(&client, build_id, &question, &answer).await;
    });
}

async fn write_turn_inner(
    client: &SoulClient<StdioTransport>,
    build_id: Uuid,
    question: &str,
    answer: &str,
) {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let path = format!(
        "claude/entries/{today}-copilot-{}-{}.md",
        &build_id.to_string()[..8],
        vault_nonce()
    );

    // Truncate to keep entries concise: 600-char question + 2000-char answer.
    let q_trunc = truncate_to_bytes(question, 600);
    let a_trunc = truncate_to_bytes(answer, 2000);
    let title_raw = question.chars().take(72).collect::<String>();

    let mut content = String::with_capacity(q_trunc.len() + a_trunc.len() + 256);
    let _ = writeln!(content, "---");
    let _ = writeln!(content, "title: \"Copilot — {title_raw}\"");
    let _ = writeln!(content, "significance: 5.5");
    let _ = writeln!(content, "step_date: {today}");
    let _ = writeln!(content, "tags: [copilot, qa, webshell]");
    let _ = writeln!(content, "---");
    let _ = writeln!(content);
    let _ = writeln!(content, "## Question\n{q_trunc}");
    let _ = writeln!(content);
    let _ = writeln!(content, "## Answer\n{a_trunc}");

    match client.write_note(&path, &content).await {
        Ok(_) => tracing::debug!(path, "soul_grounding: turn persisted to vault"),
        Err(e) => tracing::debug!(error = %e, path, "soul_grounding: write_turn failed"),
    }
}

// ── Vault nonce ───────────────────────────────────────────────────────────────

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
