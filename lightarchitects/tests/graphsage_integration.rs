//! `GraphSAGE` provider integration tests — Phase 3.
//!
//! Exercises `GraphSageProvider` against a real ephemeral Neo4j instance.
//! Tests are `#[ignore]` by default to avoid requiring Docker.
//!
//! # Running
//!
//! ```bash
//! cargo test -p lightarchitects graphsage -- --ignored
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_errors_doc
)]

use std::sync::Arc;

use lightarchitects::helix::embedding::{EmbeddingProvider, MockEmbeddingProvider};
use lightarchitects::helix::soul_search::{GraphSageProvider, ProjectionWeights};

// ── Unit tests (no Docker required) ──────────────────────────────────────────

/// `GraphSageProvider::embed` returns 128-dim vectors.
#[tokio::test]
async fn embed_returns_128_dim() {
    let inner = Arc::new(MockEmbeddingProvider::new(384));
    let weights = ProjectionWeights::random_stable();
    let provider = GraphSageProvider::with_weights(inner, weights);

    let results = provider
        .embed(&["hello world"])
        .await
        .expect("embed must not fail");

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].len(),
        128,
        "GraphSageProvider must produce 128-dim output"
    );
}

/// Same input produces identical output (deterministic projection).
#[tokio::test]
async fn embed_is_deterministic() {
    let inner = Arc::new(MockEmbeddingProvider::new(384));
    let weights = ProjectionWeights::random_stable();
    let provider = GraphSageProvider::with_weights(inner, weights);

    let r1 = provider.embed(&["determinism test"]).await.unwrap();
    let r2 = provider.embed(&["determinism test"]).await.unwrap();

    assert_eq!(r1[0], r2[0], "embed must be deterministic for same input");
}

/// Different inputs produce different outputs (projection is non-degenerate).
#[tokio::test]
async fn embed_different_inputs_differ() {
    let inner = Arc::new(MockEmbeddingProvider::new(384));
    let weights = ProjectionWeights::random_stable();
    let provider = GraphSageProvider::with_weights(inner, weights);

    let r1 = provider.embed(&["query about memory"]).await.unwrap();
    let r2 = provider.embed(&["query about deployment"]).await.unwrap();

    let same = r1[0]
        .iter()
        .zip(r2[0].iter())
        .all(|(a, b)| (a - b).abs() < 1e-6_f32);
    assert!(
        !same,
        "different inputs must produce different 128-dim vectors"
    );
}

/// Provider name and dimensions match the structural slot requirements.
#[test]
fn provider_metadata() {
    let inner = Arc::new(MockEmbeddingProvider::new(384));
    let weights = ProjectionWeights::random_stable();
    let provider = GraphSageProvider::with_weights(inner, weights);

    assert_eq!(provider.dimensions(), 128);
    assert_eq!(provider.name(), "graphsage");
}

// ── Docker integration tests ──────────────────────────────────────────────────

/// After `migrate()` runs, the `step-sage-embeddings` index exists in Neo4j.
///
/// Verifies migration v11 creates the correct HNSW index definition.
#[tokio::test]
#[ignore = "Requires Docker — run with: cargo test -p lightarchitects graphsage -- --ignored"]
async fn sage_embeddings_index_created_by_migration() {
    use lightarchitects::helix::{HelixDb, HelixNeo4j, Neo4jConfig, Neo4jConnectionMode};
    use secrecy::SecretString;
    use testcontainers_modules::neo4j::{Neo4j, Neo4jImage};
    use testcontainers_modules::testcontainers::ContainerAsync;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    let container: ContainerAsync<Neo4jImage> = Neo4j::default()
        .with_user("neo4j")
        .with_password("testpassword")
        .start()
        .await
        .expect("Neo4j container start");

    let host = container.get_host().await.expect("container host");
    let port = container.image().bolt_port_ipv4().expect("bolt port");
    let uri = format!("bolt://{host}:{port}");

    let config = Neo4jConfig {
        uri,
        user: "neo4j".to_owned(),
        password: SecretString::new("testpassword".to_owned().into()),
        mode: Neo4jConnectionMode::Local,
    };

    let db = HelixNeo4j::connect(&config)
        .await
        .expect("connect to Neo4j");
    db.migrate().await.expect("run helix migrations");

    // Query Neo4j's index catalog to confirm step-sage-embeddings was created.
    let results = db
        .execute_cypher(
            "SHOW VECTOR INDEXES WHERE name = 'step-sage-embeddings' \
             RETURN name, labelsOrTypes, properties, options",
        )
        .await
        .expect("SHOW VECTOR INDEXES");

    assert_eq!(
        results.len(),
        1,
        "step-sage-embeddings index must exist after migration"
    );

    // Drop container — implicit via RAII
    drop(container);
}
