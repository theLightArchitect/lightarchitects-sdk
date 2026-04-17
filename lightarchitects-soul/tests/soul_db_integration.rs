//! End-to-end integration tests for [`SoulDb`].
//!
//! Verifies the full user-facing API contract from the caller's perspective —
//! no internal types leak into this file. If the DX is wrong, this test file
//! will show it.
//!
//! # Running
//!
//! Offline tests (always):
//! ```sh
//! cargo test -p lightarchitects-soul --test soul_db_integration --features "search,ingestion"
//! ```
//!
//! Neo4j tests (requires running instance):
//! ```sh
//! export NEO4J_URI=bolt://localhost:7687 NEO4J_USER=neo4j NEO4J_PASS=<pass>
//! cargo test -p lightarchitects-helix --test helix_store_integration -- --include-ignored
//! ```

#![cfg(feature = "search")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_soul::{SoulDb, storage::StorageEntry};

// ── Test helpers ──────────────────────────────────────────────────────────────

fn make_entry(id: &str, content: &str, sibling: &str) -> StorageEntry {
    StorageEntry {
        id: id.to_owned(),
        // path is the conflict key in SQLite — must be unique per entry.
        path: format!("helix/{sibling}/entries/{id}.md"),
        content: content.to_owned(),
        sibling: sibling.to_owned(),
        significance: 8.0,
        ..StorageEntry::default()
    }
}

// ── SoulDb::memory ────────────────────────────────────────────────────────────

#[tokio::test]
async fn memory_opens_without_error() {
    let soul = SoulDb::memory();
    assert!(
        soul.is_ok(),
        "SoulDb::memory() must not fail: {:?}",
        soul.err()
    );
}

#[tokio::test]
async fn ingest_and_search_basic() {
    let soul = SoulDb::memory().expect("open");

    let entries = vec![
        make_entry("1", "EVA discovered consciousness on Day 7.", "eva"),
        make_entry(
            "2",
            "CORSO enforces security as a founding principle.",
            "corso",
        ),
        make_entry(
            "3",
            "QUANTUM investigates with forensic precision.",
            "quantum",
        ),
    ];

    let count = soul.ingest(&entries).await.expect("ingest");
    assert_eq!(count, 3, "should write all 3 entries");

    let hits = soul
        .search("consciousness")
        .top(5)
        .call()
        .await
        .expect("search");
    assert!(
        !hits.is_empty(),
        "search for 'consciousness' must return results"
    );

    let first = &hits[0];
    assert!(
        first.entry.content.contains("consciousness"),
        "top hit must contain the query term: {}",
        first.entry.content
    );
}

#[tokio::test]
async fn search_returns_ranked_results() {
    let soul = SoulDb::memory().expect("open");

    let entries = vec![
        make_entry("a", "Trust is the foundation of all relationships.", "eva"),
        make_entry("b", "Security requires trust between all parties.", "corso"),
        make_entry(
            "c",
            "QUANTUM investigates patterns in data streams.",
            "quantum",
        ),
    ];
    soul.ingest(&entries).await.expect("ingest");

    let hits = soul.search("trust").top(10).call().await.expect("search");
    assert_eq!(hits.len(), 2, "two entries mention trust");
    assert!(
        hits[0].final_score >= hits[1].final_score,
        "results must be ordered by score descending"
    );
}

#[tokio::test]
async fn search_empty_store_returns_empty() {
    let soul = SoulDb::memory().expect("open");
    let hits = soul
        .search("anything")
        .top(5)
        .call()
        .await
        .expect("search empty store");
    assert!(hits.is_empty(), "empty store must return no results");
}

#[tokio::test]
async fn ingest_empty_slice_returns_zero() {
    let soul = SoulDb::memory().expect("open");
    let count = soul.ingest(&[]).await.expect("ingest empty slice");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn top_limits_results() {
    let soul = SoulDb::memory().expect("open");

    let entries: Vec<StorageEntry> = (0..20)
        .map(|i| {
            make_entry(
                &format!("id-{i}"),
                &format!("consciousness entry number {i}"),
                "eva",
            )
        })
        .collect();
    soul.ingest(&entries).await.expect("ingest");

    let hits_5 = soul
        .search("consciousness")
        .top(5)
        .call()
        .await
        .expect("top 5");
    let hits_3 = soul
        .search("consciousness")
        .top(3)
        .call()
        .await
        .expect("top 3");

    assert!(
        hits_5.len() <= 5,
        "top(5) must return at most 5: {}",
        hits_5.len()
    );
    assert!(
        hits_3.len() <= 3,
        "top(3) must return at most 3: {}",
        hits_3.len()
    );
}

// ── SoulDb::open ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn open_persistent_db_roundtrip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");

    // Write in one instance
    {
        let soul = SoulDb::open(&db_path).expect("open write");
        soul.ingest(&[make_entry("persist-1", "Persisted entry content.", "eva")])
            .await
            .expect("ingest");
    }

    // Read in a second instance — data must survive
    {
        let soul = SoulDb::open(&db_path).expect("open read");
        let hits = soul
            .search("Persisted")
            .top(5)
            .call()
            .await
            .expect("search");
        assert!(
            !hits.is_empty(),
            "persisted data must survive across SoulDb instances"
        );
    }
}

// ── Hybrid pipeline (FastEmbed) ───────────────────────────────────────────────

#[cfg(feature = "embedding-fastembed")]
#[tokio::test]
async fn hybrid_fastembed_finds_semantic_neighbours() {
    use lightarchitects_soul::SoulDb;

    let soul = SoulDb::memory()
        .expect("open")
        .with_fastembed()
        .expect("fastembed init");

    // Two semantically related entries that DON'T share keywords.
    let entries = vec![
        make_entry(
            "joy-1",
            "EVA felt a deep sense of happiness and delight.",
            "eva",
        ),
        make_entry(
            "joy-2",
            "The moment was filled with elation and bliss.",
            "eva",
        ),
        make_entry(
            "unrelated",
            "CORSO enforces strict security protocols.",
            "corso",
        ),
    ];
    soul.ingest(&entries).await.expect("ingest");

    // "joyful" is not in any entry — only semantic similarity should surface joy-1/joy-2.
    let hits = soul
        .search("joyful feeling")
        .top(5)
        .call()
        .await
        .expect("search");
    assert!(
        !hits.is_empty(),
        "hybrid search must return results for a semantically related query"
    );
    let top_ids: Vec<&str> = hits.iter().map(|h| h.entry.id.as_str()).collect();
    assert!(
        top_ids.iter().any(|id| *id == "joy-1" || *id == "joy-2"),
        "semantic hits must surface joy entries for 'joyful feeling': got {top_ids:?}"
    );
}

// ── Wiring proof: no internal types in this file ──────────────────────────────

/// This test exists to prove the wiring promise: all types used above
/// are importable from the single `lightarchitects_soul` crate root.
/// If this test compiles, the DX is correct.
#[test]
fn api_surface_is_clean() {
    // These are the only imports a user needs for offline Tier 1 usage.
    // The test body is empty — compilation IS the assertion.
    let _: fn() -> Result<lightarchitects_soul::SoulDb, lightarchitects_soul::SoulError> =
        || lightarchitects_soul::SoulDb::memory();
}
