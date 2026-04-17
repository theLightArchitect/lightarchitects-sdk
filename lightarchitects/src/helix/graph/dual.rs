//! Dual-backend — synchronized File + Neo4j.
//!
//! The [`DualBackend`] writes to both backends and uses Neo4j as the
//! primary query engine (it has indexes and traversal support). The
//! file backend serves as the source-of-truth for markdown authoring
//! and as a fallback when Neo4j is unavailable.
//!
//! # Write Strategy
//!
//! All mutations (create, delete) go to **both** backends in sequence:
//! file first (safe local write), then Neo4j. If Neo4j fails, the
//! operation succeeds with a logged warning — the startup sync will
//! catch up later.
//!
//! # Read Strategy
//!
//! Queries go to Neo4j when connected (it has indexes and Cypher).
//! If Neo4j is unavailable, queries fall back to the file backend.
//!
//! # Startup Sync
//!
//! On connect, [`DualBackend::sync()`] walks the file backend's index
//! and ensures all nodes exist in Neo4j with matching content hashes.
//! Stale nodes are updated; missing nodes are created.

use std::collections::BTreeMap;

use async_trait::async_trait;
use tracing::instrument;

use super::file::FileBackend;
use super::neo4j::Neo4jBackend;
use super::validation::DEFAULT_LABELS;
use super::{
    BatchNode, BatchRelationship, BatchResult, GraphError, GraphResult, GraphStore, HealthStatus,
    Node, Record, SubGraph,
};

// ============================================================================
// DualBackend
// ============================================================================

/// Synchronized dual-backend: File (source-of-truth) + Neo4j (query engine).
///
/// Writes go to both backends. Reads prefer Neo4j, fall back to File.
pub struct DualBackend {
    file: FileBackend,
    neo4j: Neo4jBackend,
    /// Whether Neo4j is currently reachable.
    neo4j_available: std::sync::atomic::AtomicBool,
}

impl std::fmt::Debug for DualBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DualBackend")
            .field("file", &self.file)
            .field("neo4j", &self.neo4j)
            .field(
                "neo4j_available",
                &self
                    .neo4j_available
                    .load(std::sync::atomic::Ordering::Relaxed),
            )
            .finish()
    }
}

impl DualBackend {
    /// Create a dual backend from pre-connected file and Neo4j backends.
    ///
    /// After construction, call [`sync()`](Self::sync) to ensure consistency.
    #[must_use]
    pub fn new(file: FileBackend, neo4j: Neo4jBackend) -> Self {
        Self {
            file,
            neo4j,
            neo4j_available: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Check if Neo4j is currently available.
    fn is_neo4j_available(&self) -> bool {
        self.neo4j_available
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Mark Neo4j as unavailable after a failure.
    fn mark_neo4j_down(&self) {
        self.neo4j_available
            .store(false, std::sync::atomic::Ordering::Relaxed);
        tracing::warn!("Neo4j marked as unavailable — falling back to file backend");
    }

    /// Mark Neo4j as available after recovery.
    fn mark_neo4j_up(&self) {
        self.neo4j_available
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Synchronize the file backend's state into Neo4j.
    ///
    /// Walks all nodes in the file index, checks if they exist in Neo4j
    /// with matching content hashes, and creates/updates as needed.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::Connection`] if Neo4j is unreachable.
    /// Individual node sync failures are logged but don't abort the process.
    #[instrument(skip(self))]
    pub async fn sync(&self) -> GraphResult<SyncReport> {
        // Verify Neo4j is reachable
        match self.neo4j.health().await {
            Ok(_) => self.mark_neo4j_up(),
            Err(e) => {
                self.mark_neo4j_down();
                return Err(GraphError::Connection(format!(
                    "Cannot sync — Neo4j unavailable: {e}"
                )));
            }
        }

        // Get all nodes from the file backend
        let file_health = self.file.health().await?;
        let total_nodes = file_health.node_count.unwrap_or(0);

        tracing::info!(total_nodes, "Starting file → Neo4j sync");

        let mut created: u64 = 0;
        let mut updated: u64 = 0;
        let mut skipped: u64 = 0;
        let mut errors: Vec<String> = Vec::new();

        // Sync each label type (derived from DEFAULT_LABELS, excluding Neo4j-internal labels)
        for &label in DEFAULT_LABELS {
            if label == "SchemaMigration" {
                continue;
            }
            let file_nodes = self.file.find_nodes(label, BTreeMap::new(), None).await?;

            for node in &file_nodes {
                match self.sync_node(node).await {
                    Ok(SyncAction::Created) => created += 1,
                    Ok(SyncAction::Updated) => updated += 1,
                    Ok(SyncAction::Skipped) => skipped += 1,
                    Err(e) => {
                        errors.push(format!("Sync '{}': {e}", node.id));
                        tracing::warn!(node_id = %node.id, error = %e, "Node sync failed");
                    }
                }
            }
        }

        // Sync relationships using the file backend's edge data
        let file_edges = self.file.all_edges().await;
        let edges_synced = if file_edges.is_empty() {
            0
        } else {
            let edge_count = file_edges.len();
            tracing::info!(edges = edge_count, "Syncing relationships to Neo4j");
            match self.neo4j.batch_create_relationships(file_edges).await {
                Ok(result) => {
                    if !result.is_clean() {
                        for err in &result.errors {
                            errors.push(format!("Edge sync: {err}"));
                        }
                    }
                    result.created
                }
                Err(e) => {
                    errors.push(format!("Edge batch sync failed: {e}"));
                    tracing::warn!(error = %e, "Edge sync failed");
                    0
                }
            }
        };

        let report = SyncReport {
            created,
            updated,
            skipped,
            edges_synced,
            errors,
        };

        tracing::info!(
            created = report.created,
            updated = report.updated,
            skipped = report.skipped,
            edges_synced = report.edges_synced,
            errors = report.errors.len(),
            "Sync complete"
        );

        Ok(report)
    }

    /// Sync a single node from file to Neo4j.
    ///
    /// Compares content hashes to decide create vs update vs skip.
    async fn sync_node(&self, file_node: &Node) -> GraphResult<SyncAction> {
        // Check if the node exists in Neo4j
        let mut filters = BTreeMap::new();
        filters.insert("id".into(), serde_json::json!(&file_node.id));

        let label = file_node.labels.first().map_or("Note", String::as_str);
        let existing = self.neo4j.find_nodes(label, filters, Some(1)).await?;

        if let Some(neo4j_node) = existing.first() {
            // Compare content hashes
            let file_hash = file_node.properties.get("content_hash");
            let neo4j_hash = neo4j_node.properties.get("content_hash");

            if file_hash == neo4j_hash && file_hash.is_some() {
                return Ok(SyncAction::Skipped);
            }

            // Update: create_node uses MERGE semantics — it updates if exists
            self.neo4j
                .create_node(&file_node.labels, file_node.properties.clone())
                .await?;
            Ok(SyncAction::Updated)
        } else {
            // Create
            self.neo4j
                .create_node(&file_node.labels, file_node.properties.clone())
                .await?;
            Ok(SyncAction::Created)
        }
    }

    /// Run schema migrations on the Neo4j backend.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::Schema`] if migrations fail.
    pub async fn migrate(&self) -> GraphResult<u32> {
        self.neo4j.migrate().await
    }
}

/// Result of a sync operation.
#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    /// Nodes created in Neo4j.
    pub created: u64,
    /// Nodes updated in Neo4j (content hash changed).
    pub updated: u64,
    /// Nodes skipped (already in sync).
    pub skipped: u64,
    /// Relationships synced to Neo4j via batch MERGE.
    pub edges_synced: u64,
    /// Errors encountered during sync.
    pub errors: Vec<String>,
}

impl SyncReport {
    /// Returns true if the sync completed with no errors.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Internal sync action for a single node.
enum SyncAction {
    Created,
    Updated,
    Skipped,
}

// ============================================================================
// GraphStore Implementation
// ============================================================================

#[async_trait]
impl GraphStore for DualBackend {
    #[instrument(skip(self, params))]
    async fn execute(
        &self,
        cypher: &str,
        params: BTreeMap<String, serde_json::Value>,
    ) -> GraphResult<Vec<Record>> {
        // Raw Cypher only goes to Neo4j — file backend doesn't support it
        if self.is_neo4j_available() {
            match self.neo4j.execute(cypher, params).await {
                Ok(records) => return Ok(records),
                Err(e) => {
                    self.mark_neo4j_down();
                    tracing::warn!(error = %e, "Neo4j execute failed, no file fallback for raw Cypher");
                    return Err(e);
                }
            }
        }

        Err(GraphError::Unsupported(
            "Raw Cypher not available — Neo4j is down and file backend does not support it".into(),
        ))
    }

    #[instrument(skip(self, filters))]
    async fn find_nodes(
        &self,
        label: &str,
        filters: BTreeMap<String, serde_json::Value>,
        limit: Option<u32>,
    ) -> GraphResult<Vec<Node>> {
        // Prefer Neo4j (indexed), fall back to file
        if self.is_neo4j_available() {
            match self.neo4j.find_nodes(label, filters.clone(), limit).await {
                Ok(nodes) => return Ok(nodes),
                Err(e) => {
                    self.mark_neo4j_down();
                    tracing::warn!(error = %e, "Neo4j find_nodes failed, falling back to file");
                }
            }
        }

        self.file.find_nodes(label, filters, limit).await
    }

    #[instrument(skip(self))]
    async fn traverse(
        &self,
        from_id: &str,
        rel_types: &[String],
        depth: u32,
        limit: u32,
    ) -> GraphResult<SubGraph> {
        // Prefer Neo4j (native traversal), fall back to file (BFS)
        if self.is_neo4j_available() {
            match self.neo4j.traverse(from_id, rel_types, depth, limit).await {
                Ok(sg) => return Ok(sg),
                Err(e) => {
                    self.mark_neo4j_down();
                    tracing::warn!(error = %e, "Neo4j traverse failed, falling back to file");
                }
            }
        }

        self.file.traverse(from_id, rel_types, depth, limit).await
    }

    #[instrument(skip(self, props))]
    async fn create_node(
        &self,
        labels: &[String],
        props: BTreeMap<String, serde_json::Value>,
    ) -> GraphResult<String> {
        // Write to file first (safe local write)
        let id = self.file.create_node(labels, props.clone()).await?;

        // Then write to Neo4j (best-effort if unavailable)
        if self.is_neo4j_available() {
            if let Err(e) = self.neo4j.create_node(labels, props).await {
                self.mark_neo4j_down();
                tracing::warn!(error = %e, node_id = %id, "Neo4j create_node failed — will sync later");
            }
        }

        Ok(id)
    }

    #[instrument(skip(self, props))]
    async fn create_relationship(
        &self,
        from: &str,
        to: &str,
        rel_type: &str,
        props: BTreeMap<String, serde_json::Value>,
    ) -> GraphResult<String> {
        // Write to file first
        let id = self
            .file
            .create_relationship(from, to, rel_type, props.clone())
            .await?;

        // Then Neo4j (best-effort)
        if self.is_neo4j_available() {
            if let Err(e) = self
                .neo4j
                .create_relationship(from, to, rel_type, props)
                .await
            {
                self.mark_neo4j_down();
                tracing::warn!(error = %e, "Neo4j create_relationship failed — will sync later");
            }
        }

        Ok(id)
    }

    #[instrument(skip(self))]
    async fn delete_node(&self, id: &str) -> GraphResult<()> {
        // Delete from file first
        self.file.delete_node(id).await?;

        // Then Neo4j (best-effort)
        if self.is_neo4j_available() {
            if let Err(e) = self.neo4j.delete_node(id).await {
                self.mark_neo4j_down();
                tracing::warn!(error = %e, node_id = %id, "Neo4j delete_node failed — will sync later");
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn health(&self) -> GraphResult<HealthStatus> {
        let file_health = self.file.health().await?;

        let neo4j_status = if self.is_neo4j_available() {
            match self.neo4j.health().await {
                Ok(h) => {
                    self.mark_neo4j_up();
                    Some(h)
                }
                Err(e) => {
                    self.mark_neo4j_down();
                    tracing::warn!(error = %e, "Neo4j health check failed");
                    None
                }
            }
        } else {
            None
        };

        let mut details = BTreeMap::new();
        details.insert(
            "file_connected".into(),
            serde_json::json!(file_health.connected),
        );
        details.insert(
            "file_nodes".into(),
            serde_json::json!(file_health.node_count),
        );
        details.insert(
            "neo4j_connected".into(),
            serde_json::json!(neo4j_status.is_some()),
        );

        if let Some(ref neo4j) = neo4j_status {
            details.insert("neo4j_nodes".into(), serde_json::json!(neo4j.node_count));
            details.insert(
                "neo4j_latency_ms".into(),
                serde_json::json!(neo4j.latency_ms),
            );
            if let Some(version) = neo4j.details.get("version") {
                details.insert("neo4j_version".into(), version.clone());
            }
        }

        Ok(HealthStatus {
            connected: file_health.connected,
            backend: "dual".into(),
            node_count: file_health.node_count,
            edge_count: file_health.edge_count,
            latency_ms: neo4j_status.as_ref().and_then(|h| h.latency_ms).or(Some(0)),
            details,
        })
    }

    #[instrument(skip(self, nodes))]
    async fn batch_create_nodes(&self, nodes: Vec<BatchNode>) -> GraphResult<BatchResult> {
        // Write to file first
        let file_result = self.file.batch_create_nodes(nodes.clone()).await?;

        // Then Neo4j (best-effort)
        if self.is_neo4j_available() {
            if let Err(e) = self.neo4j.batch_create_nodes(nodes).await {
                self.mark_neo4j_down();
                tracing::warn!(error = %e, "Neo4j batch_create_nodes failed — will sync later");
            }
        }

        Ok(file_result)
    }

    #[instrument(skip(self, rels))]
    async fn batch_create_relationships(
        &self,
        rels: Vec<BatchRelationship>,
    ) -> GraphResult<BatchResult> {
        // Write to file first
        let file_result = self.file.batch_create_relationships(rels.clone()).await?;

        // Then Neo4j (best-effort)
        if self.is_neo4j_available() {
            if let Err(e) = self.neo4j.batch_create_relationships(rels).await {
                self.mark_neo4j_down();
                tracing::warn!(error = %e, "Neo4j batch_create_relationships failed — will sync later");
            }
        }

        Ok(file_result)
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
    fn test_sync_report_clean() {
        let report = SyncReport {
            created: 10,
            updated: 5,
            skipped: 85,
            edges_synced: 0,
            errors: vec![],
        };
        assert!(report.is_clean());
    }

    #[test]
    fn test_sync_report_dirty() {
        let report = SyncReport {
            created: 9,
            updated: 4,
            skipped: 80,
            edges_synced: 0,
            errors: vec!["node X failed".into()],
        };
        assert!(!report.is_clean());
    }

    #[test]
    fn test_sync_report_default() {
        let report = SyncReport::default();
        assert_eq!(report.created, 0);
        assert_eq!(report.updated, 0);
        assert_eq!(report.skipped, 0);
        assert!(report.is_clean());
    }
}
