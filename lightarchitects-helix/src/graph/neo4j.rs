//! Neo4j backend via Bolt protocol.
//!
//! Implements [`GraphStore`](crate::GraphStore) using [`neo4rs`] for
//! Neo4j Community Edition. All queries use parameterized Cypher —
//! labels and relationship types are validated against allowlists
//! before interpolation (they cannot be parameterized in Cypher).
//!
//! # Connection
//!
//! ```text
//! Neo4jBackend::connect("bolt://localhost:7687", "neo4j", "password")
//! ```
//!
//! # Batch Operations
//!
//! Bulk inserts use transactional batching to minimize round-trips.
//! Batch size is configurable via [`crate::DEFAULT_BATCH_SIZE`].

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use neo4rs::{BoltType, ConfigBuilder, Graph, Query, query};
use tracing::instrument;

use super::schema::{self, Migration};
use super::validation::{self, DEFAULT_LABELS, DEFAULT_REL_TYPES};
use super::{
    BatchNode, BatchRelationship, BatchResult, DEFAULT_BATCH_SIZE, DEFAULT_POOL_SIZE,
    DEFAULT_TRAVERSAL_LIMIT, Edge, GraphError, GraphResult, GraphStore, HealthStatus,
    MAX_TRAVERSAL_LIMIT, Node, Record, SubGraph,
};

// ============================================================================
// Neo4jBackend
// ============================================================================

/// Neo4j Community Edition backend using Bolt protocol.
///
/// Wraps a [`neo4rs::Graph`] connection pool. All Cypher queries use
/// parameter substitution (`$param`) for values. Labels and relationship
/// types are validated against configurable allowlists before string
/// interpolation.
pub struct Neo4jBackend {
    graph: Graph,
    allowed_labels: Vec<String>,
    allowed_rel_types: Vec<String>,
}

impl std::fmt::Debug for Neo4jBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Neo4jBackend")
            .field("allowed_labels", &self.allowed_labels)
            .field("allowed_rel_types", &self.allowed_rel_types)
            .finish_non_exhaustive()
    }
}

impl Neo4jBackend {
    /// Connect to a Neo4j instance.
    ///
    /// # Arguments
    ///
    /// - `uri` — Bolt URI (e.g., `"bolt://localhost:7687"`)
    /// - `user` — Neo4j username
    /// - `password` — Neo4j password
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::Connection`] if the connection fails.
    #[instrument(skip(password))]
    pub async fn connect(uri: &str, user: &str, password: &str) -> GraphResult<Self> {
        let config = ConfigBuilder::default()
            .uri(uri)
            .user(user)
            .password(password)
            .max_connections(DEFAULT_POOL_SIZE as usize)
            .build()
            .map_err(|e| GraphError::Connection(format!("Config error: {e}")))?;

        let graph = Graph::connect(config)
            .await
            .map_err(|e| GraphError::Connection(format!("Connection failed: {e}")))?;

        Ok(Self {
            graph,
            allowed_labels: DEFAULT_LABELS.iter().map(|&s| s.to_owned()).collect(),
            allowed_rel_types: DEFAULT_REL_TYPES.iter().map(|&s| s.to_owned()).collect(),
        })
    }

    /// Override the default label allowlist.
    #[must_use]
    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.allowed_labels = labels;
        self
    }

    /// Override the default relationship type allowlist.
    #[must_use]
    pub fn with_rel_types(mut self, rel_types: Vec<String>) -> Self {
        self.allowed_rel_types = rel_types;
        self
    }

    /// Run pending schema migrations.
    ///
    /// Idempotent — only applies migrations not yet recorded in the database.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::Schema`] if migrations are invalid or execution fails.
    #[instrument(skip(self))]
    pub async fn migrate(&self) -> GraphResult<u32> {
        schema::validate_migrations()?;

        // Find already-applied versions
        let mut result = self
            .graph
            .execute(query(schema::list_applied_cypher()))
            .await
            .map_err(|e| GraphError::Schema(format!("Failed to list migrations: {e}")))?;

        let mut applied: Vec<u32> = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Schema(format!("Failed to read migration row: {e}")))?
        {
            let version: i64 = row
                .get("version")
                .map_err(|e| GraphError::Schema(format!("Missing version field: {e}")))?;
            if let Ok(v) = u32::try_from(version) {
                applied.push(v);
            }
        }

        let pending = schema::pending_migrations(&applied);
        let count = pending.len();

        for migration in pending {
            self.apply_migration(migration).await?;
        }

        tracing::info!(applied = count, "Schema migrations complete");
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// Apply a single migration.
    ///
    /// Neo4j 5.x prohibits mixing DDL (schema) and DML (data) in the same
    /// transaction. Each DDL statement runs in its own transaction; the
    /// migration record is written in a final separate DML transaction.
    async fn apply_migration(&self, migration: &Migration) -> GraphResult<()> {
        // DDL phase: each schema statement in its own transaction.
        for stmt in migration.statements {
            let mut txn = self
                .graph
                .start_txn()
                .await
                .map_err(|e| GraphError::Schema(format!("Transaction start failed: {e}")))?;

            txn.run(query(stmt)).await.map_err(|e| {
                GraphError::Schema(format!(
                    "Migration v{} statement failed: {e}",
                    migration.version
                ))
            })?;

            txn.commit().await.map_err(|e| {
                GraphError::Schema(format!(
                    "Migration v{} DDL commit failed: {e}",
                    migration.version
                ))
            })?;
        }

        // DML phase: record the migration in a separate transaction.
        let mut txn = self
            .graph
            .start_txn()
            .await
            .map_err(|e| GraphError::Schema(format!("Transaction start failed: {e}")))?;

        let now = chrono::Utc::now().to_rfc3339();
        txn.run(
            query(schema::record_migration_cypher())
                .param("version", i64::from(migration.version))
                .param("applied_at", now.as_str())
                .param("description", migration.description),
        )
        .await
        .map_err(|e| {
            GraphError::Schema(format!(
                "Failed to record migration v{}: {e}",
                migration.version
            ))
        })?;

        txn.commit().await.map_err(|e| {
            GraphError::Schema(format!(
                "Migration v{} record commit failed: {e}",
                migration.version
            ))
        })?;

        tracing::info!(
            version = migration.version,
            desc = migration.description,
            "Applied migration"
        );
        Ok(())
    }

    /// Convert a `serde_json::Value` to a `BoltType`.
    ///
    /// Uses `TryFrom` provided by neo4rs `json` feature.
    fn json_to_bolt(value: &serde_json::Value) -> GraphResult<BoltType> {
        BoltType::try_from(value.clone())
            .map_err(|e| GraphError::Query(format!("Value conversion failed: {e}")))
    }

    /// Apply params from a `BTreeMap<String, serde_json::Value>` to a Query.
    fn apply_params(q: Query, params: &BTreeMap<String, serde_json::Value>) -> GraphResult<Query> {
        let mut q = q;
        for (key, value) in params {
            let bolt = Self::json_to_bolt(value)?;
            q = q.param(key.as_str(), bolt);
        }
        Ok(q)
    }

    /// Build label string from validated labels (e.g., `:Note:HelixEntry`).
    fn label_string(labels: &[String]) -> String {
        if labels.is_empty() {
            String::new()
        } else {
            format!(":{}", labels.join(":"))
        }
    }

    /// Validate labels against our allowlist.
    fn validate_labels(&self, labels: &[String]) -> GraphResult<()> {
        let refs: Vec<&str> = self.allowed_labels.iter().map(String::as_str).collect();
        validation::validate_labels(labels, &refs)
    }

    /// Validate a relationship type against our allowlist.
    fn validate_rel_type(&self, rel_type: &str) -> GraphResult<()> {
        let refs: Vec<&str> = self.allowed_rel_types.iter().map(String::as_str).collect();
        validation::validate_rel_type(rel_type, &refs)
    }

    /// Collect directed edges between a set of node IDs.
    ///
    /// Used by [`traverse()`](GraphStore::traverse) to populate edges
    /// in the returned [`SubGraph`]. Runs a separate Cypher query to
    /// find all relationships between the traversed nodes.
    async fn collect_traverse_edges(
        &self,
        node_ids: &[String],
        rel_filter: &str,
    ) -> GraphResult<Vec<Edge>> {
        let ids_json: Vec<serde_json::Value> =
            node_ids.iter().map(|id| serde_json::json!(id)).collect();
        let bolt_ids = Self::json_to_bolt(&serde_json::Value::Array(ids_json))?;

        let edge_cypher = format!(
            "MATCH (a)-[r{rel_filter}]->(b) \
             WHERE a.id IN $ids AND b.id IN $ids \
             RETURN coalesce(r.id, '') AS id, type(r) AS rel_type, \
                    a.id AS from_id, b.id AS to_id, properties(r) AS props"
        );

        let edge_q = query(&edge_cypher).param("ids", bolt_ids);
        let mut result = self
            .graph
            .execute(edge_q)
            .await
            .map_err(|e| GraphError::Query(format!("traverse edge query failed: {e}")))?;

        let mut edges = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(format!("Edge row fetch failed: {e}")))?
        {
            let rel_type: String = match row.get("rel_type") {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::debug!(error = %e, "Failed to parse edge rel_type, skipping");
                    continue;
                }
            };
            let id: String = row.get("id").unwrap_or_default();
            let from_id: String = row.get("from_id").unwrap_or_default();
            let to_id: String = row.get("to_id").unwrap_or_default();
            let props: HashMap<String, serde_json::Value> = row.get("props").unwrap_or_default();

            edges.push(Edge {
                id,
                rel_type,
                from_id,
                to_id,
                properties: props.into_iter().collect(),
            });
        }

        Ok(edges)
    }
}

// ============================================================================
// GraphStore Implementation
// ============================================================================

#[async_trait]
impl GraphStore for Neo4jBackend {
    #[instrument(skip(self, params))]
    async fn execute(
        &self,
        cypher: &str,
        params: BTreeMap<String, serde_json::Value>,
    ) -> GraphResult<Vec<Record>> {
        let q = Self::apply_params(query(cypher), &params)?;
        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(format!("Execute failed: {e}")))?;

        let mut records = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(format!("Row fetch failed: {e}")))?
        {
            // Deserialize the row into a HashMap — requires neo4rs json feature
            let fields: HashMap<String, serde_json::Value> = row
                .to()
                .map_err(|e| GraphError::Query(format!("Row deserialization failed: {e}")))?;
            records.push(Record {
                fields: fields.into_iter().collect(),
            });
        }

        Ok(records)
    }

    #[instrument(skip(self, filters))]
    async fn find_nodes(
        &self,
        label: &str,
        filters: BTreeMap<String, serde_json::Value>,
        limit: Option<u32>,
    ) -> GraphResult<Vec<Node>> {
        self.validate_labels(&[label.to_owned()])?;

        let capped_limit = limit.unwrap_or(DEFAULT_TRAVERSAL_LIMIT);

        // Build WHERE clause from filters with parameterized values
        let mut where_parts = Vec::new();
        let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        for (i, (key, value)) in filters.iter().enumerate() {
            let param_name = format!("f{i}");
            where_parts.push(format!("n.`{key}` = ${param_name}"));
            params.insert(param_name, value.clone());
        }

        let where_clause = if where_parts.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_parts.join(" AND "))
        };

        let cypher = format!(
            "MATCH (n:{label}){where_clause} \
             RETURN n.id AS id, labels(n) AS labels, properties(n) AS props \
             LIMIT {capped_limit}"
        );

        let q = Self::apply_params(query(&cypher), &params)?;
        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(format!("find_nodes failed: {e}")))?;

        let mut nodes = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(format!("Row fetch failed: {e}")))?
        {
            let id: String = row
                .get("id")
                .map_err(|e| GraphError::Query(format!("Missing id: {e}")))?;
            let labels: Vec<String> = row.get("labels").unwrap_or_else(|e| {
                tracing::debug!(node_id = %id, error = %e, "Failed to parse node labels");
                Vec::new()
            });
            let props: HashMap<String, serde_json::Value> = row.get("props").unwrap_or_else(|e| {
                tracing::debug!(node_id = %id, error = %e, "Failed to parse node props");
                HashMap::new()
            });

            nodes.push(Node {
                id,
                labels,
                properties: props.into_iter().collect(),
            });
        }

        Ok(nodes)
    }

    #[instrument(skip(self))]
    async fn traverse(
        &self,
        from_id: &str,
        rel_types: &[String],
        depth: u32,
        limit: u32,
    ) -> GraphResult<SubGraph> {
        for rt in rel_types {
            self.validate_rel_type(rt)?;
        }

        let capped_limit = limit.min(MAX_TRAVERSAL_LIMIT);

        // Build relationship filter
        let rel_filter = if rel_types.is_empty() {
            String::new()
        } else {
            format!(":{}", rel_types.join("|"))
        };

        let cypher = format!(
            "MATCH (start {{id: $from_id}})-[r{rel_filter}*1..{depth}]-(end) \
             WITH DISTINCT end LIMIT {capped_limit} \
             RETURN end.id AS id, labels(end) AS labels, properties(end) AS props"
        );

        let q = query(&cypher).param("from_id", from_id);
        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(format!("traverse failed: {e}")))?;

        let mut subgraph = SubGraph::default();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(format!("Row fetch failed: {e}")))?
        {
            let id: String = match row.get("id") {
                Ok(v) => v,
                Err(e) => {
                    tracing::debug!(error = %e, "Failed to parse traversed node id, skipping");
                    continue;
                }
            };
            let labels: Vec<String> = row.get("labels").unwrap_or_else(|e| {
                tracing::debug!(node_id = %id, error = %e, "Failed to parse node labels");
                Vec::new()
            });
            let props: HashMap<String, serde_json::Value> = row.get("props").unwrap_or_else(|e| {
                tracing::debug!(node_id = %id, error = %e, "Failed to parse node props");
                HashMap::new()
            });

            subgraph.nodes.push(Node {
                id,
                labels,
                properties: props.into_iter().collect(),
            });
        }

        // Collect edges between traversed nodes (fixes behavioral parity with file backend)
        if !subgraph.nodes.is_empty() {
            let mut all_ids: Vec<String> = subgraph.nodes.iter().map(|n| n.id.clone()).collect();
            all_ids.push(from_id.to_owned());
            subgraph.edges = self.collect_traverse_edges(&all_ids, &rel_filter).await?;
        }

        Ok(subgraph)
    }

    #[instrument(skip(self, props))]
    async fn create_node(
        &self,
        labels: &[String],
        props: BTreeMap<String, serde_json::Value>,
    ) -> GraphResult<String> {
        self.validate_labels(labels)?;

        let label_str = Self::label_string(labels);

        // Generate an ID if not provided
        let id = props
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map_or_else(|| uuid::Uuid::new_v4().to_string(), String::from);

        let mut all_props = props;
        all_props
            .entry("id".to_owned())
            .or_insert_with(|| serde_json::Value::String(id.clone()));

        // Build individual SET assignments with parameterized values
        let mut set_parts = Vec::new();
        let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        params.insert("merge_id".into(), serde_json::json!(&id));

        for (i, (key, value)) in all_props.iter().enumerate() {
            let param_name = format!("p{i}");
            set_parts.push(format!("n.`{key}` = ${param_name}"));
            params.insert(param_name, value.clone());
        }

        let set_clause = if set_parts.is_empty() {
            String::new()
        } else {
            format!(" SET {}", set_parts.join(", "))
        };

        let cypher =
            format!("MERGE (n{label_str} {{id: $merge_id}}){set_clause} RETURN n.id AS id");

        let q = Self::apply_params(query(&cypher), &params)?;

        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(format!("create_node failed: {e}")))?;

        if let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(format!("Row fetch failed: {e}")))?
        {
            let returned_id: String = row
                .get("id")
                .map_err(|e| GraphError::Query(format!("Missing id in result: {e}")))?;
            Ok(returned_id)
        } else {
            Err(GraphError::Query("MERGE returned no rows".into()))
        }
    }

    #[instrument(skip(self, props))]
    async fn create_relationship(
        &self,
        from: &str,
        to: &str,
        rel_type: &str,
        props: BTreeMap<String, serde_json::Value>,
    ) -> GraphResult<String> {
        self.validate_rel_type(rel_type)?;

        let rel_id = uuid::Uuid::new_v4().to_string();

        // Build individual SET assignments
        let mut set_parts = vec!["r.id = $rel_id".to_owned()];
        let mut params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        params.insert("from_id".into(), serde_json::json!(from));
        params.insert("to_id".into(), serde_json::json!(to));
        params.insert("rel_id".into(), serde_json::json!(&rel_id));

        for (i, (key, value)) in props.iter().enumerate() {
            let param_name = format!("rp{i}");
            set_parts.push(format!("r.`{key}` = ${param_name}"));
            params.insert(param_name, value.clone());
        }

        let set_clause = format!(" SET {}", set_parts.join(", "));

        let cypher = format!(
            "MATCH (a {{id: $from_id}}), (b {{id: $to_id}}) \
             MERGE (a)-[r:{rel_type}]->(b){set_clause} \
             RETURN r.id AS id"
        );

        let q = Self::apply_params(query(&cypher), &params)?;

        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(format!("create_relationship failed: {e}")))?;

        if let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(format!("Row fetch failed: {e}")))?
        {
            let returned_id: String = row
                .get("id")
                .map_err(|e| GraphError::Query(format!("Missing id: {e}")))?;
            Ok(returned_id)
        } else {
            Err(GraphError::Query(
                "MERGE returned no rows — source or target node not found".into(),
            ))
        }
    }

    #[instrument(skip(self))]
    async fn delete_node(&self, id: &str) -> GraphResult<()> {
        let cypher = "MATCH (n {id: $id}) DETACH DELETE n";
        let q = query(cypher).param("id", id);

        // Consume the stream to execute the query
        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(format!("delete_node failed: {e}")))?;

        while result
            .next()
            .await
            .map_err(|e| GraphError::Query(format!("delete_node stream: {e}")))?
            .is_some()
        {}

        Ok(())
    }

    #[instrument(skip(self))]
    async fn health(&self) -> GraphResult<HealthStatus> {
        let start = std::time::Instant::now();

        let cypher = "CALL dbms.components() YIELD name, versions, edition \
                      RETURN name, versions[0] AS version, edition";

        let mut result = self
            .graph
            .execute(query(cypher))
            .await
            .map_err(|e| GraphError::Connection(format!("Health check failed: {e}")))?;

        let mut details = BTreeMap::new();
        if let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Connection(format!("Row fetch failed: {e}")))?
        {
            if let Ok(name) = row.get::<String>("name") {
                details.insert("name".into(), serde_json::json!(name));
            }
            if let Ok(version) = row.get::<String>("version") {
                details.insert("version".into(), serde_json::json!(version));
            }
            if let Ok(edition) = row.get::<String>("edition") {
                details.insert("edition".into(), serde_json::json!(edition));
            }
        }

        // Get node/edge counts
        let count_cypher = "MATCH (n) WITH count(n) AS nc \
                            OPTIONAL MATCH ()-[r]->() \
                            RETURN nc, count(r) AS rc";
        let mut count_result = self
            .graph
            .execute(query(count_cypher))
            .await
            .map_err(|e| GraphError::Connection(format!("Count query failed: {e}")))?;

        let mut node_count = None;
        let mut edge_count = None;
        if let Some(row) = count_result
            .next()
            .await
            .map_err(|e| GraphError::Connection(format!("Row fetch failed: {e}")))?
        {
            if let Ok(nc) = row.get::<i64>("nc") {
                node_count = u64::try_from(nc).ok();
            }
            if let Ok(rc) = row.get::<i64>("rc") {
                edge_count = u64::try_from(rc).ok();
            }
        }

        let latency = start.elapsed().as_millis();

        Ok(HealthStatus {
            connected: true,
            backend: "neo4j".into(),
            node_count,
            edge_count,
            latency_ms: Some(u64::try_from(latency).unwrap_or(u64::MAX)),
            details,
        })
    }

    #[instrument(skip(self, nodes))]
    async fn batch_create_nodes(&self, nodes: Vec<BatchNode>) -> GraphResult<BatchResult> {
        if nodes.is_empty() {
            return Ok(BatchResult::default());
        }

        // Validate all labels upfront
        for node in &nodes {
            self.validate_labels(&node.labels)?;
        }

        let mut total_created: u64 = 0;
        let mut errors = Vec::new();

        // Process in transactional chunks
        for chunk in nodes.chunks(DEFAULT_BATCH_SIZE) {
            match self.batch_nodes_txn(chunk).await {
                Ok(count) => total_created = total_created.saturating_add(count),
                Err(e) => errors.push(e.to_string()),
            }
        }

        Ok(BatchResult {
            created: total_created,
            errors,
        })
    }

    #[instrument(skip(self, rels))]
    async fn batch_create_relationships(
        &self,
        rels: Vec<BatchRelationship>,
    ) -> GraphResult<BatchResult> {
        if rels.is_empty() {
            return Ok(BatchResult::default());
        }

        for rel in &rels {
            self.validate_rel_type(&rel.rel_type)?;
        }

        let mut total_created: u64 = 0;
        let mut errors = Vec::new();

        for chunk in rels.chunks(DEFAULT_BATCH_SIZE) {
            match self.batch_rels_txn(chunk).await {
                Ok(count) => total_created = total_created.saturating_add(count),
                Err(e) => errors.push(e.to_string()),
            }
        }

        Ok(BatchResult {
            created: total_created,
            errors,
        })
    }
}

// ============================================================================
// Batch Helpers (transactional batching)
// ============================================================================

impl Neo4jBackend {
    /// Create a batch of nodes within a single transaction.
    ///
    /// Each node gets an individual MERGE statement. The transaction
    /// ensures atomicity — either all succeed or none do.
    async fn batch_nodes_txn(&self, nodes: &[BatchNode]) -> GraphResult<u64> {
        let mut txn = self
            .graph
            .start_txn()
            .await
            .map_err(|e| GraphError::Query(format!("Batch txn start failed: {e}")))?;

        let mut created: u64 = 0;

        for node in nodes {
            let label_str = Self::label_string(&node.labels);

            let id = node
                .properties
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map_or_else(|| uuid::Uuid::new_v4().to_string(), String::from);

            let mut all_props = node.properties.clone();
            all_props
                .entry("id".to_owned())
                .or_insert_with(|| serde_json::Value::String(id.clone()));

            // Build SET clause with individual params
            let mut set_parts = Vec::new();
            let mut param_defs = Vec::new();
            for (i, (key, value)) in all_props.iter().enumerate() {
                let pname = format!("bp{i}");
                set_parts.push(format!("n.`{key}` = ${pname}"));
                param_defs.push((pname, value.clone()));
            }

            let set_str = set_parts.join(", ");
            let cypher =
                format!("MERGE (n{label_str} {{id: $merge_id}}) SET {set_str} RETURN 1 AS ok");

            let mut q = query(&cypher).param("merge_id", id.as_str());
            for (pname, value) in &param_defs {
                let bolt = Self::json_to_bolt(value)?;
                q = q.param(pname.as_str(), bolt);
            }

            txn.run(q)
                .await
                .map_err(|e| GraphError::Query(format!("Batch node merge failed: {e}")))?;

            created = created.saturating_add(1);
        }

        txn.commit()
            .await
            .map_err(|e| GraphError::Query(format!("Batch commit failed: {e}")))?;

        Ok(created)
    }

    /// Create a batch of relationships within a single transaction.
    async fn batch_rels_txn(&self, rels: &[BatchRelationship]) -> GraphResult<u64> {
        let mut txn = self
            .graph
            .start_txn()
            .await
            .map_err(|e| GraphError::Query(format!("Batch txn start failed: {e}")))?;

        let mut created: u64 = 0;

        for rel in rels {
            let rel_id = uuid::Uuid::new_v4().to_string();

            let mut set_parts = vec!["r.id = $rel_id".to_owned()];
            let mut param_defs: Vec<(String, serde_json::Value)> = vec![
                ("from_id".into(), serde_json::json!(&rel.from_id)),
                ("to_id".into(), serde_json::json!(&rel.to_id)),
                ("rel_id".into(), serde_json::json!(&rel_id)),
            ];

            for (i, (key, value)) in rel.properties.iter().enumerate() {
                let pname = format!("rp{i}");
                set_parts.push(format!("r.`{key}` = ${pname}"));
                param_defs.push((pname, value.clone()));
            }

            let set_str = set_parts.join(", ");
            let cypher = format!(
                "MATCH (a {{id: $from_id}}), (b {{id: $to_id}}) \
                 MERGE (a)-[r:{}]->(b) SET {set_str} RETURN 1 AS ok",
                rel.rel_type
            );

            let mut q = query(&cypher);
            for (pname, value) in &param_defs {
                let bolt = Self::json_to_bolt(value)?;
                q = q.param(pname.as_str(), bolt);
            }

            txn.run(q)
                .await
                .map_err(|e| GraphError::Query(format!("Batch rel merge failed: {e}")))?;

            created = created.saturating_add(1);
        }

        txn.commit()
            .await
            .map_err(|e| GraphError::Query(format!("Batch rel commit failed: {e}")))?;

        Ok(created)
    }
}

// ============================================================================
// Tests (unit tests — integration tests require a running Neo4j)
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_label_string_empty() {
        assert_eq!(Neo4jBackend::label_string(&[]), "");
    }

    #[test]
    fn test_label_string_single() {
        assert_eq!(Neo4jBackend::label_string(&["Note".into()]), ":Note");
    }

    #[test]
    fn test_label_string_multiple() {
        assert_eq!(
            Neo4jBackend::label_string(&["Note".into(), "HelixEntry".into()]),
            ":Note:HelixEntry"
        );
    }

    #[test]
    fn test_json_to_bolt_string() {
        let bolt = Neo4jBackend::json_to_bolt(&serde_json::json!("hello"));
        assert!(bolt.is_ok());
    }

    #[test]
    fn test_json_to_bolt_number() {
        let bolt = Neo4jBackend::json_to_bolt(&serde_json::json!(42));
        assert!(bolt.is_ok());
    }

    #[test]
    fn test_json_to_bolt_float() {
        let bolt = Neo4jBackend::json_to_bolt(&serde_json::json!(9.5));
        assert!(bolt.is_ok());
    }

    #[test]
    fn test_json_to_bolt_bool() {
        let bolt = Neo4jBackend::json_to_bolt(&serde_json::json!(true));
        assert!(bolt.is_ok());
    }

    #[test]
    fn test_json_to_bolt_null() {
        let bolt = Neo4jBackend::json_to_bolt(&serde_json::Value::Null);
        assert!(bolt.is_ok());
    }

    #[test]
    fn test_json_to_bolt_array() {
        let bolt = Neo4jBackend::json_to_bolt(&serde_json::json!(["faith", "growth"]));
        assert!(bolt.is_ok());
    }

    #[test]
    fn test_apply_params_empty() {
        let params = BTreeMap::new();
        let q = Neo4jBackend::apply_params(query("RETURN 1"), &params);
        assert!(q.is_ok());
    }

    #[test]
    fn test_apply_params_mixed_types() {
        let mut params = BTreeMap::new();
        params.insert("name".into(), serde_json::json!("Kevin"));
        params.insert("age".into(), serde_json::json!(42));
        params.insert("active".into(), serde_json::json!(true));
        let q = Neo4jBackend::apply_params(query("RETURN $name, $age, $active"), &params);
        assert!(q.is_ok());
    }
}
