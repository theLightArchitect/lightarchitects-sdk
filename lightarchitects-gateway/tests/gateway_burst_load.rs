//! Burst-load concurrency test for `write_span_to_disk`.
//!
//! Spawns 32 concurrent tasks that each write a distinct span into the same
//! temp directory.  Verifies:
//!   - all 32 `.json` files land (no collisions / silent drops)
//!   - zero `.tmp` files remain (every write completed the atomic rename)
//!   - all files parse as valid JSON with the expected AYIN schema fields

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;
use std::path::PathBuf;

use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
use lightarchitects_gateway::span_context::write_span_to_disk;

const CONCURRENT_WRITERS: usize = 32;

#[tokio::test]
async fn burst_32_concurrent_writes_all_land() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dir_path = PathBuf::from(dir.path());

    let handles: Vec<_> = (0..CONCURRENT_WRITERS)
        .map(|_| {
            let path = dir_path.clone();
            tokio::spawn(async move {
                let span = TraceContext::new(Actor::new("gateway"), "burst.write")
                    .outcome(TraceOutcome::Continue)
                    .finish()
                    .expect("span build");
                write_span_to_disk(&span, &path).await.expect("write")
            })
        })
        .collect();

    for h in handles {
        h.await.expect("task panicked");
    }

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .collect();

    let json_count = entries
        .iter()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .count();
    assert_eq!(
        json_count, CONCURRENT_WRITERS,
        "expected exactly {CONCURRENT_WRITERS} .json files, got {json_count}"
    );

    let tmp_count = entries
        .iter()
        .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
        .count();
    assert_eq!(tmp_count, 0, "stale .tmp files after burst write");
}

#[tokio::test]
async fn burst_writes_produce_unique_ids() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dir_path = PathBuf::from(dir.path());

    let handles: Vec<_> = (0..CONCURRENT_WRITERS)
        .map(|i| {
            let path = dir_path.clone();
            let action = format!("burst.{i}");
            tokio::spawn(async move {
                let span = TraceContext::new(Actor::new("gateway"), action.as_str())
                    .outcome(TraceOutcome::Continue)
                    .finish()
                    .expect("span build");
                let id = span.id.to_string();
                write_span_to_disk(&span, &path).await.expect("write");
                id
            })
        })
        .collect();

    let mut ids = HashSet::new();
    for h in handles {
        let id = h.await.expect("task panicked");
        assert!(ids.insert(id), "duplicate span id detected");
    }
    assert_eq!(ids.len(), CONCURRENT_WRITERS);
}

#[tokio::test]
async fn burst_written_json_is_valid_ayin_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dir_path = PathBuf::from(dir.path());

    let handles: Vec<_> = (0..CONCURRENT_WRITERS)
        .map(|_| {
            let path = dir_path.clone();
            tokio::spawn(async move {
                let span = TraceContext::new(Actor::new("gateway"), "burst.schema.check")
                    .outcome(TraceOutcome::Continue)
                    .finish()
                    .expect("span build");
                write_span_to_disk(&span, &path).await.expect("write");
            })
        })
        .collect();

    for h in handles {
        h.await.expect("task panicked");
    }

    for entry in std::fs::read_dir(dir.path())
        .expect("readdir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
    {
        let raw = std::fs::read(entry.path()).expect("read file");
        let v: serde_json::Value = serde_json::from_slice(&raw).expect("valid json");
        assert!(v["id"].is_string(), "id missing in {:?}", entry.path());
        assert!(v["timestamp"].is_string(), "timestamp missing");
        // Actor is #[serde(transparent)] — plain string in JSON.
        assert_eq!(v["actor"], "gateway");
    }
}
