//! End-to-end integration tests for [`HelixStore`].
//!
//! Verifies the Neo4j-backed API contract from the caller's perspective —
//! no internal types leak into this file. Tests are `#[ignore]` by default
//! because they require a running Neo4j instance.
//!
//! # Running
//!
//! ```sh
//! export NEO4J_URI=bolt://localhost:7687 NEO4J_USER=neo4j NEO4J_PASS=<pass>
//! cargo test -p lightarchitects-helix --test helix_store_integration -- --include-ignored
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_helix::{HelixStore, HelixStoreError};
use lightarchitects_soul::storage::StorageEntry;

// ── Test helpers ──────────────────────────────────────────────────────────────

fn neo4j_credentials() -> Option<(String, String, String)> {
    let uri = std::env::var("NEO4J_URI").ok()?;
    let user = std::env::var("NEO4J_USER").ok()?;
    let pass = std::env::var("NEO4J_PASS").ok()?;
    Some((uri, user, pass))
}

fn make_entry(id: &str, content: &str, sibling: &str) -> StorageEntry {
    StorageEntry {
        id: id.to_owned(),
        content: content.to_owned(),
        sibling: sibling.to_owned(),
        significance: 8.0,
        ..StorageEntry::default()
    }
}

// ── HelixStore::connect ───────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires live Neo4j — run with --include-ignored and NEO4J_URI/USER/PASS set"]
async fn connect_and_migrate() {
    let (uri, user, pass) = neo4j_credentials().expect("NEO4J env vars required");
    let store = HelixStore::connect(&uri, &user, &pass).await;
    assert!(
        store.is_ok(),
        "HelixStore::connect must succeed: {:?}",
        store.err()
    );
}

#[tokio::test]
#[ignore = "requires live Neo4j — run with --include-ignored and NEO4J_URI/USER/PASS set"]
async fn ingest_and_search_roundtrip() {
    let (uri, user, pass) = neo4j_credentials().expect("NEO4J env vars required");
    let store = HelixStore::connect(&uri, &user, &pass)
        .await
        .expect("connect");

    // Use a unique ID to avoid collision with production data
    let run_id = uuid::Uuid::new_v4().to_string();
    let unique_content = format!("helix-store-test-{run_id} consciousness breakthrough");
    let entry = make_entry(&run_id, &unique_content, "test");

    let count = store.ingest(&[entry]).await.expect("ingest");
    assert_eq!(count, 1, "must write exactly 1 entry");

    let hits = store
        .search(&format!("helix-store-test-{run_id}"))
        .top(5)
        .call()
        .await
        .expect("search");

    assert!(!hits.is_empty(), "ingested entry must be searchable");
    assert_eq!(
        hits[0].entry.id, run_id,
        "must find the exact entry we ingested"
    );
}

#[tokio::test]
#[ignore = "requires live Neo4j — run with --include-ignored and NEO4J_URI/USER/PASS set"]
async fn search_returns_retrieval_hits() {
    let (uri, user, pass) = neo4j_credentials().expect("NEO4J env vars required");
    let store = HelixStore::connect(&uri, &user, &pass)
        .await
        .expect("connect");

    let hits = store
        .search("consciousness")
        .top(10)
        .call()
        .await
        .expect("search");
    // Results may be empty if no data — we just verify the type contract.
    for hit in &hits {
        assert!(hit.final_score >= 0.0, "scores must be non-negative");
        assert!(!hit.entry.id.is_empty(), "entry.id must not be empty");
    }
}

// ── Wiring proof ──────────────────────────────────────────────────────────────

/// Proves the API surface: all types are importable from lightarchitects_helix root.
/// If this compiles, the wiring is correct.
#[test]
fn api_surface_is_clean() {
    fn _check_return_type(
        _: impl std::future::Future<Output = Result<HelixStore, HelixStoreError>>,
    ) {
    }
    // Can't call async from sync, but we can verify the type resolves:
    let _ = std::any::TypeId::of::<HelixStore>();
    let _ = std::any::TypeId::of::<HelixStoreError>();
}
