//! Graph engine — dual-backend knowledge graph.
//!
//! Absorbed from the `graph-engine` crate in SOUL-DEV. Provides a
//! [`GraphStore`] trait abstracting graph operations across multiple
//! backends: Neo4j CE and markdown files (Obsidian-compatible).
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │          GraphStore trait                │
//! ├──────────┬──────────┬───────────────────┤
//! │ Neo4j    │ File     │ Dual              │
//! │ Backend  │ Backend  │ Backend           │
//! │ (Bolt)   │ (markdown)│ (synced)          │
//! └──────────┴──────────┴───────────────────┘
//! ```
//!
//! ## Feature Flags
//!
//! - `file` (default) — Enables [`FileBackend`](file::FileBackend)
//! - `neo4j` — Enables [`Neo4jBackend`](neo4j::Neo4jBackend) via neo4rs
//! - `dual` — Enables [`DualBackend`](dual::DualBackend) (requires both)

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Backend Modules ──────────────────────────────────────────────────────────

#[cfg(feature = "neo4j")]
pub mod neo4j;

#[cfg(feature = "file")]
pub mod file;

#[cfg(feature = "dual")]
pub mod dual;

/// Always available — no feature gate needed.
pub mod validation;

#[cfg(feature = "neo4j")]
pub mod schema;

// ── Error Types ───────────────────────────────────────────────────────────────

/// Graph engine error hierarchy.
///
/// Independent from SDK errors — conversion happens at integration boundaries.
#[derive(Error, Debug)]
pub enum GraphError {
    /// Failed to connect to the graph backend.
    #[error("Connection error: {0}")]
    Connection(String),

    /// Query execution failed.
    #[error("Query error: {0}")]
    Query(String),

    /// Requested entity not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Input failed validation (e.g., label not in allowlist).
    #[error("Validation error: {0}")]
    Validation(String),

    /// Operation not supported by this backend.
    #[error("Unsupported operation: {0}")]
    Unsupported(String),

    /// Schema migration error.
    #[error("Schema error: {0}")]
    Schema(String),

    /// Filesystem I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Convenience type alias for graph operations.
pub type GraphResult<T> = std::result::Result<T, GraphError>;

// ── Core Types ────────────────────────────────────────────────────────────────

/// A graph node with labels and properties.
///
/// `id` is the application-level identifier (e.g., vault file path),
/// NOT the Neo4j internal integer ID (unstable across restores).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Application-level unique identifier.
    pub id: String,
    /// Node labels (e.g., `["Note", "HelixEntry"]`).
    pub labels: Vec<String>,
    /// Arbitrary key-value properties.
    pub properties: BTreeMap<String, serde_json::Value>,
}

/// A directed edge between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Edge identifier (backend-assigned or derived).
    pub id: String,
    /// Relationship type (e.g., `"LINKS_TO"`, `"HAS_TAG"`).
    pub rel_type: String,
    /// Application-level ID of the source node.
    pub from_id: String,
    /// Application-level ID of the target node.
    pub to_id: String,
    /// Arbitrary key-value properties.
    pub properties: BTreeMap<String, serde_json::Value>,
}

/// A subgraph — the result of a traversal or pattern match.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubGraph {
    /// Nodes in the subgraph.
    pub nodes: Vec<Node>,
    /// Edges in the subgraph.
    pub edges: Vec<Edge>,
}

/// A single row from a query result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Field values keyed by column name.
    pub fields: BTreeMap<String, serde_json::Value>,
}

/// Backend health and basic statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Whether the backend is reachable.
    pub connected: bool,
    /// Backend type identifier (e.g., `"neo4j"`, `"file"`, `"dual"`).
    pub backend: String,
    /// Total node count (if available without full scan).
    pub node_count: Option<u64>,
    /// Total edge count (if available without full scan).
    pub edge_count: Option<u64>,
    /// Health check latency in milliseconds.
    pub latency_ms: Option<u64>,
    /// Backend-specific details.
    pub details: BTreeMap<String, serde_json::Value>,
}

// ── Batch Operation Types ─────────────────────────────────────────────────────

/// A node to create in a batch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchNode {
    /// Node labels.
    pub labels: Vec<String>,
    /// Node properties (must include a unique key for MERGE).
    pub properties: BTreeMap<String, serde_json::Value>,
}

/// A relationship to create in a batch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRelationship {
    /// Application-level ID of the source node.
    pub from_id: String,
    /// Application-level ID of the target node.
    pub to_id: String,
    /// Relationship type (must be in the allowlist).
    pub rel_type: String,
    /// Relationship properties.
    pub properties: BTreeMap<String, serde_json::Value>,
}

/// Result of a batch operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchResult {
    /// Number of entities successfully created/merged.
    pub created: u64,
    /// Errors encountered during batch processing.
    pub errors: Vec<String>,
}

// ── GraphStore Trait ──────────────────────────────────────────────────────────

/// Abstract graph store supporting multiple backends.
///
/// All operations use application-level IDs, not database-internal IDs.
///
/// # Security
///
/// - All query parameters MUST be passed via the `params` map, never
///   interpolated into query strings.
/// - Labels and relationship types MUST be validated against allowlists
///   before use in Cypher (see [`validation`] module).
#[async_trait]
pub trait GraphStore: Send + Sync {
    /// Execute a parameterized query.
    ///
    /// For Neo4j: executes Cypher with `$param` substitution.
    /// For `FileBackend`: returns `Err(GraphError::Unsupported)`.
    async fn execute(
        &self,
        query: &str,
        params: BTreeMap<String, serde_json::Value>,
    ) -> GraphResult<Vec<Record>>;

    /// Find nodes by label and optional property filters.
    ///
    /// When `limit` is `None`, a default cap of [`DEFAULT_TRAVERSAL_LIMIT`]
    /// is applied to prevent unbounded result sets.
    async fn find_nodes(
        &self,
        label: &str,
        filters: BTreeMap<String, serde_json::Value>,
        limit: Option<u32>,
    ) -> GraphResult<Vec<Node>>;

    /// Traverse from a starting node through specified relationship types.
    ///
    /// - `from_id`: Application-level ID of the starting node.
    /// - `rel_types`: Relationship types to follow (empty = all types).
    /// - `depth`: Maximum traversal depth (hops).
    /// - `limit`: Maximum nodes to return (capped at [`MAX_TRAVERSAL_LIMIT`]).
    async fn traverse(
        &self,
        from_id: &str,
        rel_types: &[String],
        depth: u32,
        limit: u32,
    ) -> GraphResult<SubGraph>;

    /// Create a node with labels and properties. Idempotent (MERGE semantics).
    ///
    /// Returns the application-level ID of the created or existing node.
    async fn create_node(
        &self,
        labels: &[String],
        props: BTreeMap<String, serde_json::Value>,
    ) -> GraphResult<String>;

    /// Create a relationship between two nodes. Idempotent (MERGE semantics).
    ///
    /// Returns a relationship identifier.
    async fn create_relationship(
        &self,
        from: &str,
        to: &str,
        rel_type: &str,
        props: BTreeMap<String, serde_json::Value>,
    ) -> GraphResult<String>;

    /// Delete a node by application-level ID.
    async fn delete_node(&self, id: &str) -> GraphResult<()>;

    /// Check backend health and return basic statistics.
    async fn health(&self) -> GraphResult<HealthStatus>;

    /// Batch create/upsert nodes in a single transaction.
    async fn batch_create_nodes(&self, nodes: Vec<BatchNode>) -> GraphResult<BatchResult>;

    /// Batch create/upsert relationships in a single transaction.
    async fn batch_create_relationships(
        &self,
        rels: Vec<BatchRelationship>,
    ) -> GraphResult<BatchResult>;
}

// ── Constants ─────────────────────────────────────────────────────────────────

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default connection pool size.
///
/// Increased from 4 to 20 for `LongMemEval` workloads where
/// `QUESTION_CONCURRENCY=5` parallel checkpoint queries would exhaust a pool of 4,
/// causing spurious `.ok()` failures that triggered unnecessary re-extraction.
pub const DEFAULT_POOL_SIZE: u32 = 20;

/// Default traversal result limit.
pub const DEFAULT_TRAVERSAL_LIMIT: u32 = 100;

/// Maximum traversal result limit.
pub const MAX_TRAVERSAL_LIMIT: u32 = 1000;

/// Default batch size for transactional batching.
pub const DEFAULT_BATCH_SIZE: usize = 5000;

// ── Helpers ───────────────────────────────────────────────────────────────────

impl Node {
    /// Create a new node with the given ID and labels.
    #[must_use]
    pub fn new(id: impl Into<String>, labels: Vec<String>) -> Self {
        Self {
            id: id.into(),
            labels,
            properties: BTreeMap::new(),
        }
    }

    /// Add a property to this node, returning the modified node.
    #[must_use]
    pub fn with_property(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.properties.insert(key.into(), value);
        self
    }
}

impl SubGraph {
    /// Returns true if the subgraph contains no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns the total number of nodes in the subgraph.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the total number of edges in the subgraph.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

impl Record {
    /// Get a field value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.fields.get(key)
    }
}

impl BatchResult {
    /// Returns true if the batch completed with no errors.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}
