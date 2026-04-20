//! Wikilink slug-resolution integration tests — Phase 11.5.
//!
//! Exercises `HelixNeo4j::create_link` against a real ephemeral Neo4j
//! instance provided by `testcontainers-modules`. Tests are `#[ignore]`
//! by default to avoid requiring Docker on every `cargo test` run.
//!
//! # Running
//!
//! ```bash
//! cargo test -p lightarchitects wikilink -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_errors_doc
)]

use chrono::Utc;
use lightarchitects::helix::{HelixDb, HelixLink, HelixNeo4j, LinkType, Neo4jConfig, Step};
use secrecy::SecretString;
use testcontainers_modules::neo4j::{Neo4j, Neo4jImage};
use testcontainers_modules::testcontainers::ContainerAsync;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

// ── Container credentials ─────────────────────────────────────────────────────

const TEST_USER: &str = "neo4j";
const TEST_PASS: &str = "testpassword";

// ── Setup helpers ─────────────────────────────────────────────────────────────

/// Spin up an ephemeral Neo4j container and return a migrated [`HelixNeo4j`].
///
/// The caller must hold the returned `ContainerAsync` alive for the duration
/// of the test — dropping it stops the container.
async fn setup_neo4j() -> (ContainerAsync<Neo4jImage>, HelixNeo4j) {
    let container = Neo4j::default()
        .with_user(TEST_USER)
        .with_password(TEST_PASS)
        .start()
        .await
        .expect("Neo4j container start");

    let host = container.get_host().await.expect("container host");
    let port = container.image().bolt_port_ipv4().expect("bolt port");
    let uri = format!("bolt://{host}:{port}");

    let config = Neo4jConfig {
        uri,
        user: TEST_USER.to_owned(),
        password: SecretString::new(TEST_PASS.to_owned().into()),
    };

    let db = HelixNeo4j::connect(&config)
        .await
        .expect("connect to Neo4j");
    db.migrate().await.expect("run helix migrations");
    (container, db)
}

/// Build a minimal [`Step`] for test insertion.
fn make_step(id: &str, vault_path: Option<&str>) -> Step {
    Step {
        id: id.to_owned(),
        helix_id: "test-helix".to_owned(),
        title: None,
        content: format!("Integration test content for {id}"),
        significance: 5.0,
        step_date: None,
        step_index: None,
        community_id: None,
        expires: None,
        created_at: Utc::now(),
        metadata: serde_json::Value::Null,
        vault_path: vault_path.map(str::to_owned),
    }
}

/// Build a [`HelixLink`] of kind `Wikilink` between two IDs.
fn wikilink(source_id: &str, target_id: &str) -> HelixLink {
    HelixLink {
        source_id: source_id.to_owned(),
        target_id: target_id.to_owned(),
        link_type: LinkType::Wikilink,
        strength: 1.0,
        raw_wikilink: Some(format!("[[{target_id}]]")),
        metadata: serde_json::Value::Null,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Stage-1 UUID resolution: target is matched directly by its `id` property.
///
/// This is the fast path — the wikilink text is the target's UUID string.
#[tokio::test]
#[ignore = "Requires Docker — run with: cargo test -p lightarchitects wikilink -- --ignored"]
async fn uuid_hit() {
    let (_container, db) = setup_neo4j().await;

    let source = make_step("src-uuid-1", None);
    let target = make_step("tgt-uuid-1", None);
    db.create_step(&source).await.expect("insert source");
    db.create_step(&target).await.expect("insert target");

    let link = wikilink(&source.id, &target.id);
    let edge_id = db.create_link(&link).await.expect("create_link UUID hit");
    assert!(
        !edge_id.is_empty(),
        "edge_id should be a non-empty UUID string"
    );
}

/// Stage-2 path resolution: target matched via `vault_path ENDS WITH slug`.
///
/// Simulates Obsidian wikilinks that use bare path slugs (`[[eva/identity]]`)
/// against steps whose `vault_path` includes the `.md` suffix.
#[tokio::test]
#[ignore = "Requires Docker — run with: cargo test -p lightarchitects wikilink -- --ignored"]
async fn path_hit() {
    let (_container, db) = setup_neo4j().await;

    let source = make_step("src-path-1", None);
    let target = make_step("tgt-path-1", Some("eva/identity.md"));
    db.create_step(&source).await.expect("insert source");
    db.create_step(&target).await.expect("insert target");

    // Slug without `.md` — the Cypher appends `.md` via `$target_id_md`.
    let link = wikilink(&source.id, "eva/identity");
    let edge_id = db.create_link(&link).await.expect("create_link path hit");
    assert!(!edge_id.is_empty());
}

/// Stage-2 resolution when the wikilink already carries the `.md` extension.
///
/// `[[eva/identity.md]]` must resolve against `vault_path = "eva/identity.md"`
/// via the `$target_id` match branch (not `$target_id_md`).
#[tokio::test]
#[ignore = "Requires Docker — run with: cargo test -p lightarchitects wikilink -- --ignored"]
async fn md_variant() {
    let (_container, db) = setup_neo4j().await;

    let source = make_step("src-md-1", None);
    let target = make_step("tgt-md-1", Some("eva/identity.md"));
    db.create_step(&source).await.expect("insert source");
    db.create_step(&target).await.expect("insert target");

    // Slug already includes `.md` — resolved by `ENDS WITH $target_id`.
    let link = wikilink(&source.id, "eva/identity.md");
    let edge_id = db
        .create_link(&link)
        .await
        .expect("create_link .md variant");
    assert!(!edge_id.is_empty());
}

/// Stage-2 resolution under a nested directory hierarchy.
///
/// `[[corso/builds/x/plan]]` must resolve against
/// `vault_path = "corso/builds/x/plan.md"`.
#[tokio::test]
#[ignore = "Requires Docker — run with: cargo test -p lightarchitects wikilink -- --ignored"]
async fn nested_path() {
    let (_container, db) = setup_neo4j().await;

    let source = make_step("src-nested-1", None);
    let target = make_step("tgt-nested-1", Some("corso/builds/x/plan.md"));
    db.create_step(&source).await.expect("insert source");
    db.create_step(&target).await.expect("insert target");

    let link = wikilink(&source.id, "corso/builds/x/plan");
    let edge_id = db
        .create_link(&link)
        .await
        .expect("create_link nested path");
    assert!(!edge_id.is_empty());
}

/// `MERGE` deduplication: two identical `create_link` calls must produce
/// exactly one `:LINKS_TO` edge.
///
/// The assertion leverages `create_link`'s `ON CREATE SET r.id = $rel_id`
/// semantics: both calls must return the **same** edge ID, proving that the
/// second call found — rather than created — the relationship.
#[tokio::test]
#[ignore = "Requires Docker — run with: cargo test -p lightarchitects wikilink -- --ignored"]
async fn duplicate_dedup() {
    let (_container, db) = setup_neo4j().await;

    let source = make_step("src-dedup-1", None);
    let target = make_step("tgt-dedup-1", None);
    db.create_step(&source).await.expect("insert source");
    db.create_step(&target).await.expect("insert target");

    let link = wikilink(&source.id, &target.id);
    let id1 = db.create_link(&link).await.expect("first create_link");
    let id2 = db
        .create_link(&link)
        .await
        .expect("second create_link (MERGE, must not error)");

    assert_eq!(
        id1, id2,
        "Both calls must return the same edge ID — MERGE must not create a second edge"
    );
}
