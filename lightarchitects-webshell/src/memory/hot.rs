//! Hot memory projection — reads active turnlog sessions and yields `ContextMemo`s.
//!
//! "Hot" is defined as entries in any session file currently under
//! `{turnlog_root}/active/*.ndjson`. These are either live sessions (writer
//! still running) or abandoned sessions (process crashed). Both count as hot
//! because neither has reached the promotion pipeline yet.

use lightarchitects::turnlog::entry::TurnEntry;
use lightarchitects::turnlog::reader::TurnLogReader;
use lightarchitects::turnlog::store::StoreLayout;

use super::types::{ContextMemo, MemoryTier};

/// Maximum characters kept from a span's textual payload when projecting to `content`.
const CONTENT_MAX_CHARS: usize = 200;

/// Snapshot the N most recent hot memos across all active sessions.
///
/// Walks `{root}/active/*.ndjson`, reads every entry, projects each into a
/// [`ContextMemo`], then sorts by timestamp descending and truncates to `limit`.
///
/// Returns an empty `Vec` if the `active/` directory doesn't exist, is empty,
/// or every session file fails to parse — never errors, because partial visibility
/// is always better than a blank panel in the UI.
///
/// # Performance
///
/// This scans every active session file on every call. The volume is bounded
/// by practical session counts (~1–10 simultaneously active), so the cost is
/// `O(sessions × entries_per_session)`. For typical usage (<100 total entries
/// across <5 sessions), a single call completes in <5ms.
///
/// If the volume grows, the natural next step is a broadcast channel listener
/// that tails each active session; that's explicitly out of scope for Phase 9.
#[doc(hidden)] // Phase 18c Step 3: no longer on the hot-tier serving path; kept for tests + NDJSON archive inspection
pub async fn snapshot_hot(layout: &StoreLayout, limit: usize) -> Vec<ContextMemo> {
    let reader = TurnLogReader::new(layout.clone());
    let Ok(session_ids) = reader.list_active().await else {
        return Vec::new();
    };

    let mut all_memos: Vec<ContextMemo> = Vec::new();
    for session_id in session_ids {
        let Ok(entries) = reader.read_all(&session_id).await else {
            continue;
        };
        for entry in entries {
            all_memos.push(project_entry(&session_id, &entry));
        }
    }

    // Sort newest-first by timestamp. `ts_ns()` returns 0 on out-of-range
    // timestamps so malformed entries sink to the bottom rather than crashing.
    all_memos.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all_memos.truncate(limit);
    all_memos
}

/// Project a single [`TurnEntry`] into a [`ContextMemo`] for UI display.
///
/// Drops HMAC chain fields (`hmac_prev`, `hmac_self`, `seq`, `parent_seq`)
/// since they are internal to the chain-integrity contract. Keeps the
/// human-meaningful content from the span.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn project_entry(session_id: &str, entry: &TurnEntry) -> ContextMemo {
    let significance = entry
        .span
        .metadata
        .get("significance")
        .and_then(serde_json::Value::as_f64)
        .or_else(|| {
            entry
                .span
                .metadata
                .get("weight")
                .and_then(serde_json::Value::as_f64)
        })
        .map_or(0.5, |v| v as f32);

    let strands = entry
        .span
        .metadata
        .get("strands")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    let content = extract_content(&entry.span.metadata, &entry.span.action);

    ContextMemo {
        id: format!("{session_id}:{}", entry.seq),
        tier: MemoryTier::Hot,
        content,
        significance,
        sibling: entry.span.actor.to_string(),
        strands,
        created_at: entry.span.timestamp.to_rfc3339(),
        source_path: Some(format!("active/{session_id}.ndjson")),
        // Hot memos don't carry the full zettelkasten primitive metadata
        // until they promote to cold — these stay empty at projection time.
        resonance: Vec::new(),
        themes: Vec::new(),
        self_defining: false,
        entry_type: Some(entry.span.action.clone()),
    }
}

/// Extract a human-meaningful summary from a span's metadata.
///
/// Checks, in order: `memo_body`, `summary`, `content`, `message`. Falls back
/// to the action string when none are present. Truncated to [`CONTENT_MAX_CHARS`].
fn extract_content(metadata: &serde_json::Value, action: &str) -> String {
    for key in ["memo_body", "summary", "content", "message"] {
        if let Some(s) = metadata.get(key).and_then(serde_json::Value::as_str) {
            return truncate_chars(s, CONTENT_MAX_CHARS);
        }
    }
    format!("[{action}]")
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
    use lightarchitects::turnlog::{StoreLayout, TurnLogWriter};
    use secrecy::SecretSlice;
    use tempfile::tempdir;

    fn test_pepper() -> SecretSlice<u8> {
        SecretSlice::from(vec![0u8; 32])
    }

    #[tokio::test]
    async fn snapshot_hot_returns_empty_when_no_active_dir() {
        let tmp = tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        let memos = snapshot_hot(&layout, 10).await;
        assert!(memos.is_empty());
    }

    #[tokio::test]
    async fn snapshot_hot_reads_one_session() {
        let tmp = tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        layout.ensure_dirs().await.unwrap();

        let pepper = test_pepper();
        let writer = TurnLogWriter::open(
            &layout,
            "test-session-abc".to_owned(),
            tmp.path().to_path_buf(),
            "claude".to_owned(),
            "webshell".to_owned(),
            None,
            &pepper,
        )
        .await
        .unwrap();

        let ctx = TraceContext::new(Actor::new("webshell"), "reflection")
            .session_id("test-session-abc")
            .outcome(TraceOutcome::Continue)
            .metadata(serde_json::json!({
                "memo_body": "A thing happened worth remembering",
                "significance": 0.85,
                "strands": ["methodical", "contextual"]
            }))
            .finish()
            .unwrap();
        writer.append(ctx);
        // Allow the background writer task to flush before reading.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let memos = snapshot_hot(&layout, 10).await;
        // At least the session_start entry plus our reflection.
        assert!(!memos.is_empty(), "expected at least one memo");
        assert!(memos.iter().any(|m| m.content.contains("A thing happened")));
        assert!(memos.iter().any(|m| {
            m.strands.contains(&"methodical".to_owned()) && (m.significance - 0.85).abs() < 1e-3
        }));

        // Cleanup — close the writer so the tempdir can drop cleanly.
        writer
            .close(lightarchitects::turnlog::EndReason::Complete)
            .await
            .ok();
    }

    #[test]
    fn truncate_preserves_short_strings() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn truncate_appends_ellipsis_on_overflow() {
        let long = "a".repeat(300);
        let truncated = truncate_chars(&long, 200);
        assert_eq!(truncated.chars().count(), 201); // 200 + ellipsis
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn extract_content_prefers_memo_body() {
        let meta = serde_json::json!({
            "memo_body": "The body",
            "summary": "A summary"
        });
        assert_eq!(extract_content(&meta, "reflection"), "The body");
    }

    #[test]
    fn extract_content_falls_back_to_action_tag() {
        let meta = serde_json::json!({});
        assert_eq!(extract_content(&meta, "turn.user"), "[turn.user]");
    }
}
