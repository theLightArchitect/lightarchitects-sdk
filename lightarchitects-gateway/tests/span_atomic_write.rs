//! Atomic write semantics + span_dir layout tests.
//!
//! Verifies R11/R12 durability requirements:
//!   - `span_dir` produces the expected `<base>/<actor>/<YYYY-MM-DD>` hierarchy.
//!   - No `.tmp` files remain after a successful atomic rename.
//!   - Spans exceeding the 64 KB budget are silently dropped (return Ok, no file).
//!   - Actions containing `/` are sanitised in the filename (no path traversal).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use chrono::Utc;
use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
use lightarchitects_gateway::span_context::{span_dir, write_span_to_disk};

fn make_span(action: &str) -> lightarchitects::ayin::span::TraceSpan {
    TraceContext::new(Actor::new("gateway"), action)
        .outcome(TraceOutcome::Continue)
        .finish()
        .expect("span build")
}

// ── span_dir layout ───────────────────────────────────────────────────────────

/// `span_dir` returns `<base>/<actor>/<YYYY-MM-DD>` — the canonical AYIN
/// trace hierarchy that both `ayin-viewer` and the shell hook expect.
#[test]
fn span_dir_layout_matches_actor_date_hierarchy() {
    let base = std::path::Path::new("/traces");
    let now = Utc::now();
    let date_str = now.format("%Y-%m-%d").to_string();

    let dir = span_dir(base, "gateway", &now);

    assert_eq!(dir, PathBuf::from(format!("/traces/gateway/{date_str}")));
}

// ── Atomic rename — no .tmp ───────────────────────────────────────────────────

/// After a successful `write_span_to_disk` the `.tmp` staging file must be
/// absent — the atomic rename completed and only the final `.json` remains.
#[tokio::test]
async fn atomic_write_leaves_no_tmp_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let span = make_span("gateway.atomic.test");

    write_span_to_disk(&span, &PathBuf::from(dir.path()))
        .await
        .expect("write");

    let tmp_count = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
        .count();
    assert_eq!(tmp_count, 0, "stale .tmp files after write");

    let json_count = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .count();
    assert_eq!(json_count, 1, "exactly one .json file written");
}

// ── Oversized span budget ─────────────────────────────────────────────────────

/// Spans whose serialised payload exceeds 64 KB are silently dropped.
///
/// `write_span_to_disk` returns `Ok(())` but writes no file — this prevents
/// R11 eviction attacks via unbounded span payloads.
#[tokio::test]
async fn oversized_span_silently_dropped_returns_ok() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Build an action string large enough to push the JSON payload past 64 KB.
    // 66_000 'a' chars + ~200 bytes JSON framing ≈ 66 KB — exceeds the 64 KB cap.
    let big_action = "a".repeat(66_000);
    let span = make_span(&big_action);

    let result = write_span_to_disk(&span, &PathBuf::from(dir.path())).await;
    assert!(result.is_ok(), "oversized span must return Ok, not Err");

    let file_count = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            let ext = e.path();
            let ext = ext.extension().unwrap_or_default();
            ext == "json" || ext == "tmp"
        })
        .count();
    assert_eq!(
        file_count, 0,
        "oversized span must not write any file to disk"
    );
}

// ── Action sanitisation in filename ──────────────────────────────────────────

/// Actions containing `/` are replaced with `_` in the filename — preventing
/// path traversal and ensuring the span lands as a flat file in the trace dir.
#[tokio::test]
async fn action_with_slash_creates_safe_filename() {
    let dir = tempfile::tempdir().expect("tempdir");
    let span = make_span("tool/dispatch");

    write_span_to_disk(&span, &PathBuf::from(dir.path()))
        .await
        .expect("write");

    let files: Vec<_> = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect();
    assert_eq!(files.len(), 1, "exactly one file");

    let name = files[0].file_name();
    let name_str = name.to_string_lossy();
    assert!(
        name_str.contains("tool_dispatch"),
        "filename must use '_' not '/' for action: got {name_str}"
    );
    assert!(
        !name_str.contains('/'),
        "filename must not contain a path separator: got {name_str}"
    );
}
