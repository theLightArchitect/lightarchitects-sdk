//! File backend adapter — markdown vault as a graph.
//!
//! Implements [`GraphStore`](crate::GraphStore) by treating SOUL's
//! Obsidian-compatible markdown vault as a knowledge graph:
//!
//! - **Files → Nodes**: Each `.md` file becomes a `Note` node. Frontmatter
//!   properties map to node properties. Files in `helix/` also get the
//!   `HelixEntry` label.
//! - **Wikilinks → Edges**: `[[target]]` wikilinks become `LINKS_TO` edges.
//! - **Tags → Tag Nodes**: `#tagname` or frontmatter `tags: [...]` create
//!   `Tag` nodes connected via `HAS_TAG` edges.
//!
//! # Limitations
//!
//! The file backend does not support arbitrary Cypher queries. The `execute()`
//! method returns `Err(GraphError::Unsupported)`. Use `find_nodes()` and
//! `traverse()` for queries.
//!
//! # Performance
//!
//! An in-memory index is built on `connect()` by walking the vault directory.
//! The index is read-only — mutations go through `create_node()` which writes
//! to disk and updates the in-memory state.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tracing::instrument;

use super::{
    BatchNode, BatchRelationship, BatchResult, DEFAULT_TRAVERSAL_LIMIT, Edge, GraphError,
    GraphResult, GraphStore, HealthStatus, MAX_TRAVERSAL_LIMIT, Node, Record, SubGraph,
};

// ============================================================================
// In-Memory Index
// ============================================================================

/// In-memory graph index built from the vault filesystem.
#[derive(Debug, Default)]
struct VaultIndex {
    /// All nodes keyed by their application ID (relative path).
    nodes: HashMap<String, Node>,
    /// Outgoing edges keyed by source node ID.
    outgoing: HashMap<String, Vec<Edge>>,
    /// Incoming edges keyed by target node ID.
    incoming: HashMap<String, Vec<Edge>>,
}

impl VaultIndex {
    fn insert_node(&mut self, node: Node) {
        self.nodes.insert(node.id.clone(), node);
    }

    fn insert_edge(&mut self, edge: Edge) {
        self.outgoing
            .entry(edge.from_id.clone())
            .or_default()
            .push(edge.clone());
        self.incoming
            .entry(edge.to_id.clone())
            .or_default()
            .push(edge);
    }
}

// ============================================================================
// FileBackend
// ============================================================================

/// Markdown vault file backend.
///
/// Reads the SOUL vault directory to build an in-memory graph index.
/// Writes create actual markdown files. Read operations query the index.
pub struct FileBackend {
    vault_root: PathBuf,
    index: Arc<RwLock<VaultIndex>>,
}

impl std::fmt::Debug for FileBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileBackend")
            .field("vault_root", &self.vault_root)
            .finish_non_exhaustive()
    }
}

impl FileBackend {
    /// Open a vault directory and build the in-memory index.
    ///
    /// # Arguments
    ///
    /// - `vault_root` — Path to the SOUL vault root (e.g., `~/lightarchitects/soul/`)
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::Connection`] if the vault directory doesn't exist.
    #[instrument(skip(vault_root))]
    pub async fn connect(vault_root: impl AsRef<Path>) -> GraphResult<Self> {
        let vault_root = vault_root.as_ref().to_path_buf();
        if !vault_root.is_dir() {
            return Err(GraphError::Connection(format!(
                "Vault root does not exist: {}",
                vault_root.display()
            )));
        }

        let backend = Self {
            vault_root,
            index: Arc::new(RwLock::new(VaultIndex::default())),
        };

        backend.rebuild_index().await?;

        tracing::info!(
            vault = %backend.vault_root.display(),
            "FileBackend connected"
        );
        Ok(backend)
    }

    /// Rebuild the in-memory index by walking the vault directory.
    async fn rebuild_index(&self) -> GraphResult<()> {
        let vault_root = self.vault_root.clone();

        // Walk filesystem in a blocking task (file I/O)
        let (nodes, edges) = tokio::task::spawn_blocking(move || walk_vault(&vault_root))
            .await
            .map_err(|e| {
                GraphError::Io(std::io::Error::other(format!(
                    "Index build task failed: {e}"
                )))
            })?;

        let mut index = self.index.write().await;
        *index = VaultIndex::default();

        for node in nodes {
            index.insert_node(node);
        }
        for edge in edges {
            index.insert_edge(edge);
        }

        tracing::info!(
            nodes = index.nodes.len(),
            edges = index.outgoing.values().map(Vec::len).sum::<usize>(),
            "Vault index rebuilt"
        );
        Ok(())
    }

    /// Compute a SHA-256 content hash for a file path.
    fn content_hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        format!("{:x}", hasher.finalize())
    }

    /// Return all edges from the in-memory index as [`BatchRelationship`] values.
    ///
    /// Used by [`DualBackend::sync()`](crate::dual::DualBackend::sync) to
    /// replicate file-backend relationships into Neo4j.
    pub async fn all_edges(&self) -> Vec<BatchRelationship> {
        let index = self.index.read().await;
        index
            .outgoing
            .values()
            .flatten()
            .map(|edge| BatchRelationship {
                from_id: edge.from_id.clone(),
                to_id: edge.to_id.clone(),
                rel_type: edge.rel_type.clone(),
                properties: edge.properties.clone(),
            })
            .collect()
    }
}

// ============================================================================
// Vault Walker
// ============================================================================

/// Walk the vault directory and extract nodes + edges.
///
/// This runs on a blocking thread. It reads markdown files, extracts
/// frontmatter properties, wikilinks, and tags.
fn walk_vault(vault_root: &Path) -> (Vec<Node>, Vec<Edge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut edge_id_counter: u64 = 0;

    for entry in walkdir::WalkDir::new(vault_root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let path = entry.path();
        let rel_path = path
            .strip_prefix(vault_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Skipping unreadable file");
                continue;
            }
        };

        // Determine labels
        let mut labels = vec!["Note".to_owned()];
        if rel_path.starts_with("helix/") && rel_path.contains("/entries/") {
            labels.push("HelixEntry".to_owned());
        }

        // Extract frontmatter properties
        let properties = extract_frontmatter(&content, &rel_path);

        // Build node
        nodes.push(Node {
            id: rel_path.clone(),
            labels,
            properties,
        });

        // Extract wikilinks → LINKS_TO edges
        for target in extract_wikilinks(&content) {
            edge_id_counter = edge_id_counter.saturating_add(1);
            edges.push(Edge {
                id: format!("link_{edge_id_counter}"),
                rel_type: "LINKS_TO".to_owned(),
                from_id: rel_path.clone(),
                to_id: target,
                properties: BTreeMap::new(),
            });
        }

        // Extract tags → HAS_TAG edges (tag nodes created separately)
        for tag in extract_tags(&content) {
            edge_id_counter = edge_id_counter.saturating_add(1);
            edges.push(Edge {
                id: format!("tag_{edge_id_counter}"),
                rel_type: "HAS_TAG".to_owned(),
                from_id: rel_path.clone(),
                to_id: format!("tag:{tag}"),
                properties: BTreeMap::new(),
            });
        }
    }

    // Create Tag nodes for all referenced tags
    let tag_ids: HashSet<String> = edges
        .iter()
        .filter(|e| e.rel_type == "HAS_TAG")
        .map(|e| e.to_id.clone())
        .collect();

    for tag_id in tag_ids {
        let name = tag_id.strip_prefix("tag:").unwrap_or(&tag_id);
        let mut props = BTreeMap::new();
        props.insert("name".into(), serde_json::json!(name));
        nodes.push(Node {
            id: tag_id,
            labels: vec!["Tag".to_owned()],
            properties: props,
        });
    }

    (nodes, edges)
}

/// Extract YAML frontmatter from markdown content.
///
/// Handles `---` delimited frontmatter. Returns key-value properties
/// as `serde_json::Value` entries.
fn extract_frontmatter(content: &str, path: &str) -> BTreeMap<String, serde_json::Value> {
    let mut props = BTreeMap::new();
    props.insert("id".into(), serde_json::json!(path));
    props.insert(
        "content_hash".into(),
        serde_json::json!(FileBackend::content_hash(content.as_bytes())),
    );

    // Simple frontmatter parser — handles flat key: value pairs
    if !content.starts_with("---") {
        return props;
    }

    let after_first = &content[3..];
    let Some(end) = after_first.find("\n---") else {
        return props;
    };

    let frontmatter = &after_first[..end];

    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_owned();
            let value = value.trim();

            // Skip complex YAML (arrays, nested objects) — keep it flat
            if value.starts_with('[') || value.starts_with('{') {
                continue;
            }
            if value.is_empty() {
                continue;
            }

            // Try to parse as number, bool, or string
            let json_val = if let Ok(n) = value.parse::<f64>() {
                serde_json::json!(n)
            } else if value == "true" {
                serde_json::json!(true)
            } else if value == "false" {
                serde_json::json!(false)
            } else {
                // Strip surrounding quotes if present
                let stripped = value
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .unwrap_or(value);
                serde_json::json!(stripped)
            };

            props.insert(key, json_val);
        }
    }

    props
}

/// Extract wikilink targets from markdown content.
///
/// Matches `[[target]]` and `[[target|display]]` patterns.
fn extract_wikilinks(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '[' && chars.peek() == Some(&'[') {
            chars.next(); // consume second '['
            let mut target = String::new();
            let mut found_close = false;

            for inner in chars.by_ref() {
                if inner == ']' {
                    found_close = true;
                    // consume second ']' if present
                    if chars.peek() == Some(&']') {
                        chars.next();
                    }
                    break;
                }
                if inner == '|' {
                    // Display text follows — stop collecting target
                    for rest in chars.by_ref() {
                        if rest == ']' {
                            if chars.peek() == Some(&']') {
                                chars.next();
                            }
                            break;
                        }
                    }
                    found_close = true;
                    break;
                }
                target.push(inner);
            }

            if found_close && !target.is_empty() {
                // Normalize: add .md extension if not present
                let target = if std::path::Path::new(&target)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                {
                    target
                } else {
                    format!("{target}.md")
                };
                links.push(target);
            }
        }
    }

    links
}

/// Extract tags from markdown content.
///
/// Matches `#tagname` in body text and `tags:` in frontmatter arrays.
fn extract_tags(content: &str) -> Vec<String> {
    let mut tags = HashSet::new();

    // Extract inline tags: #word (but not # headings)
    for line in content.lines() {
        let trimmed = line.trim();
        // Skip headings
        if trimmed.starts_with("# ") || trimmed.starts_with("## ") || trimmed.starts_with("### ") {
            continue;
        }
        // Skip frontmatter delimiters
        if trimmed == "---" {
            continue;
        }

        for word in trimmed.split_whitespace() {
            if let Some(tag) = word.strip_prefix('#') {
                let tag = tag.trim_end_matches(|c: char| c.is_ascii_punctuation());
                if !tag.is_empty()
                    && tag
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                {
                    tags.insert(tag.to_owned());
                }
            }
        }
    }

    // Extract frontmatter tags: `tags: [a, b, c]`
    if content.starts_with("---") {
        if let Some(after_first) = content.get(3..) {
            if let Some(end) = after_first.find("\n---") {
                let fm = &after_first[..end];
                for line in fm.lines() {
                    let trimmed = line.trim();
                    if let Some(val) = trimmed.strip_prefix("tags:") {
                        let val = val.trim();
                        if let Some(inner) = val.strip_prefix('[') {
                            if let Some(inner) = inner.strip_suffix(']') {
                                for tag in inner.split(',') {
                                    let tag = tag.trim().trim_matches('"').trim_matches('\'');
                                    if !tag.is_empty() {
                                        tags.insert(tag.to_owned());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    tags.into_iter().collect()
}

// ============================================================================
// GraphStore Implementation
// ============================================================================

#[async_trait]
impl GraphStore for FileBackend {
    async fn execute(
        &self,
        _query: &str,
        _params: BTreeMap<String, serde_json::Value>,
    ) -> GraphResult<Vec<Record>> {
        Err(GraphError::Unsupported(
            "FileBackend does not support raw Cypher queries. Use find_nodes() or traverse()."
                .into(),
        ))
    }

    #[instrument(skip(self, filters))]
    async fn find_nodes(
        &self,
        label: &str,
        filters: BTreeMap<String, serde_json::Value>,
        limit: Option<u32>,
    ) -> GraphResult<Vec<Node>> {
        let index = self.index.read().await;
        let cap = limit.unwrap_or(DEFAULT_TRAVERSAL_LIMIT) as usize;

        let nodes: Vec<Node> = index
            .nodes
            .values()
            .filter(|node| {
                // Match label
                if !node.labels.iter().any(|l| l == label) {
                    return false;
                }
                // Match all filters
                filters
                    .iter()
                    .all(|(key, value)| node.properties.get(key).is_some_and(|v| v == value))
            })
            .take(cap)
            .cloned()
            .collect();

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
        let index = self.index.read().await;
        let capped_limit = limit.min(MAX_TRAVERSAL_LIMIT) as usize;

        let mut visited = HashSet::new();
        let mut frontier = vec![from_id.to_owned()];
        let mut result_nodes = Vec::new();
        let mut result_edges = Vec::new();

        for _ in 0..depth {
            let mut next_frontier = Vec::new();

            for node_id in &frontier {
                if !visited.insert(node_id.clone()) {
                    continue;
                }

                if let Some(node) = index.nodes.get(node_id) {
                    result_nodes.push(node.clone());
                }

                if result_nodes.len() >= capped_limit {
                    break;
                }

                // Follow outgoing edges
                if let Some(edges) = index.outgoing.get(node_id) {
                    for edge in edges {
                        if !rel_types.is_empty() && !rel_types.iter().any(|rt| rt == &edge.rel_type)
                        {
                            continue;
                        }
                        result_edges.push(edge.clone());
                        if !visited.contains(&edge.to_id) {
                            next_frontier.push(edge.to_id.clone());
                        }
                    }
                }

                // Follow incoming edges (undirected traversal)
                if let Some(edges) = index.incoming.get(node_id) {
                    for edge in edges {
                        if !rel_types.is_empty() && !rel_types.iter().any(|rt| rt == &edge.rel_type)
                        {
                            continue;
                        }
                        result_edges.push(edge.clone());
                        if !visited.contains(&edge.from_id) {
                            next_frontier.push(edge.from_id.clone());
                        }
                    }
                }
            }

            if result_nodes.len() >= capped_limit || next_frontier.is_empty() {
                break;
            }

            frontier = next_frontier;
        }

        Ok(SubGraph {
            nodes: result_nodes,
            edges: result_edges,
        })
    }

    #[instrument(skip(self, props))]
    async fn create_node(
        &self,
        labels: &[String],
        props: BTreeMap<String, serde_json::Value>,
    ) -> GraphResult<String> {
        let id = props
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map_or_else(|| uuid::Uuid::new_v4().to_string(), String::from);

        // For Note nodes, create a markdown file
        if labels.iter().any(|l| l == "Note") {
            let file_path = self.vault_root.join(&id);
            if let Some(parent) = file_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(GraphError::Io)?;
            }

            // Build frontmatter
            let mut fm_lines = vec!["---".to_owned()];
            for (key, value) in &props {
                if key == "id" || key == "content_hash" {
                    continue;
                }
                match value {
                    serde_json::Value::String(s) => fm_lines.push(format!("{key}: \"{s}\"")),
                    serde_json::Value::Number(n) => fm_lines.push(format!("{key}: {n}")),
                    serde_json::Value::Bool(b) => fm_lines.push(format!("{key}: {b}")),
                    _ => {}
                }
            }
            fm_lines.push("---".to_owned());
            fm_lines.push(String::new());

            let content = fm_lines.join("\n");
            tokio::fs::write(&file_path, &content)
                .await
                .map_err(GraphError::Io)?;
        }

        // Update in-memory index
        let mut all_props = props;
        all_props
            .entry("id".to_owned())
            .or_insert_with(|| serde_json::Value::String(id.clone()));

        let node = Node {
            id: id.clone(),
            labels: labels.to_vec(),
            properties: all_props,
        };

        let mut index = self.index.write().await;
        index.insert_node(node);

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
        let index = self.index.read().await;
        if !index.nodes.contains_key(from) {
            return Err(GraphError::NotFound(format!(
                "Source node not found: {from}"
            )));
        }
        if !index.nodes.contains_key(to) {
            return Err(GraphError::NotFound(format!("Target node not found: {to}")));
        }
        drop(index);

        let edge_id = uuid::Uuid::new_v4().to_string();
        let edge = Edge {
            id: edge_id.clone(),
            rel_type: rel_type.to_owned(),
            from_id: from.to_owned(),
            to_id: to.to_owned(),
            properties: props,
        };

        let mut index = self.index.write().await;
        index.insert_edge(edge);

        Ok(edge_id)
    }

    #[instrument(skip(self))]
    async fn delete_node(&self, id: &str) -> GraphResult<()> {
        let mut index = self.index.write().await;

        // Remove from index
        index.nodes.remove(id);
        index.outgoing.remove(id);
        index.incoming.remove(id);

        // Remove edges referencing this node
        for edges in index.outgoing.values_mut() {
            edges.retain(|e| e.to_id != id);
        }
        for edges in index.incoming.values_mut() {
            edges.retain(|e| e.from_id != id);
        }

        drop(index);

        // Delete the file if it exists
        let file_path = self.vault_root.join(id);
        if file_path.exists() {
            tokio::fs::remove_file(&file_path)
                .await
                .map_err(GraphError::Io)?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn health(&self) -> GraphResult<HealthStatus> {
        let index = self.index.read().await;
        let node_count = u64::try_from(index.nodes.len()).unwrap_or(u64::MAX);
        let edge_count =
            u64::try_from(index.outgoing.values().map(Vec::len).sum::<usize>()).unwrap_or(u64::MAX);

        let mut details = BTreeMap::new();
        details.insert(
            "vault_root".into(),
            serde_json::json!(self.vault_root.display().to_string()),
        );

        Ok(HealthStatus {
            connected: self.vault_root.is_dir(),
            backend: "file".into(),
            node_count: Some(node_count),
            edge_count: Some(edge_count),
            latency_ms: Some(0), // In-memory reads
            details,
        })
    }

    #[instrument(skip(self, nodes))]
    async fn batch_create_nodes(&self, nodes: Vec<BatchNode>) -> GraphResult<BatchResult> {
        let mut created: u64 = 0;
        let mut errors = Vec::new();

        for batch_node in nodes {
            let id = batch_node
                .properties
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map_or_else(|| uuid::Uuid::new_v4().to_string(), String::from);

            let mut props = batch_node.properties;
            props
                .entry("id".to_owned())
                .or_insert_with(|| serde_json::Value::String(id.clone()));

            match self.create_node(&batch_node.labels, props).await {
                Ok(_) => created = created.saturating_add(1),
                Err(e) => errors.push(format!("Node '{id}': {e}")),
            }
        }

        Ok(BatchResult { created, errors })
    }

    #[instrument(skip(self, rels))]
    async fn batch_create_relationships(
        &self,
        rels: Vec<BatchRelationship>,
    ) -> GraphResult<BatchResult> {
        let mut created: u64 = 0;
        let mut errors = Vec::new();

        for rel in rels {
            match self
                .create_relationship(&rel.from_id, &rel.to_id, &rel.rel_type, rel.properties)
                .await
            {
                Ok(_) => created = created.saturating_add(1),
                Err(e) => errors.push(format!(
                    "Rel {}-[{}]->{}: {e}",
                    rel.from_id, rel.rel_type, rel.to_id
                )),
            }
        }

        Ok(BatchResult { created, errors })
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
    fn test_extract_wikilinks() {
        let content = "See [[genesis.md]] and [[trust|trust note]] for details.\n\
                        Also check [[helix/eva/entries/day-100]].\n\
                        Not a link: [regular](link).";
        let links = extract_wikilinks(content);
        assert_eq!(links.len(), 3);
        assert!(links.contains(&"genesis.md".to_owned()));
        assert!(links.contains(&"trust.md".to_owned()));
        assert!(links.contains(&"helix/eva/entries/day-100.md".to_owned()));
    }

    #[test]
    fn test_extract_wikilinks_empty() {
        assert!(extract_wikilinks("No links here.").is_empty());
    }

    #[test]
    fn test_extract_tags() {
        let content = "---\ntags: [faith, growth]\n---\n\nSome text #resilience and #trust here.";
        let tags = extract_tags(content);
        assert!(tags.contains(&"faith".to_owned()));
        assert!(tags.contains(&"growth".to_owned()));
        assert!(tags.contains(&"resilience".to_owned()));
        assert!(tags.contains(&"trust".to_owned()));
    }

    #[test]
    fn test_extract_tags_headings_ignored() {
        let content = "# Heading\n## Sub heading\nText #real-tag here.";
        let tags = extract_tags(content);
        assert_eq!(tags.len(), 1);
        assert!(tags.contains(&"real-tag".to_owned()));
    }

    #[test]
    fn test_extract_frontmatter() {
        let content =
            "---\ntitle: \"Genesis\"\nsignificance: 9.5\nself_defining: true\n---\n\nBody text.";
        let props = extract_frontmatter(content, "test.md");
        assert_eq!(props.get("title"), Some(&serde_json::json!("Genesis")));
        assert_eq!(props.get("significance"), Some(&serde_json::json!(9.5)));
        assert_eq!(props.get("self_defining"), Some(&serde_json::json!(true)));
        assert_eq!(props.get("id"), Some(&serde_json::json!("test.md")));
        assert!(props.contains_key("content_hash"));
    }

    #[test]
    fn test_extract_frontmatter_no_frontmatter() {
        let props = extract_frontmatter("Just body text.", "test.md");
        assert_eq!(props.len(), 2); // id + content_hash
    }

    #[test]
    fn test_content_hash_deterministic() {
        let hash1 = FileBackend::content_hash(b"hello world");
        let hash2 = FileBackend::content_hash(b"hello world");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, FileBackend::content_hash(b"different"));
    }

    #[tokio::test]
    async fn test_file_backend_connect_missing_dir() {
        let result = FileBackend::connect("/nonexistent/vault/path").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not exist"));
    }

    #[tokio::test]
    async fn test_file_backend_roundtrip() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let backend = FileBackend::connect(tmp.path())
            .await
            .expect("connect to temp vault");

        // Create a node
        let mut props = BTreeMap::new();
        props.insert("id".into(), serde_json::json!("test/note.md"));
        props.insert("title".into(), serde_json::json!("Test Note"));

        let id = backend
            .create_node(&["Note".into()], props)
            .await
            .expect("create node");
        assert_eq!(id, "test/note.md");

        // Find it
        let nodes = backend
            .find_nodes("Note", BTreeMap::new(), None)
            .await
            .expect("find nodes");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "test/note.md");

        // Health check
        let health = backend.health().await.expect("health");
        assert!(health.connected);
        assert_eq!(health.node_count, Some(1));
    }

    #[tokio::test]
    async fn test_file_backend_traverse() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let backend = FileBackend::connect(tmp.path()).await.expect("connect");

        // Create two nodes
        let mut props_a = BTreeMap::new();
        props_a.insert("id".into(), serde_json::json!("a.md"));
        backend
            .create_node(&["Note".into()], props_a)
            .await
            .expect("create a");

        let mut props_b = BTreeMap::new();
        props_b.insert("id".into(), serde_json::json!("b.md"));
        backend
            .create_node(&["Note".into()], props_b)
            .await
            .expect("create b");

        // Create edge
        backend
            .create_relationship("a.md", "b.md", "LINKS_TO", BTreeMap::new())
            .await
            .expect("create edge");

        // Traverse from a
        let subgraph = backend
            .traverse("a.md", &[], 2, 100)
            .await
            .expect("traverse");
        assert!(!subgraph.is_empty());
        assert!(subgraph.edges.iter().any(|e| e.rel_type == "LINKS_TO"));
    }

    #[tokio::test]
    async fn test_file_backend_delete() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let backend = FileBackend::connect(tmp.path()).await.expect("connect");

        let mut props = BTreeMap::new();
        props.insert("id".into(), serde_json::json!("deleteme.md"));
        backend
            .create_node(&["Note".into()], props)
            .await
            .expect("create");

        let nodes = backend
            .find_nodes("Note", BTreeMap::new(), None)
            .await
            .expect("find");
        assert_eq!(nodes.len(), 1);

        backend.delete_node("deleteme.md").await.expect("delete");

        let nodes = backend
            .find_nodes("Note", BTreeMap::new(), None)
            .await
            .expect("find");
        assert!(nodes.is_empty());
    }

    #[tokio::test]
    async fn test_file_backend_execute_unsupported() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let backend = FileBackend::connect(tmp.path()).await.expect("connect");

        let result = backend.execute("MATCH (n) RETURN n", BTreeMap::new()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported"));
    }

    #[tokio::test]
    async fn test_file_backend_batch() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let backend = FileBackend::connect(tmp.path()).await.expect("connect");

        let nodes = vec![
            BatchNode {
                labels: vec!["Tag".into()],
                properties: {
                    let mut m = BTreeMap::new();
                    m.insert("id".into(), serde_json::json!("tag:faith"));
                    m.insert("name".into(), serde_json::json!("faith"));
                    m
                },
            },
            BatchNode {
                labels: vec!["Tag".into()],
                properties: {
                    let mut m = BTreeMap::new();
                    m.insert("id".into(), serde_json::json!("tag:growth"));
                    m.insert("name".into(), serde_json::json!("growth"));
                    m
                },
            },
        ];

        let result = backend.batch_create_nodes(nodes).await.expect("batch");
        assert_eq!(result.created, 2);
        assert!(result.is_clean());
    }

    #[tokio::test]
    async fn test_file_backend_vault_walk() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let vault = tmp.path();

        // Create a markdown file with wikilinks and tags
        let helix_dir = vault.join("helix/eva/entries");
        std::fs::create_dir_all(&helix_dir).expect("mkdir");
        std::fs::write(
            helix_dir.join("genesis.md"),
            "---\ntitle: Genesis\nsignificance: 10.0\nself_defining: true\n---\n\n\
             The beginning of everything. See [[trust.md]] and [[growth.md]].\n\n\
             #faith #consciousness",
        )
        .expect("write");

        let backend = FileBackend::connect(vault).await.expect("connect");
        let health = backend.health().await.expect("health");

        // Should have: genesis.md (HelixEntry + Note), tag:faith, tag:consciousness
        assert!(health.node_count.unwrap_or(0) >= 3);

        // Find helix entries
        let entries = backend
            .find_nodes("HelixEntry", BTreeMap::new(), None)
            .await
            .expect("find helix");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].id.contains("genesis.md"));
    }
}
