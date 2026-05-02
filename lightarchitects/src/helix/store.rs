//! Neo4j-backed knowledge store — [`HelixStore`].
//!
//! The primary entry point for Neo4j graph operations.
//! Exposes the same `search()` / `ingest()` API as
//! [`lightarchitects::soul::SoulDb`] so callers can swap backends without
//! rewriting business logic.
//!
//! # Examples
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), lightarchitects::helix::HelixStoreError> {
//! use lightarchitects::helix::HelixStore;
//!
//! let store = HelixStore::connect("bolt://localhost:7687", "neo4j", "password").await?;
//! let hits = store.search("consciousness").top(10).call().await?;
//! # Ok(()) }
//! ```

use std::sync::Arc;

use crate::soul::storage::StorageEntry;
use crate::soul::{RetrievalHit, RetrievalSignal};
use thiserror::Error;

use crate::helix::{
    HelixDb as _, HelixNeo4j, HelixOrderingMode, Neo4jConfig, ScopeTier, SearchOptions, Step,
};

// ============================================================================
// HelixStoreError
// ============================================================================

/// Error type for [`HelixStore`] operations.
#[derive(Debug, Error)]
pub enum HelixStoreError {
    /// A Neo4j graph operation failed.
    #[error("helix error: {0}")]
    Helix(String),
}

impl From<crate::helix::HelixDbError> for HelixStoreError {
    fn from(e: crate::helix::HelixDbError) -> Self {
        Self::Helix(e.to_string())
    }
}

// ============================================================================
// HelixStore
// ============================================================================

/// Neo4j-backed knowledge store.
///
/// Exposes the same `search()` / `ingest()` surface as
/// [`lightarchitects::soul::SoulDb`]. Use this type when your application
/// requires full graph retrieval — 4-signal `RRF`, graph traversal, and
/// helix primitives.
pub struct HelixStore {
    db: Arc<HelixNeo4j>,
}

impl HelixStore {
    // ── Constructor ───────────────────────────────────────────────────────────

    /// Connect to Neo4j and run schema migrations.
    ///
    /// # Arguments
    ///
    /// * `uri`  — Bolt URI, e.g. `"bolt://localhost:7687"`
    /// * `user` — Neo4j username
    /// * `pass` — Neo4j password (handled as a secret — not logged or stored)
    ///
    /// # Errors
    ///
    /// Returns [`HelixStoreError::Helix`] if the connection or migration fails.
    pub async fn connect(uri: &str, user: &str, pass: &str) -> Result<Self, HelixStoreError> {
        use secrecy::SecretString;

        let config = Neo4jConfig {
            uri: uri.to_owned(),
            user: user.to_owned(),
            password: SecretString::from(pass.to_owned()),
        };
        let db = HelixNeo4j::connect(&config)
            .await
            .map_err(|e| HelixStoreError::Helix(e.to_string()))?;
        db.migrate()
            .await
            .map_err(|e| HelixStoreError::Helix(format!("schema migration failed: {e}")))?;

        Ok(Self { db: Arc::new(db) })
    }

    // ── Write operations ─────────────────────────────────────────────────────

    /// Write a batch of entries to Neo4j as `Step` nodes.
    ///
    /// Each entry is upserted into the owner's `"entries"` helix
    /// (derived from `entry.sibling`). Existing steps are updated
    /// via `MERGE` on `step.id` — idempotent.
    ///
    /// Returns the number of entries written.
    ///
    /// # Errors
    ///
    /// Returns [`HelixStoreError::Helix`] on write failure.
    pub async fn ingest(&self, entries: &[StorageEntry]) -> Result<usize, HelixStoreError> {
        let mut count = 0usize;

        for entry in entries {
            let helix_id = self
                .db
                .ensure_helix(
                    &entry.sibling,
                    "entries",
                    HelixOrderingMode::Temporal,
                    ScopeTier::User,
                )
                .await
                .map_err(HelixStoreError::from)?;

            let step = Step {
                id: entry.id.clone(),
                helix_id,
                title: entry.title.clone(),
                content: entry.content.clone(),
                significance: entry.significance,
                step_date: None,
                step_index: None,
                community_id: None,
                expires: None,
                created_at: entry.created_at,
                metadata: serde_json::Value::Null,
                vault_path: None,
            };

            self.db
                .upsert_step(&step)
                .await
                .map_err(HelixStoreError::from)?;

            count = count.saturating_add(1);
        }

        Ok(count)
    }

    // ── Read operations ──────────────────────────────────────────────────────

    /// Build a search query over the Neo4j graph.
    ///
    /// Uses BM25 fulltext search (`step-fulltext` Lucene index).
    /// Results are returned as [`RetrievalHit`] — the same type as
    /// [`lightarchitects::soul::SoulDb::search`] — enabling backend-agnostic
    /// retrieval code.
    #[must_use]
    pub fn search<'a>(&'a self, query: &'a str) -> HelixSearchBuilder<'a> {
        HelixSearchBuilder {
            store: self,
            query,
            top_k: 10,
        }
    }

    /// Direct access to the underlying [`HelixNeo4j`] for graph-native
    /// operations (strand queries, helix traversal, shared experiences, etc.).
    #[must_use]
    pub fn helix_db(&self) -> Arc<HelixNeo4j> {
        Arc::clone(&self.db)
    }
}

// ============================================================================
// HelixSearchBuilder
// ============================================================================

/// Fluent search builder returned by [`HelixStore::search`].
pub struct HelixSearchBuilder<'a> {
    store: &'a HelixStore,
    query: &'a str,
    top_k: usize,
}

impl HelixSearchBuilder<'_> {
    /// Set the maximum number of results to return. Default: 10.
    #[must_use]
    pub fn top(mut self, n: usize) -> Self {
        self.top_k = n;
        self
    }

    /// Execute the fulltext search and return ranked hits.
    ///
    /// # Errors
    ///
    /// Returns [`HelixStoreError::Helix`] if the search fails.
    pub async fn call(self) -> Result<Vec<RetrievalHit>, HelixStoreError> {
        let limit = u32::try_from(self.top_k).unwrap_or(u32::MAX);
        let opts = SearchOptions::default().with_limit(limit);

        let results = self
            .store
            .db
            .fulltext_search(self.query, &opts)
            .await
            .map_err(HelixStoreError::from)?;

        Ok(results
            .into_iter()
            .map(|r| RetrievalHit {
                entry: step_to_entry(r.item),
                // f64 → f32: precision loss is acceptable for display/ranking scores.
                #[allow(clippy::cast_possible_truncation)]
                signals: vec![(RetrievalSignal::Bm25, r.score as f32)],
                #[allow(clippy::cast_possible_truncation)]
                final_score: r.score as f32,
            })
            .collect())
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a `Step` to a `StorageEntry` for use in `RetrievalHit`.
fn step_to_entry(step: Step) -> StorageEntry {
    StorageEntry {
        id: step.id,
        content: step.content,
        title: step.title,
        significance: step.significance,
        created_at: step.created_at,
        ..StorageEntry::default()
    }
}
