//! Helix database — domain-specific graph operations on top of `graph-engine`.
//!
//! [`HelixDb`] is the trait defining helix-aware graph operations.
//! [`HelixNeo4j`] implements it using `graph-engine::Neo4jBackend`.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────┐
//! │   soul-helix (domain logic)      │
//! │   HelixDb trait                  │
//! ├──────────────────────────────────┤
//! │   HelixNeo4j implementation      │
//! │   (extends Neo4jBackend)         │
//! ├──────────────────────────────────┤
//! │   graph-engine (storage)         │
//! │   GraphStore trait / Neo4jBackend│
//! └──────────────────────────────────┘
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use crate::core::paths;

use crate::helix::graph::GraphStore;
use crate::helix::graph::neo4j::Neo4jBackend;
use async_trait::async_trait;
use chrono::DateTime;
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use tracing::instrument;

use crate::helix::search::{ScoredResult, SearchOptions, index_names};
use crate::helix::types::{
    DiscoveryMethod, Helix, HelixLink, HelixOrderingMode, MAX_TRAVERSAL_DEPTH, SharedExperience,
    Step, Strand, StrandMembership,
};

// ============================================================================
// Observability Constants
// ============================================================================

/// Threshold in milliseconds above which queries are logged at WARN level.
const SLOW_QUERY_THRESHOLD_MS: u128 = 500;

// ============================================================================
// HelixDb Trait
// ============================================================================

/// Domain-specific helix graph operations.
///
/// All operations use application-level types ([`Helix`], [`Step`], etc.)
/// rather than raw graph primitives. The underlying [`GraphStore`] handles
/// Cypher parameterization and injection prevention.
#[async_trait]
pub trait HelixDb: Send + Sync {
    /// Create or update a helix node.
    async fn upsert_helix(&self, helix: &Helix) -> Result<String, HelixDbError>;

    /// Retrieve a helix by its ID.
    ///
    /// # Errors
    ///
    /// Returns [`HelixDbError::NotFound`] if the helix does not exist.
    async fn get_helix(&self, helix_id: &str) -> Result<Helix, HelixDbError>;

    /// Create a step within a helix.
    async fn create_step(&self, step: &Step) -> Result<String, HelixDbError>;

    /// Get steps from a helix, ordered by its ordering mode.
    async fn get_steps(
        &self,
        helix_id: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<Step>, HelixDbError>;

    /// Create a strand and its domain helix.
    async fn create_strand(&self, strand: &Strand) -> Result<String, HelixDbError>;

    /// Assign a step to a strand with a weight.
    async fn assign_to_strand(&self, membership: &StrandMembership) -> Result<(), HelixDbError>;

    /// Create a link between two steps.
    async fn create_link(&self, link: &HelixLink) -> Result<String, HelixDbError>;

    /// Phase 14.2 — create a typed relationship between two steps.
    ///
    /// Same source/target resolution as [`HelixDb::create_link`] (UUID first,
    /// `vault_path` suffix fallback with `.md` variant) but writes a
    /// differently-labeled relationship in Neo4j. The label must appear in
    /// [`HELIX_REL_TYPES`]; unknown labels return `HelixDbError::Validation`.
    ///
    /// Idempotent via `MERGE`: re-creating an existing `source→rel_type→target`
    /// triple returns the existing relationship's id. Used by the markdown
    /// vault ingester to materialise `PLAN_FOR_BUILD`, `REVIEWS_PLAN`, and
    /// `LESSON_FROM_ENTRY` edges when front-matter advertises them.
    async fn create_typed_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
    ) -> Result<String, HelixDbError>;

    /// Create a shared experience node with participant step IDs.
    async fn create_shared_experience(
        &self,
        experience: &SharedExperience,
        participant_step_ids: &[String],
    ) -> Result<String, HelixDbError>;

    /// Query shared experiences for a helix.
    async fn query_convergences(
        &self,
        helix_id: &str,
        min_participants: Option<usize>,
    ) -> Result<Vec<SharedExperience>, HelixDbError>;

    /// Drill down into sub-helixes from a step.
    async fn drill_down(
        &self,
        step_id: &str,
        max_depth: u8,
        min_significance: Option<f64>,
    ) -> Result<Vec<Step>, HelixDbError>;

    /// Find backlinks to a step (reverse `[:LINKS_TO]` traversal).
    async fn find_backlinks(&self, step_id: &str) -> Result<Vec<Step>, HelixDbError>;

    /// Get or create a day-step for a helix.
    async fn get_or_create_day_step(
        &self,
        helix_id: &str,
        date: chrono::NaiveDate,
    ) -> Result<String, HelixDbError>;

    /// Full-text search over Step content and title.
    ///
    /// Uses the Lucene `step-fulltext` index (English analyzer, BM25 scoring).
    /// Supports Lucene query syntax: quoted phrases, boolean operators, fuzzy (`~`).
    async fn fulltext_search(
        &self,
        query: &str,
        opts: &SearchOptions,
    ) -> Result<Vec<ScoredResult<Step>>, HelixDbError>;

    /// Vector similarity search over Step embeddings.
    ///
    /// Uses HNSW index for approximate nearest-neighbor lookup.
    /// Pass `index_name` to select semantic (768-dim) or structural (128-dim).
    async fn vector_search(
        &self,
        embedding: &[f32],
        index_name: &str,
        opts: &SearchOptions,
    ) -> Result<Vec<ScoredResult<Step>>, HelixDbError>;

    /// Idempotent helix creation using Cypher `MERGE`.
    ///
    /// If a helix with the given owner+name already exists, returns its ID.
    /// Otherwise creates it. Safe for concurrent calls from parallel ingestors.
    async fn ensure_helix(
        &self,
        owner: &str,
        name: &str,
        ordering_mode: crate::helix::types::HelixOrderingMode,
    ) -> Result<String, HelixDbError>;

    /// Content-hash dedup step upsert.
    ///
    /// Computes SHA-256 of `step.content` and uses `MERGE` on `content_hash`.
    /// If a step with the same hash exists, it is updated. Otherwise created.
    /// Returns `(step_id, was_created)`.
    async fn upsert_step(&self, step: &Step) -> Result<(String, bool), HelixDbError>;

    /// Idempotent strand creation using Cypher `MERGE`.
    ///
    /// If a strand with the given name under the parent helix already exists,
    /// returns its ID. Otherwise creates it with a new domain helix.
    async fn ensure_strand(
        &self,
        parent_helix_id: &str,
        name: &str,
    ) -> Result<String, HelixDbError>;

    /// Register an ingestion source for watermark tracking.
    async fn register_source(
        &self,
        source: &crate::helix::types::SourceWatermark,
    ) -> Result<String, HelixDbError>;

    /// Update the watermark for a previously registered source.
    async fn update_source_watermark(
        &self,
        source_id: &str,
        last_ingested_at: DateTime<chrono::Utc>,
        record_count: u64,
    ) -> Result<(), HelixDbError>;

    /// Write a personality profile into a helix's metadata.
    async fn write_personality(
        &self,
        helix_id: &str,
        profile: &crate::helix::types::PersonalityProfile,
    ) -> Result<(), HelixDbError>;

    /// Check if a step already has a semantic embedding.
    async fn step_has_embedding(&self, step_id: &str) -> Result<bool, HelixDbError>;

    /// Return the subset of `step_ids` that already have a semantic embedding.
    ///
    /// Default implementation issues one `step_has_embedding` call per ID.
    /// The `HelixNeo4j` backend overrides with a single `WHERE id IN $ids` query
    /// to replace N round-trips with one.
    async fn batch_step_ids_with_embeddings(
        &self,
        step_ids: &[String],
    ) -> Result<HashSet<String>, HelixDbError> {
        let mut result = HashSet::new();
        for id in step_ids {
            if self.step_has_embedding(id).await? {
                result.insert(id.clone());
            }
        }
        Ok(result)
    }

    /// Write a semantic embedding vector to a Step node.
    ///
    /// Uses `db.create.setNodeVectorProperty()` to trigger HNSW index updates.
    async fn set_step_embedding(
        &self,
        step_id: &str,
        embedding: &[f32],
    ) -> Result<(), HelixDbError>;

    /// Batch-upsert multiple [`Step`] nodes and their `HAS_STEP` relationships.
    ///
    /// Default implementation loops over [`upsert_step`].  Backends may override
    /// with a single UNWIND round-trip for 15× fewer Bolt calls.
    ///
    /// Returns one `(actual_id, was_created)` pair per input step, **in input order**.
    async fn batch_upsert_steps(
        &self,
        steps: &[Step],
    ) -> Result<Vec<(String, bool)>, HelixDbError> {
        let mut results = Vec::with_capacity(steps.len());
        for step in steps {
            results.push(self.upsert_step(step).await?);
        }
        Ok(results)
    }

    /// Batch-write semantic embedding vectors to Step nodes.
    ///
    /// Default implementation loops over [`set_step_embedding`].  Backends may
    /// override with a single UNWIND round-trip for 15× fewer Bolt calls.
    ///
    /// `items` is a slice of `(step_id, embedding)` pairs.
    async fn batch_set_embeddings(&self, items: &[(String, Vec<f32>)]) -> Result<(), HelixDbError> {
        for (step_id, embedding) in items {
            self.set_step_embedding(step_id, embedding).await?;
        }
        Ok(())
    }

    /// Phase 18 — upsert a Tier-1 ephemeral `:HotMemo` node.
    ///
    /// `MERGE` on `id` so repeated writes (NDJSON replay, retry-on-network)
    /// are idempotent. Properties are overwritten on every call so a later
    /// promotion pipeline can bump significance / extend strands without
    /// creating a second hot node.
    ///
    /// Default implementation returns `Ok(())` — backends without graph
    /// support silently no-op so webshell dual-write never errors out when
    /// the Neo4j tier is absent.
    async fn create_hot_memo(
        &self,
        _memo: &crate::helix::types::HotMemo,
    ) -> Result<(), HelixDbError> {
        Ok(())
    }

    /// Phase 18 — list hot memos, newest-first, filtered by TTL.
    ///
    /// The default implementation returns an empty vec; the `HelixNeo4j`
    /// backend runs a `MATCH (h:HotMemo) WHERE h.expires > datetime() …`
    /// query keyed off [`crate::helix::migrations`] v9 indexes.
    async fn query_hot_memos(
        &self,
        _sibling: Option<&str>,
        _limit: u32,
    ) -> Result<Vec<crate::helix::types::HotMemo>, HelixDbError> {
        Ok(Vec::new())
    }

    /// Batch-assign steps to a named strand — single UNWIND round-trip.
    ///
    /// Domain-agnostic: `strand_name` is caller-defined (e.g. `"preference"`,
    /// `"diagnosis"`, `"purchase"`). Default implementation loops; `HelixNeo4j`
    /// overrides with UNWIND MERGE for O(1) Bolt calls regardless of batch size.
    ///
    /// # Errors
    /// Returns `HelixDbError` if the strand or any step cannot be found.
    async fn batch_assign_strand(
        &self,
        strand_id: &str,
        step_ids: &[String],
        weight: f64,
    ) -> Result<(), HelixDbError> {
        for step_id in step_ids {
            self.assign_to_strand(&crate::helix::types::StrandMembership {
                step_id: step_id.clone(),
                strand_id: strand_id.to_owned(),
                weight,
            })
            .await?;
        }
        Ok(())
    }

    /// Query all step IDs in a helix that belong to a named strand.
    ///
    /// Domain-agnostic — strand names are caller-defined.
    /// Returns an empty `Vec` if the strand does not exist or has no members.
    ///
    /// # Errors
    /// Returns `HelixDbError` on graph query failure.
    async fn strand_step_ids(
        &self,
        helix_id: &str,
        strand_name: &str,
    ) -> Result<Vec<String>, HelixDbError> {
        let cypher = "MATCH (s:Step {helix_id: $helix_id})-[:MEMBER_OF]->(st:Strand {name: $name}) \
                      RETURN s.id AS id";
        let mut params = std::collections::BTreeMap::new();
        params.insert("helix_id".into(), serde_json::json!(helix_id));
        params.insert("name".into(), serde_json::json!(strand_name));
        let records = self.execute_cypher_with_params(cypher, params).await?;
        Ok(records
            .iter()
            .filter_map(|r| {
                r.fields
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(String::from)
            })
            .collect())
    }

    /// Query `SharedExperience` convergence clusters in a helix.
    ///
    /// Returns `(participant_step_ids, participant_count)` pairs for clusters
    /// with ≥ 2 members, ordered by participant count descending.
    ///
    /// Domain-agnostic: any domain can use convergence nodes to represent
    /// N-way agreement across steps (preferences, diagnoses, reviews, etc.).
    ///
    /// # Errors
    /// Returns `HelixDbError` on graph query failure.
    async fn convergence_clusters(
        &self,
        helix_id: &str,
    ) -> Result<Vec<(Vec<String>, usize)>, HelixDbError> {
        let cypher = "MATCH (s:Step {helix_id: $helix_id})-[:PARTICIPATES_IN]->(se:SharedExperience) \
                      WITH se, collect(s.id) AS step_ids, size(collect(s.id)) AS cnt \
                      WHERE cnt >= 2 \
                      RETURN step_ids, cnt \
                      ORDER BY cnt DESC";
        let mut params = std::collections::BTreeMap::new();
        params.insert("helix_id".into(), serde_json::json!(helix_id));
        let records = self.execute_cypher_with_params(cypher, params).await?;
        Ok(records
            .iter()
            .filter_map(|r| {
                let step_ids: Vec<String> = r
                    .fields
                    .get("step_ids")
                    .and_then(serde_json::Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })?;
                let count = r
                    .fields
                    .get("cnt")
                    .and_then(serde_json::Value::as_i64)
                    .map_or(0, |i| usize::try_from(i).unwrap_or(0));
                if count >= 2 {
                    Some((step_ids, count))
                } else {
                    None
                }
            })
            .collect())
    }

    /// Execute arbitrary Cypher (for GDS procedures, etc.).
    ///
    /// Returns the raw records. Use sparingly — prefer typed methods.
    async fn execute_cypher(
        &self,
        cypher: &str,
    ) -> Result<Vec<crate::helix::graph::Record>, HelixDbError>;

    /// Execute parameterized Cypher with named `$param` placeholders.
    ///
    /// All consolidator stages that query or mutate the graph should use this
    /// method to prevent Cypher injection. GDS procedure calls that take no
    /// user-supplied input may use [`execute_cypher`] instead.
    async fn execute_cypher_with_params(
        &self,
        cypher: &str,
        params: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<Vec<crate::helix::graph::Record>, HelixDbError>;

    /// Run pending schema migrations.
    async fn migrate(&self) -> Result<u32, HelixDbError>;

    /// Check database health.
    async fn health(&self) -> Result<crate::helix::graph::HealthStatus, HelixDbError>;
}

// ============================================================================
// HelixDbError
// ============================================================================

/// Helix database error.
#[derive(Debug, thiserror::Error)]
pub enum HelixDbError {
    /// Underlying graph engine error.
    #[error("Graph error: {0}")]
    Graph(#[from] crate::helix::graph::GraphError),

    /// Entity not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Validation error (e.g., `max_depth` exceeds global limit).
    #[error("Validation: {0}")]
    Validation(String),

    /// Configuration error.
    #[error("Config: {0}")]
    Config(String),
}

// ============================================================================
// Connection Config
// ============================================================================

/// Neo4j connection configuration.
///
/// Credentials stored as [`SecretString`] — never exposed in logs or debug output.
pub struct Neo4jConfig {
    /// Bolt URI (e.g., `bolt://localhost:7687`).
    pub uri: String,
    /// Neo4j username.
    pub user: String,
    /// Neo4j password (secret).
    pub password: SecretString,
}

impl Neo4jConfig {
    /// Load config from environment variables.
    ///
    /// - `NEO4J_URI` (default: `bolt://localhost:7687`)
    /// - `NEO4J_USER` (default: `neo4j`)
    /// - `NEO4J_PASS` (required)
    ///
    /// # Errors
    ///
    /// Returns [`HelixDbError::Config`] if `NEO4J_PASS` is not set.
    pub fn from_env() -> Result<Self, HelixDbError> {
        let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
        let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
        let password = std::env::var("NEO4J_PASS")
            .map_err(|_| HelixDbError::Config("NEO4J_PASS environment variable not set".into()))?;

        Ok(Self {
            uri,
            user,
            password: SecretString::from(password),
        })
    }
}

/// Unified helix configuration — database, cache, embedding, and paths.
///
/// Combines [`Neo4jConfig`] with cache and application settings.
/// Use [`HelixConfig::from_env`] for environment-variable-driven config.
pub struct HelixConfig {
    /// Neo4j connection settings.
    pub neo4j: Neo4jConfig,
    /// Path to the SOUL home directory (default: `~/.soul`).
    pub soul_home: PathBuf,
    /// Embedding model name for vector search (default: `nomic-embed-text`).
    pub embedding_model: String,
    /// Maximum cache entries (default: 1000).
    pub cache_capacity: u64,
    /// Cache entry time-to-live in seconds (default: 300).
    pub cache_ttl_seconds: u64,
}

impl HelixConfig {
    /// Load from environment variables with sensible defaults.
    ///
    /// - `NEO4J_URI`, `NEO4J_USER`, `NEO4J_PASS` — database connection
    /// - `SOUL_HOME` (default: `~/.soul`)
    /// - `HELIX_EMBEDDING_MODEL` (default: `nomic-embed-text`)
    /// - `HELIX_CACHE_CAPACITY` (default: `1000`)
    /// - `HELIX_CACHE_TTL` (default: `300`)
    ///
    /// # Errors
    ///
    /// Returns [`HelixDbError::Config`] if `NEO4J_PASS` is not set.
    pub fn from_env() -> Result<Self, HelixDbError> {
        let neo4j = Neo4jConfig::from_env()?;

        let soul_home = if let Ok(v) = std::env::var("SOUL_HOME") {
            PathBuf::from(v)
        } else {
            paths::soul_or_fallback()
        };

        let embedding_model =
            std::env::var("HELIX_EMBEDDING_MODEL").unwrap_or_else(|_| "nomic-embed-text".into());

        let cache_capacity = std::env::var("HELIX_CACHE_CAPACITY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000);

        let cache_ttl_seconds = std::env::var("HELIX_CACHE_TTL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);

        Ok(Self {
            neo4j,
            soul_home,
            embedding_model,
            cache_capacity,
            cache_ttl_seconds,
        })
    }

    /// Build a [`lightarchitects::helix::cache::HelixCacheConfig`] from this config.
    #[must_use]
    pub fn cache_config(&self) -> crate::helix::cache::HelixCacheConfig {
        crate::helix::cache::HelixCacheConfig::default()
            .with_max_capacity(self.cache_capacity)
            .with_ttl(Duration::from_secs(self.cache_ttl_seconds))
    }
}

// ============================================================================
// Label & Relationship Allowlists
// ============================================================================

/// Extended label allowlist for helix domain.
///
/// Includes graph-engine defaults plus helix-specific labels.
pub const HELIX_LABELS: &[&str] = &[
    // graph-engine defaults
    "Note",
    "HelixEntry",
    "Tag",
    "Strand",
    "Emotion",
    "Theme",
    "Journal",
    "SchemaMigration",
    // helix domain
    "Helix",
    "Step",
    "SharedExperience",
    "Source",
    "Attachment",
];

/// Extended relationship type allowlist for helix domain.
pub const HELIX_REL_TYPES: &[&str] = &[
    // graph-engine defaults
    "LINKS_TO",
    "HAS_TAG",
    "HAS_STRAND",
    "HAS_EMOTION",
    "HAS_THEME",
    "REFERENCES",
    "NEXT",
    "PREVIOUS",
    // helix domain
    "HAS_STEP",
    "HAS_SUB_HELIX",
    "IS_HELIX",
    "MEMBER_OF",
    "PARTICIPATES_IN",
    "HAS_ATTACHMENT",
    "CHUNK_OF",
    "INGESTED_FROM",
    // Phase 14.2 — typed sibling-output edges.
    // PLAN_FOR_BUILD:    plan → build (from a plan.md under corso/builds/<id>/)
    // REVIEWS_PLAN:      review/scrum-assessment → plan (from plan_ids field)
    // LESSON_FROM_ENTRY: lesson → source entry (from source_entry_id field)
    "PLAN_FOR_BUILD",
    "REVIEWS_PLAN",
    "LESSON_FROM_ENTRY",
    // Phase 18 — hot→cold lineage edge.
    // MATERIALIZED_FROM: step (cold) → hot_memo (promoted source).
    "MATERIALIZED_FROM",
];

// ============================================================================
// HelixNeo4j Implementation
// ============================================================================

/// Neo4j-backed helix database.
///
/// Wraps `graph-engine::Neo4jBackend` with extended label/`rel_type` allowlists
/// and helix-specific domain operations.
pub struct HelixNeo4j {
    backend: Neo4jBackend,
}

impl std::fmt::Debug for HelixNeo4j {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelixNeo4j").finish_non_exhaustive()
    }
}

impl HelixNeo4j {
    /// Connect to Neo4j with helix-domain configuration.
    ///
    /// Extends graph-engine's default allowlists with helix-specific
    /// labels and relationship types.
    ///
    /// # Errors
    ///
    /// Returns [`HelixDbError::Graph`] if the connection fails.
    #[instrument(skip(config))]
    pub async fn connect(config: &Neo4jConfig) -> Result<Self, HelixDbError> {
        let backend =
            Neo4jBackend::connect(&config.uri, &config.user, config.password.expose_secret())
                .await?
                .with_labels(HELIX_LABELS.iter().map(|&s| s.to_owned()).collect())
                .with_rel_types(HELIX_REL_TYPES.iter().map(|&s| s.to_owned()).collect());

        Ok(Self { backend })
    }

    /// Get a reference to the underlying graph backend.
    ///
    /// Useful for raw Cypher queries not covered by [`HelixDb`] methods.
    #[must_use]
    pub fn backend(&self) -> &Neo4jBackend {
        &self.backend
    }

    /// Execute a parameterized Cypher query with slow-query monitoring.
    ///
    /// Logs a WARN-level span when the query takes longer than
    /// [`SLOW_QUERY_THRESHOLD_MS`] (500ms). The warning includes the
    /// query template (without parameter values) and the elapsed duration.
    ///
    /// # Errors
    ///
    /// Returns [`HelixDbError::Graph`] if query execution fails.
    async fn timed_execute(
        &self,
        operation: &str,
        cypher: &str,
        params: BTreeMap<String, serde_json::Value>,
    ) -> Result<Vec<crate::helix::graph::Record>, HelixDbError> {
        let start = std::time::Instant::now();
        let result = self.backend.execute(cypher, params).await;
        let elapsed = start.elapsed();
        let duration_ms = elapsed.as_millis();

        if duration_ms > SLOW_QUERY_THRESHOLD_MS {
            // Truncate query to first 200 chars to avoid flooding logs
            let query_preview: String = cypher.chars().take(200).collect();
            tracing::warn!(
                neo4j.operation = operation,
                neo4j.query = %query_preview,
                duration_ms = %duration_ms,
                "Slow Neo4j query detected (>{SLOW_QUERY_THRESHOLD_MS}ms)"
            );
        } else {
            tracing::debug!(
                neo4j.operation = operation,
                duration_ms = %duration_ms,
                "Neo4j query completed"
            );
        }

        // Feed the training recorder — no-op when no session is active.
        crate::helix::training::record_cypher_call(operation, cypher, duration_ms);

        result.map_err(HelixDbError::from)
    }

    /// Report pool metrics (limited by neo4rs driver capabilities).
    ///
    /// neo4rs 0.8 does not expose connection pool statistics (active
    /// connections, idle connections, wait queue depth). The pool size
    /// is fixed at [`lightarchitects::helix::graph::DEFAULT_POOL_SIZE`].
    ///
    /// This method returns the configured pool size. If neo4rs adds
    /// pool introspection in a future version, this method will be
    /// extended to report live metrics.
    #[must_use]
    pub fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            max_connections: crate::helix::graph::DEFAULT_POOL_SIZE,
            // neo4rs 0.8 does not expose active/idle counts
            active_connections: None,
            idle_connections: None,
            // neo4rs 0.8 ConfigBuilder has no connection_timeout/wait_timeout
            connection_wait_timeout_ms: None,
        }
    }
}

/// Connection pool metrics (best-effort, limited by driver).
///
/// neo4rs 0.8 does not expose pool introspection. Fields are `Option`
/// to express what is and is not available from the driver.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PoolMetrics {
    /// Maximum pool size (configured at connect time).
    pub max_connections: u32,
    /// Currently active (checked-out) connections. `None` if driver
    /// does not expose this metric.
    pub active_connections: Option<u32>,
    /// Currently idle connections in the pool. `None` if driver
    /// does not expose this metric.
    pub idle_connections: Option<u32>,
    /// Connection acquisition timeout in milliseconds. `None` means
    /// the driver blocks indefinitely when the pool is exhausted.
    pub connection_wait_timeout_ms: Option<u64>,
}

#[async_trait]
impl HelixDb for HelixNeo4j {
    #[instrument(skip(self, helix), fields(neo4j.operation = "upsert_helix"))]
    async fn upsert_helix(&self, helix: &Helix) -> Result<String, HelixDbError> {
        let mut props = BTreeMap::new();
        props.insert("id".into(), serde_json::json!(&helix.id));
        props.insert("owner".into(), serde_json::json!(&helix.owner));
        props.insert("name".into(), serde_json::json!(&helix.name));
        props.insert("level".into(), serde_json::json!(helix.level));
        props.insert(
            "ordering_mode".into(),
            serde_json::json!(helix.ordering_mode.to_string()),
        );
        if let Some(md) = helix.max_depth {
            props.insert("max_depth".into(), serde_json::json!(md));
        }
        props.insert(
            "created_at".into(),
            serde_json::json!(helix.created_at.to_rfc3339()),
        );

        let id = self.backend.create_node(&["Helix".into()], props).await?;
        Ok(id)
    }

    #[instrument(skip(self), fields(neo4j.operation = "get_helix"))]
    async fn get_helix(&self, helix_id: &str) -> Result<Helix, HelixDbError> {
        let cypher = "MATCH (h:Helix {id: $id}) \
                      RETURN h.id AS id, h.owner AS owner, h.name AS name, \
                             h.level AS level, h.ordering_mode AS ordering_mode, \
                             h.max_depth AS max_depth, h.created_at AS created_at";
        let mut params = BTreeMap::new();
        params.insert("id".into(), serde_json::json!(helix_id));

        let records = self.timed_execute("get_helix", cypher, params).await?;
        let rec = records
            .first()
            .ok_or_else(|| HelixDbError::NotFound(format!("Helix not found: {helix_id}")))?;

        let ordering_mode = rec
            .get("ordering_mode")
            .and_then(serde_json::Value::as_str)
            .map_or(HelixOrderingMode::default(), |s| match s {
                "indexed" => HelixOrderingMode::Indexed,
                "custom" => HelixOrderingMode::Custom,
                _ => HelixOrderingMode::Temporal,
            });

        let created_at = rec
            .get("created_at")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map_or_else(chrono::Utc::now, |dt| dt.with_timezone(&chrono::Utc));

        Ok(Helix {
            id: rec
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(helix_id)
                .to_owned(),
            owner: rec
                .get("owner")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            name: rec
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            level: rec
                .get("level")
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(0),
            ordering_mode,
            max_depth: rec
                .get("max_depth")
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| u8::try_from(v).ok()),
            created_at,
        })
    }

    #[instrument(skip(self, step), fields(neo4j.operation = "create_step"))]
    async fn create_step(&self, step: &Step) -> Result<String, HelixDbError> {
        let mut props = BTreeMap::new();
        props.insert("id".into(), serde_json::json!(&step.id));
        props.insert("helix_id".into(), serde_json::json!(&step.helix_id));
        if let Some(ref title) = step.title {
            props.insert("title".into(), serde_json::json!(title));
        }
        props.insert("content".into(), serde_json::json!(&step.content));
        props.insert("significance".into(), serde_json::json!(step.significance));
        if let Some(date) = step.step_date {
            props.insert("step_date".into(), serde_json::json!(date.to_string()));
        }
        if let Some(idx) = step.step_index {
            props.insert("step_index".into(), serde_json::json!(idx));
        }
        if let Some(cid) = step.community_id {
            props.insert("community_id".into(), serde_json::json!(cid));
        }
        props.insert(
            "created_at".into(),
            serde_json::json!(step.created_at.to_rfc3339()),
        );
        if step.metadata != serde_json::Value::Null {
            props.insert(
                "metadata".into(),
                serde_json::json!(step.metadata.to_string()),
            );
        }

        let id = self.backend.create_node(&["Step".into()], props).await?;

        // Create HAS_STEP relationship to parent helix
        self.backend
            .create_relationship(&step.helix_id, &id, "HAS_STEP", BTreeMap::new())
            .await?;

        Ok(id)
    }

    #[instrument(skip(self), fields(neo4j.operation = "get_steps"))]
    async fn get_steps(
        &self,
        helix_id: &str,
        limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<Step>, HelixDbError> {
        // First get the helix ordering mode
        let helix_nodes = self
            .backend
            .find_nodes(
                "Helix",
                {
                    let mut f = BTreeMap::new();
                    f.insert("id".into(), serde_json::json!(helix_id));
                    f
                },
                Some(1),
            )
            .await?;

        let ordering_mode = helix_nodes
            .first()
            .and_then(|n| n.properties.get("ordering_mode"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("temporal");

        let order_clause = match ordering_mode {
            "indexed" => "s.step_index ASC, s.created_at ASC",
            "custom" => "s.metadata ASC, s.created_at ASC",
            _ => "s.step_date ASC, s.created_at ASC",
        };

        let capped = limit.unwrap_or(100);
        let cypher = format!(
            "MATCH (h:Helix {{id: $helix_id}})-[:HAS_STEP]->(s:Step) \
             RETURN s.id AS id, s.helix_id AS helix_id, s.title AS title, \
                    s.content AS content, s.significance AS significance, \
                    s.step_date AS step_date, s.step_index AS step_index, \
                    s.community_id AS community_id, s.expires AS expires, \
                    s.created_at AS created_at, s.metadata AS metadata \
             ORDER BY {order_clause} \
             LIMIT {capped}"
        );

        let mut params = BTreeMap::new();
        params.insert("helix_id".into(), serde_json::json!(helix_id));

        let records = self.timed_execute("get_steps", &cypher, params).await?;

        let mut steps = Vec::new();
        for record in records {
            if let Some(step) = Self::record_to_step(&record) {
                steps.push(step);
            }
        }

        Ok(steps)
    }

    #[instrument(skip(self, strand), fields(neo4j.operation = "create_strand"))]
    async fn create_strand(&self, strand: &Strand) -> Result<String, HelixDbError> {
        let mut props = BTreeMap::new();
        props.insert("id".into(), serde_json::json!(&strand.id));
        props.insert("name".into(), serde_json::json!(&strand.name));
        props.insert(
            "parent_helix_id".into(),
            serde_json::json!(&strand.parent_helix_id),
        );
        props.insert(
            "domain_helix_id".into(),
            serde_json::json!(&strand.domain_helix_id),
        );

        let id = self.backend.create_node(&["Strand".into()], props).await?;

        // HAS_STRAND from parent helix
        self.backend
            .create_relationship(&strand.parent_helix_id, &id, "HAS_STRAND", BTreeMap::new())
            .await?;

        // IS_HELIX to domain helix
        self.backend
            .create_relationship(&id, &strand.domain_helix_id, "IS_HELIX", BTreeMap::new())
            .await?;

        Ok(id)
    }

    #[instrument(skip(self, membership), fields(neo4j.operation = "assign_to_strand"))]
    async fn assign_to_strand(&self, membership: &StrandMembership) -> Result<(), HelixDbError> {
        let mut props = BTreeMap::new();
        props.insert("weight".into(), serde_json::json!(membership.weight));

        self.backend
            .create_relationship(
                &membership.step_id,
                &membership.strand_id,
                "MEMBER_OF",
                props,
            )
            .await?;

        Ok(())
    }

    #[instrument(skip(self, link), fields(neo4j.operation = "create_link"))]
    async fn create_link(&self, link: &HelixLink) -> Result<String, HelixDbError> {
        let rel_id = uuid::Uuid::new_v4().to_string();
        let link_type = link.link_type.to_string();
        let raw_wikilink = link.raw_wikilink.as_deref().unwrap_or("");
        let metadata_str = if link.metadata == serde_json::Value::Null {
            String::new()
        } else {
            link.metadata.to_string()
        };

        // Two-stage target resolution:
        //   Stage 1: match target by its UUID `id` property (normal steps).
        //   Stage 2: if stage 1 finds nothing, match by vault_path suffix
        //            (Obsidian wikilinks carry path slugs, not UUIDs).
        //
        // Obsidian wikilinks omit the `.md` extension (e.g. `[[eva/identity]]`)
        // while `Step.vault_path` always stores it (`"eva/identity.md"`).
        // Matching both variants in a single OPTIONAL MATCH keeps this a
        // single round-trip and covers already-suffixed references too.
        let cypher = "MATCH (a:Step {id: $source_id}) \
             OPTIONAL MATCH (b1:Step {id: $target_id}) \
             OPTIONAL MATCH (b2:Step) \
               WHERE b1 IS NULL \
                 AND b2.vault_path IS NOT NULL \
                 AND (b2.vault_path ENDS WITH $target_id \
                      OR b2.vault_path ENDS WITH $target_id_md) \
             WITH a, coalesce(b1, b2) AS b \
             WHERE b IS NOT NULL \
             MERGE (a)-[r:LINKS_TO]->(b) \
               ON CREATE SET r.id = $rel_id, \
                             r.link_type = $link_type, \
                             r.strength = $strength, \
                             r.raw_wikilink = $raw_wikilink, \
                             r.metadata = $metadata \
             RETURN r.id AS id";

        // Compute the `.md`-suffixed variant so Obsidian wikilinks like
        // `[[eva/identity]]` resolve against `vault_path = "eva/identity.md"`.
        let target_id_md = if std::path::Path::new(&link.target_id)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            link.target_id.clone()
        } else {
            format!("{}.md", link.target_id)
        };

        let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        params.insert("source_id".into(), serde_json::json!(&link.source_id));
        params.insert("target_id".into(), serde_json::json!(&link.target_id));
        params.insert("target_id_md".into(), serde_json::json!(&target_id_md));
        params.insert("rel_id".into(), serde_json::json!(&rel_id));
        params.insert("link_type".into(), serde_json::json!(link_type));
        params.insert("strength".into(), serde_json::json!(link.strength));
        params.insert("raw_wikilink".into(), serde_json::json!(raw_wikilink));
        params.insert("metadata".into(), serde_json::json!(metadata_str));

        let records = self.timed_execute("create_link", cypher, params).await?;

        records
            .first()
            .and_then(|r| r.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                HelixDbError::NotFound(format!(
                    "create_link: no matching target for '{}'",
                    link.target_id
                ))
            })
    }

    #[instrument(
        skip(self),
        fields(neo4j.operation = "create_typed_relationship", rel_type = %rel_type)
    )]
    async fn create_typed_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
    ) -> Result<String, HelixDbError> {
        // Defence-in-depth: refuse any label that isn't on the compile-time
        // allowlist. Neo4j doesn't accept relationship labels as bind
        // parameters, so `rel_type` MUST be interpolated into the Cypher
        // string — allowlist membership is the only thing standing between
        // this call site and Cypher injection.
        if !HELIX_REL_TYPES.contains(&rel_type) {
            return Err(HelixDbError::Validation(format!(
                "create_typed_relationship: rel_type '{rel_type}' not in HELIX_REL_TYPES allowlist"
            )));
        }

        let rel_id = uuid::Uuid::new_v4().to_string();

        // Three-variant target resolution for typed edges:
        //   1. UUID direct match (`b1` on id)
        //   2. Obsidian wikilink suffix (`ENDS WITH target_id` / `.md`)
        //   3. Phase 14.2 plan-slug shape: `.../{target_id}/plan.md`
        //      — this lets `plan_ids: [foo]` resolve to the canonical plan
        //      target at `corso/builds/foo/plan.md` without callers having
        //      to know the full path.
        let target_id_md = if std::path::Path::new(target_id)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            target_id.to_owned()
        } else {
            format!("{target_id}.md")
        };
        let target_plan_path = format!("/{target_id}/plan.md");

        let cypher = format!(
            "MATCH (a:Step {{id: $source_id}}) \
             OPTIONAL MATCH (b1:Step {{id: $target_id}}) \
             OPTIONAL MATCH (b2:Step) \
               WHERE b1 IS NULL \
                 AND b2.vault_path IS NOT NULL \
                 AND (b2.vault_path ENDS WITH $target_id \
                      OR b2.vault_path ENDS WITH $target_id_md \
                      OR b2.vault_path ENDS WITH $target_plan_path) \
             WITH a, coalesce(b1, b2) AS b \
             WHERE b IS NOT NULL \
             MERGE (a)-[r:{rel_type}]->(b) \
               ON CREATE SET r.id = $rel_id \
             RETURN r.id AS id"
        );

        let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        params.insert("source_id".into(), serde_json::json!(source_id));
        params.insert("target_id".into(), serde_json::json!(target_id));
        params.insert("target_id_md".into(), serde_json::json!(&target_id_md));
        params.insert(
            "target_plan_path".into(),
            serde_json::json!(&target_plan_path),
        );
        params.insert("rel_id".into(), serde_json::json!(&rel_id));

        let records = self
            .timed_execute("create_typed_relationship", &cypher, params)
            .await?;

        records
            .first()
            .and_then(|r| r.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                HelixDbError::NotFound(format!(
                    "create_typed_relationship: no matching target for '{target_id}' ({rel_type})"
                ))
            })
    }

    #[instrument(skip(self, experience, participant_step_ids), fields(neo4j.operation = "create_shared_experience"))]
    async fn create_shared_experience(
        &self,
        experience: &SharedExperience,
        participant_step_ids: &[String],
    ) -> Result<String, HelixDbError> {
        let mut props = BTreeMap::new();
        props.insert("id".into(), serde_json::json!(&experience.id));
        props.insert("weight".into(), serde_json::json!(experience.weight));
        props.insert(
            "participant_count".into(),
            serde_json::json!(experience.participant_count),
        );
        props.insert(
            "discovered_by".into(),
            serde_json::json!(experience.discovered_by.to_string()),
        );
        if let Some(ref label) = experience.label {
            props.insert("label".into(), serde_json::json!(label));
        }
        props.insert(
            "created_at".into(),
            serde_json::json!(experience.created_at.to_rfc3339()),
        );

        let id = self
            .backend
            .create_node(&["SharedExperience".into()], props)
            .await?;

        // Create PARTICIPATES_IN from each participant step
        for step_id in participant_step_ids {
            self.backend
                .create_relationship(step_id, &id, "PARTICIPATES_IN", BTreeMap::new())
                .await?;
        }

        Ok(id)
    }

    #[instrument(skip(self), fields(neo4j.operation = "query_convergences"))]
    async fn query_convergences(
        &self,
        helix_id: &str,
        min_participants: Option<usize>,
    ) -> Result<Vec<SharedExperience>, HelixDbError> {
        let min_p = min_participants.unwrap_or(2);
        let cypher = "MATCH (s:Step)-[:PARTICIPATES_IN]->(se:SharedExperience) \
                      WHERE s.helix_id = $helix_id AND se.participant_count >= $min_p \
                      RETURN DISTINCT se.id AS id, se.weight AS weight, \
                             se.participant_count AS participant_count, \
                             se.discovered_by AS discovered_by, \
                             se.label AS label, se.created_at AS created_at \
                      ORDER BY se.weight DESC";

        let mut params = BTreeMap::new();
        params.insert("helix_id".into(), serde_json::json!(helix_id));
        let min_p_i64 = i64::try_from(min_p).unwrap_or(i64::MAX);
        params.insert("min_p".into(), serde_json::json!(min_p_i64));

        let records = self
            .timed_execute("query_convergences", cypher, params)
            .await?;

        let mut experiences = Vec::new();
        for record in records {
            if let Some(se) = Self::record_to_shared_experience(&record) {
                experiences.push(se);
            }
        }

        Ok(experiences)
    }

    #[instrument(skip(self), fields(neo4j.operation = "drill_down"))]
    async fn drill_down(
        &self,
        step_id: &str,
        max_depth: u8,
        min_significance: Option<f64>,
    ) -> Result<Vec<Step>, HelixDbError> {
        let depth = max_depth.min(MAX_TRAVERSAL_DEPTH);
        let min_sig = min_significance.unwrap_or(0.0);

        let cypher = format!(
            "MATCH (parent:Step {{id: $step_id}})-[:HAS_SUB_HELIX]->(h:Helix)-[:HAS_STEP]->(child:Step) \
             WHERE child.significance >= $min_sig \
             RETURN child.id AS id, child.helix_id AS helix_id, child.title AS title, \
                    child.content AS content, child.significance AS significance, \
                    child.step_date AS step_date, child.step_index AS step_index, \
                    child.community_id AS community_id, child.expires AS expires, \
                    child.created_at AS created_at, child.metadata AS metadata \
             ORDER BY child.step_index ASC, child.created_at ASC \
             LIMIT {limit}",
            limit = u32::from(depth).saturating_mul(100)
        );

        let mut params = BTreeMap::new();
        params.insert("step_id".into(), serde_json::json!(step_id));
        params.insert("min_sig".into(), serde_json::json!(min_sig));

        let records = self.timed_execute("drill_down", &cypher, params).await?;

        let mut steps = Vec::new();
        for record in records {
            if let Some(step) = Self::record_to_step(&record) {
                steps.push(step);
            }
        }

        Ok(steps)
    }

    #[instrument(skip(self), fields(neo4j.operation = "find_backlinks"))]
    async fn find_backlinks(&self, step_id: &str) -> Result<Vec<Step>, HelixDbError> {
        let cypher = "MATCH (source:Step)-[:LINKS_TO]->(target:Step {id: $step_id}) \
                      RETURN source.id AS id, source.helix_id AS helix_id, \
                             source.title AS title, source.content AS content, \
                             source.significance AS significance, \
                             source.step_date AS step_date, source.step_index AS step_index, \
                             source.community_id AS community_id, source.expires AS expires, \
                             source.created_at AS created_at, source.metadata AS metadata";

        let mut params = BTreeMap::new();
        params.insert("step_id".into(), serde_json::json!(step_id));

        let records = self.timed_execute("find_backlinks", cypher, params).await?;

        let mut steps = Vec::new();
        for record in records {
            if let Some(step) = Self::record_to_step(&record) {
                steps.push(step);
            }
        }

        Ok(steps)
    }

    #[instrument(skip(self), fields(neo4j.operation = "get_or_create_day_step"))]
    async fn get_or_create_day_step(
        &self,
        helix_id: &str,
        date: chrono::NaiveDate,
    ) -> Result<String, HelixDbError> {
        let step_id = format!("{helix_id}/{date}");

        let cypher = "MERGE (s:Step {helix_id: $helix_id, step_date: $date}) \
                      ON CREATE SET s.id = $step_id, \
                                    s.content = '', \
                                    s.significance = 0.0, \
                                    s.created_at = datetime() \
                      RETURN s.id AS id";

        let mut params = BTreeMap::new();
        params.insert("helix_id".into(), serde_json::json!(helix_id));
        params.insert("date".into(), serde_json::json!(date.to_string()));
        params.insert("step_id".into(), serde_json::json!(&step_id));

        let records = self
            .timed_execute("get_or_create_day_step", cypher, params)
            .await?;

        records
            .first()
            .and_then(|r| r.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(String::from)
            .ok_or_else(|| HelixDbError::NotFound("Day step MERGE returned no ID".into()))
    }

    #[instrument(skip(self), fields(neo4j.operation = "fulltext_search"))]
    async fn fulltext_search(
        &self,
        query: &str,
        opts: &SearchOptions,
    ) -> Result<Vec<ScoredResult<Step>>, HelixDbError> {
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let index = index_names::STEP_FULLTEXT;
        let limit = opts.limit;

        // Lucene fulltext procedure → YIELD node, score → optional filters.
        // Optional filters use IS NULL pattern: if param is null, condition passes.
        let cypher = format!(
            "CALL db.index.fulltext.queryNodes('{index}', $query) YIELD node, score \
             WITH node, score \
             WHERE ($min_score IS NULL OR score >= $min_score) \
               AND ($helix_id IS NULL OR node.helix_id = $helix_id) \
               AND ($min_sig IS NULL OR node.significance >= $min_sig) \
               AND ($session_only IS NULL OR node.turn_role IS NULL) \
             RETURN node.id AS id, node.helix_id AS helix_id, node.title AS title, \
                    node.content AS content, node.significance AS significance, \
                    node.step_date AS step_date, node.step_index AS step_index, \
                    node.community_id AS community_id, node.expires AS expires, \
                    node.created_at AS created_at, node.metadata AS metadata, score \
             ORDER BY score DESC \
             LIMIT {limit}"
        );

        let mut params = Self::search_filter_params(opts);
        params.insert("query".into(), serde_json::json!(query));

        let records = self
            .timed_execute("fulltext_search", &cypher, params)
            .await?;
        Ok(Self::records_to_scored_steps(&records))
    }

    #[instrument(skip(self, embedding), fields(neo4j.operation = "vector_search"))]
    async fn vector_search(
        &self,
        embedding: &[f32],
        index_name: &str,
        opts: &SearchOptions,
    ) -> Result<Vec<ScoredResult<Step>>, HelixDbError> {
        // Validate index name against known constants.
        if index_name != index_names::STEP_EMBEDDINGS
            && index_name != index_names::STEP_STRUCT_EMBEDDINGS
        {
            return Err(HelixDbError::Validation(format!(
                "Unknown vector index: {index_name}"
            )));
        }

        if embedding.is_empty() {
            return Ok(Vec::new());
        }

        // When helix_id is specified, use brute-force cosine on just the helix's steps.
        // This avoids the global HNSW → post-filter problem where 498/500 helixes are
        // dropped, leaving ~1 result per helix. With ~50 steps per helix, brute-force
        // cosine is O(50) and guarantees all results belong to the target helix.
        //
        // When helix_id is None, fall back to global HNSW (the original behavior).
        let cypher = if opts.helix_id.is_some() {
            format!(
                "MATCH (s:Step {{helix_id: $helix_id}}) \
                 WHERE s.{embedding_prop} IS NOT NULL \
                   AND ($min_score IS NULL OR true) \
                   AND ($min_sig IS NULL OR s.significance >= $min_sig) \
                 WITH s, vector.similarity.cosine(s.{embedding_prop}, $embedding) AS score \
                 ORDER BY score DESC \
                 LIMIT $k \
                 RETURN s.id AS id, s.helix_id AS helix_id, s.title AS title, \
                        s.content AS content, s.significance AS significance, \
                        s.step_date AS step_date, s.step_index AS step_index, \
                        s.community_id AS community_id, s.expires AS expires, \
                        s.created_at AS created_at, s.metadata AS metadata, score",
                embedding_prop = if index_name == index_names::STEP_EMBEDDINGS {
                    "embedding"
                } else {
                    "struct_embedding"
                }
            )
        } else {
            format!(
                "CALL db.index.vector.queryNodes('{index_name}', $k, $embedding) YIELD node, score \
                 WITH node, score \
                 WHERE ($min_score IS NULL OR score >= $min_score) \
                   AND ($helix_id IS NULL OR node.helix_id = $helix_id) \
                   AND ($min_sig IS NULL OR node.significance >= $min_sig) \
                 RETURN node.id AS id, node.helix_id AS helix_id, node.title AS title, \
                        node.content AS content, node.significance AS significance, \
                        node.step_date AS step_date, node.step_index AS step_index, \
                        node.community_id AS community_id, node.expires AS expires, \
                        node.created_at AS created_at, node.metadata AS metadata, score \
                 ORDER BY score DESC"
            )
        };

        let mut params = Self::search_filter_params(opts);
        // Convert embedding slice to JSON array of f64 (Neo4j expects float list).
        let embedding_json: Vec<serde_json::Value> = embedding
            .iter()
            .map(|&v| serde_json::json!(f64::from(v)))
            .collect();
        params.insert("embedding".into(), serde_json::json!(embedding_json));
        params.insert("k".into(), serde_json::json!(i64::from(opts.limit)));

        let records = self.timed_execute("vector_search", &cypher, params).await?;
        Ok(Self::records_to_scored_steps(&records))
    }

    #[instrument(skip(self), fields(neo4j.operation = "ensure_helix"))]
    async fn ensure_helix(
        &self,
        owner: &str,
        name: &str,
        ordering_mode: crate::helix::types::HelixOrderingMode,
    ) -> Result<String, HelixDbError> {
        let id = format!("{owner}/{name}");
        let cypher = "MERGE (h:Helix {owner: $owner, name: $name}) \
                      ON CREATE SET h.id = $id, \
                                    h.level = 0, \
                                    h.ordering_mode = $mode, \
                                    h.created_at = datetime() \
                      RETURN h.id AS id";

        let mut params = BTreeMap::new();
        params.insert("owner".into(), serde_json::json!(owner));
        params.insert("name".into(), serde_json::json!(name));
        params.insert("id".into(), serde_json::json!(&id));
        params.insert("mode".into(), serde_json::json!(ordering_mode.to_string()));

        let records = self.timed_execute("ensure_helix", cypher, params).await?;
        records
            .first()
            .and_then(|r| r.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(String::from)
            .ok_or_else(|| HelixDbError::NotFound("Helix MERGE returned no ID".into()))
    }

    #[instrument(skip(self, step), fields(neo4j.operation = "upsert_step"))]
    async fn upsert_step(&self, step: &Step) -> Result<(String, bool), HelixDbError> {
        let content_hash = Self::content_hash(&step.content);
        let cypher = "MERGE (s:Step {content_hash: $hash, helix_id: $helix_id}) \
                      ON CREATE SET s.id = $id, s.title = $title, \
                                    s.content = $content, s.significance = $sig, \
                                    s.step_date = $step_date, s.step_index = $step_index, \
                                    s.expires = $expires, s.entry_type = $entry_type, \
                                    s.vault_path = $vault_path, \
                                    s.created_at = datetime(), s._created = true \
                      ON MATCH SET  s.title = $title, s.significance = $sig, \
                                    s.step_index = $step_index, s.expires = $expires, \
                                    s.entry_type = $entry_type, \
                                    s.vault_path = $vault_path, \
                                    s._created = false \
                      RETURN s.id AS id, s._created AS created";

        let mut params = BTreeMap::new();
        params.insert("hash".into(), serde_json::json!(&content_hash));
        params.insert("helix_id".into(), serde_json::json!(&step.helix_id));
        params.insert("id".into(), serde_json::json!(&step.id));
        params.insert("title".into(), Self::opt_json(step.title.as_deref()));
        params.insert("content".into(), serde_json::json!(&step.content));
        params.insert("sig".into(), serde_json::json!(step.significance));
        params.insert("step_date".into(), Self::opt_date_json(step.step_date));
        params.insert("step_index".into(), Self::opt_i64_json(step.step_index));
        params.insert(
            "expires".into(),
            step.expires.map_or(serde_json::Value::Null, |exp| {
                serde_json::json!(exp.to_rfc3339())
            }),
        );
        // entry_type is stored in Step.metadata by the ingestion pipeline and mirrored
        // here as a first-class Neo4j property to enable Cypher queries like:
        // MATCH (s:Step {entry_type: 'build_plan'}) RETURN s
        params.insert(
            "entry_type".into(),
            step.metadata
                .get("entry_type")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        );
        // vault_path enables wikilink slug resolution in create_link.
        // Null for steps created outside the markdown vault pipeline.
        params.insert(
            "vault_path".into(),
            step.vault_path
                .as_deref()
                .map_or(serde_json::Value::Null, |p| serde_json::json!(p)),
        );

        let records = self.timed_execute("upsert_step", cypher, params).await?;
        let record = records
            .first()
            .ok_or_else(|| HelixDbError::NotFound("Step MERGE returned no result".into()))?;

        let id = record
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&step.id)
            .to_owned();
        let was_created = record
            .get("created")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        // Ensure the HAS_STEP relationship to the parent Helix exists.
        // MERGE is idempotent — safe on both create and update paths.
        let rel_cypher = "MATCH (h:Helix {id: $helix_id}), (s:Step {id: $step_id}) \
                          MERGE (h)-[:HAS_STEP]->(s)";
        let mut rel_params = BTreeMap::new();
        rel_params.insert("helix_id".into(), serde_json::json!(&step.helix_id));
        rel_params.insert("step_id".into(), serde_json::json!(&id));
        self.timed_execute("upsert_step_rel", rel_cypher, rel_params)
            .await?;

        Ok((id, was_created))
    }

    #[instrument(skip(self), fields(neo4j.operation = "ensure_strand"))]
    async fn ensure_strand(
        &self,
        parent_helix_id: &str,
        name: &str,
    ) -> Result<String, HelixDbError> {
        let strand_id = format!("{parent_helix_id}/strand/{name}");
        let domain_id = format!("{parent_helix_id}/strand/{name}/helix");

        let cypher = "MERGE (st:Strand {parent_helix_id: $parent, name: $name}) \
                      ON CREATE SET st.id = $strand_id, \
                                    st.domain_helix_id = $domain_id \
                      RETURN st.id AS id";

        let mut params = BTreeMap::new();
        params.insert("parent".into(), serde_json::json!(parent_helix_id));
        params.insert("name".into(), serde_json::json!(name));
        params.insert("strand_id".into(), serde_json::json!(&strand_id));
        params.insert("domain_id".into(), serde_json::json!(&domain_id));

        let records = self.timed_execute("ensure_strand", cypher, params).await?;
        records
            .first()
            .and_then(|r| r.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(String::from)
            .ok_or_else(|| HelixDbError::NotFound("Strand MERGE returned no ID".into()))
    }

    #[instrument(skip(self, source), fields(neo4j.operation = "register_source"))]
    async fn register_source(
        &self,
        source: &crate::helix::types::SourceWatermark,
    ) -> Result<String, HelixDbError> {
        let mut props = BTreeMap::new();
        props.insert("id".into(), serde_json::json!(&source.id));
        props.insert("source_type".into(), serde_json::json!(&source.source_type));
        props.insert("path".into(), serde_json::json!(&source.path));
        props.insert(
            "last_ingested_at".into(),
            serde_json::json!(source.last_ingested_at.to_rfc3339()),
        );
        if let Some(ref hash) = source.content_hash {
            props.insert("content_hash".into(), serde_json::json!(hash));
        }
        props.insert(
            "record_count".into(),
            serde_json::json!(source.record_count),
        );

        let id = self.backend.create_node(&["Source".into()], props).await?;
        Ok(id)
    }

    #[instrument(skip(self), fields(neo4j.operation = "update_source_watermark"))]
    async fn update_source_watermark(
        &self,
        source_id: &str,
        last_ingested_at: DateTime<chrono::Utc>,
        record_count: u64,
    ) -> Result<(), HelixDbError> {
        let cypher = "MATCH (src:Source {id: $source_id}) \
                      SET src.last_ingested_at = $ts, src.record_count = $count";

        let mut params = BTreeMap::new();
        params.insert("source_id".into(), serde_json::json!(source_id));
        params.insert(
            "ts".into(),
            serde_json::json!(last_ingested_at.to_rfc3339()),
        );
        params.insert("count".into(), serde_json::json!(record_count));

        self.timed_execute("update_source_watermark", cypher, params)
            .await?;
        Ok(())
    }

    #[instrument(skip(self, profile), fields(neo4j.operation = "write_personality"))]
    async fn write_personality(
        &self,
        helix_id: &str,
        profile: &crate::helix::types::PersonalityProfile,
    ) -> Result<(), HelixDbError> {
        let profile_json = serde_json::to_string(profile)
            .map_err(|e| HelixDbError::Validation(format!("Serialize personality: {e}")))?;

        let cypher = "MATCH (h:Helix {id: $helix_id}) \
                      SET h.personality = $profile";

        let mut params = BTreeMap::new();
        params.insert("helix_id".into(), serde_json::json!(helix_id));
        params.insert("profile".into(), serde_json::json!(&profile_json));

        self.timed_execute("write_personality", cypher, params)
            .await?;
        Ok(())
    }

    #[instrument(skip(self), fields(neo4j.operation = "step_has_embedding"))]
    async fn step_has_embedding(&self, step_id: &str) -> Result<bool, HelixDbError> {
        let cypher = "MATCH (s:Step {id: $step_id}) \
                      RETURN s.embedding IS NOT NULL AS has_embedding";
        let mut params = BTreeMap::new();
        params.insert("step_id".into(), serde_json::json!(step_id));

        let records = self
            .timed_execute("step_has_embedding", cypher, params)
            .await?;
        let has = records
            .first()
            .and_then(|r| r.get("has_embedding"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        Ok(has)
    }

    #[instrument(skip(self, step_ids), fields(neo4j.operation = "batch_step_ids_with_embeddings", count = step_ids.len()))]
    async fn batch_step_ids_with_embeddings(
        &self,
        step_ids: &[String],
    ) -> Result<HashSet<String>, HelixDbError> {
        if step_ids.is_empty() {
            return Ok(HashSet::new());
        }
        let cypher = "MATCH (s:Step) \
                      WHERE s.id IN $ids AND s.embedding IS NOT NULL \
                      RETURN s.id AS id";
        let ids_json: Vec<serde_json::Value> =
            step_ids.iter().map(|id| serde_json::json!(id)).collect();
        let mut params = BTreeMap::new();
        params.insert("ids".into(), serde_json::json!(ids_json));

        let records = self
            .timed_execute("batch_step_ids_with_embeddings", cypher, params)
            .await?;
        Ok(records
            .iter()
            .filter_map(|r| {
                r.get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(String::from)
            })
            .collect())
    }

    #[instrument(skip(self, embedding), fields(neo4j.operation = "set_step_embedding", dims = embedding.len()))]
    async fn set_step_embedding(
        &self,
        step_id: &str,
        embedding: &[f32],
    ) -> Result<(), HelixDbError> {
        // Use setNodeVectorProperty to trigger HNSW index update.
        let cypher = "MATCH (s:Step {id: $step_id}) \
                      CALL db.create.setNodeVectorProperty(s, 'embedding', $vector)";
        let embedding_json: Vec<serde_json::Value> = embedding
            .iter()
            .map(|&v| serde_json::json!(f64::from(v)))
            .collect();

        let mut params = BTreeMap::new();
        params.insert("step_id".into(), serde_json::json!(step_id));
        params.insert("vector".into(), serde_json::json!(embedding_json));

        self.timed_execute("set_step_embedding", cypher, params)
            .await?;
        Ok(())
    }

    #[instrument(skip(self, memo), fields(neo4j.operation = "create_hot_memo", id = %memo.id))]
    async fn create_hot_memo(
        &self,
        memo: &crate::helix::types::HotMemo,
    ) -> Result<(), HelixDbError> {
        // Chain fields: prev_seq is NULL for genesis memos (seq=0) so the
        // OPTIONAL MATCH finds nothing and the FOREACH is a safe no-op.
        let prev_seq: serde_json::Value = memo
            .seq
            .checked_sub(1)
            .and_then(|n| i64::try_from(n).ok())
            .map_or(serde_json::Value::Null, |n| serde_json::json!(n));
        let cypher = "MERGE (h:HotMemo {id: $id}) \
             SET h.sibling = $sibling, \
                 h.content = $content, \
                 h.significance = $significance, \
                 h.strands = $strands, \
                 h.created_at = datetime($created_at), \
                 h.expires = datetime($expires), \
                 h.session_id = $session_id, \
                 h.seq = $seq, \
                 h.hmac_prev = $hmac_prev, \
                 h.hmac_self = $hmac_self \
             WITH h \
             OPTIONAL MATCH (prev:HotMemo {session_id: $session_id, seq: $prev_seq}) \
             FOREACH (p IN CASE WHEN prev IS NOT NULL THEN [prev] ELSE [] END | \
               MERGE (p)-[:NEXT]->(h))";
        let strands_json: Vec<serde_json::Value> =
            memo.strands.iter().map(|s| serde_json::json!(s)).collect();
        let mut params = BTreeMap::new();
        params.insert("id".into(), serde_json::json!(memo.id));
        params.insert("sibling".into(), serde_json::json!(memo.sibling));
        params.insert("content".into(), serde_json::json!(memo.content));
        params.insert("significance".into(), serde_json::json!(memo.significance));
        params.insert("strands".into(), serde_json::json!(strands_json));
        params.insert(
            "created_at".into(),
            serde_json::json!(memo.created_at.to_rfc3339()),
        );
        params.insert(
            "expires".into(),
            serde_json::json!(memo.expires.to_rfc3339()),
        );
        params.insert("session_id".into(), serde_json::json!(memo.session_id));
        params.insert(
            "seq".into(),
            i64::try_from(memo.seq).map_or(serde_json::Value::Null, |n| serde_json::json!(n)),
        );
        params.insert("prev_seq".into(), prev_seq);
        params.insert(
            "hmac_prev".into(),
            memo.hmac_prev
                .as_deref()
                .map_or(serde_json::Value::Null, |s| serde_json::json!(s)),
        );
        params.insert(
            "hmac_self".into(),
            memo.hmac_self
                .as_deref()
                .map_or(serde_json::Value::Null, |s| serde_json::json!(s)),
        );
        self.timed_execute("create_hot_memo", cypher, params)
            .await?;
        Ok(())
    }

    #[instrument(skip(self), fields(neo4j.operation = "query_hot_memos", limit = limit))]
    async fn query_hot_memos(
        &self,
        sibling: Option<&str>,
        limit: u32,
    ) -> Result<Vec<crate::helix::types::HotMemo>, HelixDbError> {
        // Filter by sibling when supplied; always gate on the TTL so expired
        // memos drop out of the list without requiring a compaction pass.
        let cypher = "MATCH (h:HotMemo) \
             WHERE h.expires > datetime() \
               AND ($sibling IS NULL OR h.sibling = $sibling) \
             RETURN h.id AS id, h.sibling AS sibling, h.content AS content, \
                    h.significance AS significance, h.strands AS strands, \
                    toString(h.created_at) AS created_at, \
                    toString(h.expires) AS expires, \
                    coalesce(h.session_id, '') AS session_id, \
                    coalesce(h.seq, 0) AS seq, \
                    h.hmac_prev AS hmac_prev, \
                    h.hmac_self AS hmac_self \
             ORDER BY h.created_at DESC \
             LIMIT $limit";
        let mut params = BTreeMap::new();
        params.insert(
            "sibling".into(),
            sibling.map_or(serde_json::Value::Null, |s| serde_json::json!(s)),
        );
        params.insert("limit".into(), serde_json::json!(i64::from(limit)));

        let records = self
            .timed_execute("query_hot_memos", cypher, params)
            .await?;
        let mut out = Vec::with_capacity(records.len());
        for r in records {
            let Some(id) = r.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(sibling) = r.get("sibling").and_then(|v| v.as_str()) else {
                continue;
            };
            let content = r
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            let significance = r
                .get("significance")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let strands: Vec<String> = r
                .get("strands")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let created_at = r
                .get("created_at")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));
            let expires = r
                .get("expires")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));
            let (Some(created_at), Some(expires)) = (created_at, expires) else {
                continue;
            };
            let session_id = r
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            let seq = r
                .get("seq")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let hmac_prev = r
                .get("hmac_prev")
                .and_then(|v| v.as_str())
                .map(String::from);
            let hmac_self = r
                .get("hmac_self")
                .and_then(|v| v.as_str())
                .map(String::from);
            out.push(crate::helix::types::HotMemo {
                id: id.to_owned(),
                sibling: sibling.to_owned(),
                content,
                significance,
                strands,
                created_at,
                expires,
                session_id,
                seq,
                hmac_prev,
                hmac_self,
            });
        }
        Ok(out)
    }

    #[instrument(skip(self, steps), fields(neo4j.operation = "batch_upsert_steps", count = steps.len()))]
    async fn batch_upsert_steps(
        &self,
        steps: &[Step],
    ) -> Result<Vec<(String, bool)>, HelixDbError> {
        if steps.is_empty() {
            return Ok(Vec::new());
        }
        // Single UNWIND round-trip: MERGE all steps + HAS_STEP relationships.
        let cypher = "UNWIND $steps AS s \
                      MERGE (node:Step {content_hash: s.hash, helix_id: s.helix_id}) \
                      ON CREATE SET node.id = s.id, node.title = s.title, \
                                    node.content = s.content, node.significance = s.sig, \
                                    node.step_date = s.step_date, node.step_index = s.step_index, \
                                    node.expires = s.expires, \
                                    node.created_at = datetime(), node._created = true \
                      ON MATCH SET  node.title = s.title, node.significance = s.sig, \
                                    node.step_index = s.step_index, node.expires = s.expires, \
                                    node._created = false \
                      WITH node, s \
                      MATCH (h:Helix {id: s.helix_id}) \
                      MERGE (h)-[:HAS_STEP]->(node) \
                      RETURN s.id AS input_id, node.id AS actual_id, node._created AS created";

        let steps_json: Vec<serde_json::Value> = steps
            .iter()
            .map(|step| {
                serde_json::json!({
                    "hash":       Self::content_hash(&step.content),
                    "helix_id":   step.helix_id,
                    "id":         step.id,
                    "title":      step.title,
                    "content":    step.content,
                    "sig":        step.significance,
                    "step_date":  Self::opt_date_json(step.step_date),
                    "step_index": Self::opt_i64_json(step.step_index),
                    "expires":    step.expires.map_or(serde_json::Value::Null,
                                      |exp| serde_json::json!(exp.to_rfc3339())),
                })
            })
            .collect();

        let mut params = BTreeMap::new();
        params.insert("steps".into(), serde_json::json!(steps_json));

        let records = self
            .timed_execute("batch_upsert_steps", cypher, params)
            .await?;

        // Build a lookup so we can return results in the same order as `steps`.
        let id_map: HashMap<String, (String, bool)> = records
            .iter()
            .filter_map(|r| {
                let input_id = r.get("input_id")?.as_str()?.to_owned();
                let actual_id = r.get("actual_id")?.as_str()?.to_owned();
                let created = r
                    .get("created")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                Some((input_id, (actual_id, created)))
            })
            .collect();

        Ok(steps
            .iter()
            .map(|step| {
                id_map
                    .get(&step.id)
                    .cloned()
                    .unwrap_or_else(|| (step.id.clone(), false))
            })
            .collect())
    }

    #[instrument(skip(self, items), fields(neo4j.operation = "batch_set_embeddings", count = items.len()))]
    async fn batch_set_embeddings(&self, items: &[(String, Vec<f32>)]) -> Result<(), HelixDbError> {
        if items.is_empty() {
            return Ok(());
        }
        // Single UNWIND round-trip: set all vector properties in one query.
        let cypher = "UNWIND $items AS item \
                      MATCH (s:Step {id: item.step_id}) \
                      CALL db.create.setNodeVectorProperty(s, 'embedding', item.vector)";

        let items_json: Vec<serde_json::Value> = items
            .iter()
            .map(|(step_id, embedding)| {
                let vector: Vec<serde_json::Value> = embedding
                    .iter()
                    .map(|&v| serde_json::json!(f64::from(v)))
                    .collect();
                serde_json::json!({ "step_id": step_id, "vector": vector })
            })
            .collect();

        let mut params = BTreeMap::new();
        params.insert("items".into(), serde_json::json!(items_json));

        self.timed_execute("batch_set_embeddings", cypher, params)
            .await?;
        Ok(())
    }

    #[instrument(skip(self, step_ids), fields(neo4j.operation = "batch_assign_strand", count = step_ids.len()))]
    async fn batch_assign_strand(
        &self,
        strand_id: &str,
        step_ids: &[String],
        weight: f64,
    ) -> Result<(), HelixDbError> {
        if step_ids.is_empty() {
            return Ok(());
        }
        // Single UNWIND MERGE round-trip — idempotent, safe to re-run.
        let cypher = "UNWIND $step_ids AS sid \
                      MATCH (s:Step {id: sid}) \
                      MATCH (st:Strand {id: $strand_id}) \
                      MERGE (s)-[r:MEMBER_OF]->(st) \
                      ON CREATE SET r.weight = $weight";
        let mut params = BTreeMap::new();
        params.insert("step_ids".into(), serde_json::json!(step_ids));
        params.insert("strand_id".into(), serde_json::json!(strand_id));
        params.insert("weight".into(), serde_json::json!(weight));
        self.timed_execute("batch_assign_strand", cypher, params)
            .await?;
        Ok(())
    }

    #[instrument(skip(self), fields(neo4j.operation = "execute_cypher"))]
    async fn execute_cypher(
        &self,
        cypher: &str,
    ) -> Result<Vec<crate::helix::graph::Record>, HelixDbError> {
        let params = BTreeMap::new();
        let records = self.timed_execute("execute_cypher", cypher, params).await?;
        Ok(records)
    }

    #[instrument(skip(self, params), fields(neo4j.operation = "execute_cypher_with_params"))]
    async fn execute_cypher_with_params(
        &self,
        cypher: &str,
        params: BTreeMap<String, serde_json::Value>,
    ) -> Result<Vec<crate::helix::graph::Record>, HelixDbError> {
        let records = self
            .timed_execute("execute_cypher_with_params", cypher, params)
            .await?;
        Ok(records)
    }

    #[instrument(skip(self), fields(neo4j.operation = "migrate"))]
    async fn migrate(&self) -> Result<u32, HelixDbError> {
        use crate::helix::graph::schema;

        // Phase 1: Apply graph-engine core migrations (v1-v2)
        let core_count = self.backend.migrate().await?;

        // Phase 2: Validate helix migrations
        crate::helix::migrations::validate_helix_migrations()
            .map_err(|e| HelixDbError::Config(format!("Invalid helix migrations: {e}")))?;

        // Phase 3: Query all applied versions (core + helix)
        let applied_cypher = schema::list_applied_cypher();
        let applied_params = BTreeMap::new();
        let rows = self.backend.execute(applied_cypher, applied_params).await?;
        let mut applied_versions: Vec<u32> = Vec::new();
        for row in &rows {
            if let Some(version) = row.get("version") {
                if let Some(v) = version.as_i64() {
                    if let Ok(u) = u32::try_from(v) {
                        applied_versions.push(u);
                    }
                }
            }
        }

        // Phase 4: Apply pending helix migrations (v3-v5)
        let pending = crate::helix::migrations::helix_pending_migrations(&applied_versions);
        let helix_count = pending.len();

        for migration in pending {
            tracing::info!(
                version = migration.version,
                desc = migration.description,
                statements = migration.statements.len(),
                "Applying helix migration"
            );

            for stmt in migration.statements {
                let params = BTreeMap::new();
                self.backend.execute(stmt, params).await.map_err(|e| {
                    HelixDbError::Graph(crate::helix::graph::GraphError::Schema(format!(
                        "Helix migration v{} statement failed: {e}",
                        migration.version
                    )))
                })?;
            }

            // Record the migration as applied
            let record_cypher = schema::record_migration_cypher();
            let mut record_params = BTreeMap::new();
            record_params.insert(
                "version".into(),
                serde_json::json!(i64::from(migration.version)),
            );
            record_params.insert(
                "applied_at".into(),
                serde_json::json!(chrono::Utc::now().to_rfc3339()),
            );
            record_params.insert(
                "description".into(),
                serde_json::json!(migration.description),
            );
            self.backend
                .execute(record_cypher, record_params)
                .await
                .map_err(|e| {
                    HelixDbError::Graph(crate::helix::graph::GraphError::Schema(format!(
                        "Failed to record helix migration v{}: {e}",
                        migration.version
                    )))
                })?;
        }

        let helix_applied = u32::try_from(helix_count).unwrap_or(u32::MAX);
        let total = core_count.saturating_add(helix_applied);
        tracing::info!(
            core = core_count,
            helix = helix_count,
            total,
            "All schema migrations complete"
        );
        Ok(total)
    }

    #[instrument(skip(self), fields(neo4j.operation = "health"))]
    async fn health(&self) -> Result<crate::helix::graph::HealthStatus, HelixDbError> {
        let status = self.backend.health().await?;
        Ok(status)
    }
}

// ============================================================================
// Record Conversion Helpers
// ============================================================================

impl HelixNeo4j {
    /// Compute SHA-256 hex digest of content for dedup.
    ///
    /// Public so that write-through callers (e.g., `soul-mcp`) can compute
    /// the same hash the consolidator uses, ensuring content-hash dedup
    /// works across both write-through and batch ingestion paths.
    #[must_use]
    pub fn content_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Convert optional string to JSON value (null if None).
    fn opt_json(val: Option<&str>) -> serde_json::Value {
        val.map_or(serde_json::Value::Null, |v| serde_json::json!(v))
    }

    /// Convert `Option<NaiveDate>` to JSON value (null if None).
    fn opt_date_json(val: Option<chrono::NaiveDate>) -> serde_json::Value {
        val.map_or(serde_json::Value::Null, |d| {
            serde_json::json!(d.to_string())
        })
    }

    /// Convert `Option<i64>` to JSON value (null if None).
    fn opt_i64_json(val: Option<i64>) -> serde_json::Value {
        val.map_or(serde_json::Value::Null, |v| serde_json::json!(v))
    }

    /// Build nullable filter params from `SearchOptions` for Cypher IS NULL pattern.
    fn search_filter_params(opts: &SearchOptions) -> BTreeMap<String, serde_json::Value> {
        let mut params = BTreeMap::new();
        params.insert(
            "min_score".into(),
            opts.min_score
                .map_or(serde_json::Value::Null, |s| serde_json::json!(s)),
        );
        params.insert(
            "helix_id".into(),
            opts.helix_id
                .as_deref()
                .map_or(serde_json::Value::Null, |h| serde_json::json!(h)),
        );
        params.insert(
            "min_sig".into(),
            opts.min_significance
                .map_or(serde_json::Value::Null, |s| serde_json::json!(s)),
        );
        params.insert(
            "session_only".into(),
            if opts.session_only {
                serde_json::json!(true)
            } else {
                serde_json::Value::Null
            },
        );
        params
    }

    /// Map a list of graph records to scored Step results.
    fn records_to_scored_steps(records: &[crate::helix::graph::Record]) -> Vec<ScoredResult<Step>> {
        records
            .iter()
            .filter_map(|record| {
                let step = Self::record_to_step(record)?;
                let score = record
                    .get("score")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                Some(ScoredResult::new(step, score))
            })
            .collect()
    }

    fn record_to_step(record: &crate::helix::graph::Record) -> Option<Step> {
        let id = record.get("id")?.as_str()?.to_owned();
        let helix_id = record
            .get("helix_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned();
        let title = record
            .get("title")
            .and_then(serde_json::Value::as_str)
            .map(String::from);
        let content = record
            .get("content")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned();
        let significance = record
            .get("significance")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let step_date = record
            .get("step_date")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| s.parse::<chrono::NaiveDate>().ok());
        let step_index = record.get("step_index").and_then(serde_json::Value::as_i64);
        let community_id = record
            .get("community_id")
            .and_then(serde_json::Value::as_i64);
        let expires = record
            .get("expires")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));
        let created_at = record
            .get("created_at")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map_or_else(chrono::Utc::now, |dt| dt.with_timezone(&chrono::Utc));
        let metadata = record
            .get("metadata")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Null);
        let vault_path = record
            .get("vault_path")
            .and_then(serde_json::Value::as_str)
            .map(String::from);

        Some(Step {
            id,
            helix_id,
            title,
            content,
            significance,
            step_date,
            step_index,
            community_id,
            expires,
            created_at,
            metadata,
            vault_path,
        })
    }

    fn record_to_shared_experience(
        record: &crate::helix::graph::Record,
    ) -> Option<SharedExperience> {
        let id = record.get("id")?.as_str()?.to_owned();
        let weight = record
            .get("weight")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let participant_count = record
            .get("participant_count")
            .and_then(serde_json::Value::as_u64)
            .map_or(0, |v| usize::try_from(v).unwrap_or(usize::MAX));
        let discovered_by = record
            .get("discovered_by")
            .and_then(serde_json::Value::as_str)
            .map_or(DiscoveryMethod::Explicit, |s| match s {
                "louvain" => DiscoveryMethod::Louvain,
                "embedding_similarity" => DiscoveryMethod::EmbeddingSimilarity,
                _ => DiscoveryMethod::Explicit,
            });
        let label = record
            .get("label")
            .and_then(serde_json::Value::as_str)
            .map(String::from);
        let created_at = record
            .get("created_at")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map_or_else(chrono::Utc::now, |dt| dt.with_timezone(&chrono::Utc));

        Some(SharedExperience {
            id,
            weight,
            participant_count,
            discovered_by,
            label,
            created_at,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_helix_labels_include_all_required() {
        let required = [
            "Helix",
            "Step",
            "Strand",
            "SharedExperience",
            "Source",
            "Attachment",
        ];
        for label in required {
            assert!(
                HELIX_LABELS.contains(&label),
                "Missing helix label: {label}"
            );
        }
    }

    #[test]
    fn test_helix_rel_types_include_all_required() {
        let required = [
            "HAS_STEP",
            "HAS_SUB_HELIX",
            "HAS_STRAND",
            "IS_HELIX",
            "MEMBER_OF",
            "PARTICIPATES_IN",
            "LINKS_TO",
            "HAS_ATTACHMENT",
            "CHUNK_OF",
            "INGESTED_FROM",
        ];
        for rt in required {
            assert!(
                HELIX_REL_TYPES.contains(&rt),
                "Missing helix rel type: {rt}"
            );
        }
    }

    #[test]
    fn test_neo4j_config_env_key() {
        // Verify that the error type is Config (not connection etc.) when NEO4J_PASS missing.
        // We can't remove env vars in Rust 2024 without unsafe, so we test the
        // HelixDbError::Config variant directly.
        let err = HelixDbError::Config("NEO4J_PASS environment variable not set".into());
        assert!(err.to_string().contains("NEO4J_PASS"));
    }

    #[test]
    fn test_record_to_step_minimal() {
        let mut fields = BTreeMap::new();
        fields.insert("id".into(), serde_json::json!("step-1"));
        fields.insert("content".into(), serde_json::json!("hello"));
        let record = crate::helix::graph::Record { fields };

        let step = HelixNeo4j::record_to_step(&record);
        assert!(step.is_some());
        let step = step.expect("step");
        assert_eq!(step.id, "step-1");
        assert_eq!(step.content, "hello");
    }

    #[test]
    fn test_record_to_step_missing_id() {
        let fields = BTreeMap::new();
        let record = crate::helix::graph::Record { fields };
        assert!(HelixNeo4j::record_to_step(&record).is_none());
    }
}
