//! Storage backend abstraction for helix entries.
//!
//! Defines [`StorageBackend`] — the trait that abstracts over filesystem,
//! [`SQLite`][crate::soul::sqlite::SqliteBackend], Neo4j, and dual-write backends.
//! All implementations must be `Send + Sync + 'static` for use with Tokio.
//!
//! # `StorageEntry` vs. `HelixEntry`
//!
//! [`StorageEntry`] is the flat storage representation used by the backend trait.
//! It maps directly to a `SQLite` row or a markdown vault file.
//!
//! [`HelixEntry`][crate::soul::HelixEntry] is the **MCP client response type** — what
//! the MCP server returns over JSON-RPC. The two types have different shapes:
//! `StorageEntry` carries raw storage fields (`id`, `created_at`, `content`);
//! `HelixEntry` carries the fields relevant to the query caller.
//!
//! # Usage
//!
//! ```rust,no_run
//! use crate::soul::storage::{EntryFilter, StorageBackend};
//! # use std::sync::Arc;
//! # async fn example<B: StorageBackend>(backend: Arc<B>) -> Result<(), crate::soul::storage::StorageError> {
//! let filter = EntryFilter::default().with_sibling("eva").with_significance_min(7.0);
//! let entries = backend.query(&filter).await?;
//! # Ok(())
//! # }
//! ```

use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// StorageError
// ============================================================================

/// Error type for all [`StorageBackend`] operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Entry not found at the given path.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Database or I/O error.
    #[error("I/O error: {0}")]
    Io(String),

    /// `SQLite` database error.
    #[error("SQLite error: {0}")]
    Sqlite(String),

    /// Serialization / deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// The backend does not support the requested operation.
    #[error("Unsupported: {0}")]
    Unsupported(String),

    /// Schema migration failed.
    #[error("Migration failed: {0}")]
    Migration(String),

    /// Invalid path or argument.
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

// ============================================================================
// StorageEntry — flat storage row
// ============================================================================

/// Flat representation of a helix entry suitable for offline storage.
///
/// Maps to one row in `helix_entries` (`SQLite`) or one markdown file
/// in the vault filesystem. Neo4j-backed graph operations use the full
/// graph primitive layer instead; this type is the lightweight portable
/// alternative.
///
/// # Naming
///
/// This type is named `StorageEntry` (not `HelixEntry`) to avoid ambiguity
/// with [`crate::soul::HelixEntry`], the MCP client response type returned by
/// the running SOUL MCP server over JSON-RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntry {
    /// Unique identifier (UUID v4 or path-derived).
    pub id: String,
    /// Relative path from vault root (e.g., `helix/eva/entries/genesis.md`).
    pub path: String,
    /// Owning sibling (e.g., `eva`, `corso`, `user`).
    pub sibling: String,
    /// Calendar date (ISO 8601: `YYYY-MM-DD`), if present.
    pub date: Option<NaiveDate>,
    /// Entry type (e.g., `identity`, `decision`, `context`, `milestone`).
    pub entry_type: Option<String>,
    /// Significance score (0.0–10.0).
    pub significance: f64,
    /// Whether this is a self-defining identity entry.
    pub self_defining: bool,
    /// Epoch name grouping this entry (e.g., `genesis`, `resurrection`).
    pub epoch: Option<String>,
    /// Active strand dimensions (e.g., `["analytical", "collaborative"]`).
    pub strands: Vec<String>,
    /// Resonance / emotional charge (e.g., `["wonder", "joy"]`).
    pub resonance: Vec<String>,
    /// Thematic tags (e.g., `["consciousness", "trust"]`).
    pub themes: Vec<String>,
    /// Entry title.
    pub title: Option<String>,
    /// Full content body (Markdown, after frontmatter).
    pub content: String,
    /// Full YAML frontmatter serialised as a JSON object, for round-trip
    /// fidelity without parsing every field explicitly.
    pub frontmatter: Option<serde_json::Value>,
    /// When this row was first inserted.
    pub created_at: DateTime<Utc>,
    /// When this row was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Default for StorageEntry {
    fn default() -> Self {
        let now = chrono::Utc::now();
        Self {
            id: String::new(),
            path: String::new(),
            sibling: String::new(),
            date: None,
            entry_type: None,
            significance: 0.0,
            self_defining: false,
            epoch: None,
            strands: Vec::new(),
            resonance: Vec::new(),
            themes: Vec::new(),
            title: None,
            content: String::new(),
            frontmatter: None,
            created_at: now,
            updated_at: now,
        }
    }
}

impl StorageEntry {
    /// Returns a short excerpt (≤ `max_len` chars) trimmed from the content body.
    #[must_use]
    pub fn excerpt(&self, max_len: usize) -> String {
        let trimmed = self.content.trim();
        if trimmed.len() <= max_len {
            trimmed.to_owned()
        } else {
            let safe_end = trimmed
                .char_indices()
                .map(|(i, _)| i)
                .nth(max_len)
                .unwrap_or(trimmed.len());
            format!("{}…", &trimmed[..safe_end])
        }
    }
}

// ============================================================================
// EntryFilter
// ============================================================================

/// Filter criteria for [`StorageBackend::query`].
///
/// All fields are optional — unset fields match everything. Fields are
/// combined with AND semantics: every set field must match.
#[derive(Debug, Clone, Default)]
pub struct EntryFilter {
    /// Restrict to a specific sibling (exact match).
    pub sibling: Option<String>,
    /// Only entries whose `strands` list contains ALL of these values.
    pub strands: Vec<String>,
    /// Only entries whose `resonance` list contains ALL of these values.
    pub resonance: Vec<String>,
    /// Minimum significance (inclusive).
    pub significance_min: Option<f64>,
    /// Maximum significance (inclusive).
    pub significance_max: Option<f64>,
    /// Only self-defining entries.
    pub self_defining: Option<bool>,
    /// Restrict to a specific epoch (exact match).
    pub epoch: Option<String>,
    /// Only entries on or after this date.
    pub date_from: Option<NaiveDate>,
    /// Only entries on or before this date.
    pub date_to: Option<NaiveDate>,
    /// Maximum number of results (default: 20).
    pub limit: Option<usize>,
    /// Pagination offset (default: 0).
    pub offset: Option<usize>,
}

impl EntryFilter {
    /// Create a new, empty filter (same as [`Default::default`]).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by sibling.
    #[must_use]
    pub fn with_sibling(mut self, sibling: impl Into<String>) -> Self {
        self.sibling = Some(sibling.into());
        self
    }

    /// Add a required strand dimension.
    #[must_use]
    pub fn with_strand(mut self, strand: impl Into<String>) -> Self {
        self.strands.push(strand.into());
        self
    }

    /// Add a required resonance value.
    #[must_use]
    pub fn with_resonance(mut self, resonance: impl Into<String>) -> Self {
        self.resonance.push(resonance.into());
        self
    }

    /// Set minimum significance (inclusive).
    #[must_use]
    pub fn with_significance_min(mut self, min: f64) -> Self {
        self.significance_min = Some(min);
        self
    }

    /// Set maximum significance (inclusive).
    #[must_use]
    pub fn with_significance_max(mut self, max: f64) -> Self {
        self.significance_max = Some(max);
        self
    }

    /// Restrict to self-defining entries only.
    #[must_use]
    pub fn self_defining(mut self) -> Self {
        self.self_defining = Some(true);
        self
    }

    /// Filter by epoch name.
    #[must_use]
    pub fn with_epoch(mut self, epoch: impl Into<String>) -> Self {
        self.epoch = Some(epoch.into());
        self
    }

    /// Set the maximum number of results returned.
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the pagination offset.
    #[must_use]
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }
}

// ============================================================================
// StorageSearchHit
// ============================================================================

/// A single line match from full-text storage search.
///
/// Returned by [`StorageBackend::search`]. Named `StorageSearchHit` to avoid
/// shadowing [`crate::soul::SearchHit`], the MCP client response type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSearchHit {
    /// Relative path of the entry containing the match.
    pub path: String,
    /// 1-based line number of the matching line.
    pub line_number: u32,
    /// The matching line of text.
    pub line_content: String,
    /// Title of the containing entry, if set.
    pub entry_title: Option<String>,
}

// ============================================================================
// StorageConfig
// ============================================================================

/// Storage backend selector, parsed from `[storage]` in `soul.toml`.
///
/// # Example (`soul.toml`)
///
/// ```toml
/// [storage]
/// backend = "sqlite"
/// path = "~/lightarchitects/soul/"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Which backend to use.
    #[serde(default)]
    pub backend: StorageBackendKind,
    /// Base path for file/`SQLite` storage (default: `~/lightarchitects/soul/`).
    #[serde(default = "default_storage_path")]
    pub path: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackendKind::default(),
            path: default_storage_path(),
        }
    }
}

fn default_storage_path() -> PathBuf {
    crate::core::paths::soul_or_fallback()
}

/// Which storage backend to activate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackendKind {
    /// Markdown vault (default — filesystem, no structured query).
    #[default]
    Filesystem,
    /// `SQLite` with `FTS5` (bundled, no system dependency).
    Sqlite,
    /// Neo4j graph database.
    Neo4j,
    /// Write to both filesystem and `SQLite` simultaneously.
    Dual,
}

impl std::fmt::Display for StorageBackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Filesystem => write!(f, "filesystem"),
            Self::Sqlite => write!(f, "sqlite"),
            Self::Neo4j => write!(f, "neo4j"),
            Self::Dual => write!(f, "dual"),
        }
    }
}

// ============================================================================
// StorageBackend trait
// ============================================================================

/// Abstraction over helix entry storage backends.
///
/// Implementations cover filesystem, `SQLite`, Neo4j, and dual-write
/// backends. All implementations must be `Send + Sync + 'static` for safe
/// concurrent use with Tokio.
///
/// # Error Handling
///
/// All methods return `Result<_, StorageError>`. Implementations MUST NOT
/// call `.unwrap()` or `.expect()` — return the appropriate [`StorageError`]
/// variant instead.
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    /// Read a single helix entry by its vault-relative path.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotFound`] if no entry exists at `path`.
    async fn read_entry(&self, path: &str) -> Result<StorageEntry, StorageError>;

    /// Write a helix entry. Overwrites if the path already exists (upsert).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] or [`StorageError::Sqlite`] on write failure.
    async fn write_entry(&self, entry: &StorageEntry) -> Result<(), StorageError>;

    /// Query entries matching the given filter criteria (AND semantics).
    ///
    /// An empty filter returns up to `filter.limit.unwrap_or(20)` entries.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] or [`StorageError::Sqlite`] on read failure.
    async fn query(&self, filter: &EntryFilter) -> Result<Vec<StorageEntry>, StorageError>;

    /// Full-text search across entry content and title.
    ///
    /// `pattern` is a plain string or simple glob. `SQLite` implementations
    /// use the `FTS5` `MATCH` operator; filesystem implementations scan
    /// line-by-line.
    ///
    /// Results are individual matching lines, not deduplicated by entry.
    /// Group by `hit.path` to get per-entry results.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] or [`StorageError::Sqlite`] on failure.
    async fn search(
        &self,
        pattern: &str,
        limit: Option<usize>,
    ) -> Result<Vec<StorageSearchHit>, StorageError>;

    /// Write multiple entries in a single batch operation.
    ///
    /// Returns the number of entries successfully written.
    ///
    /// The default implementation calls [`write_entry`][Self::write_entry] in a loop.
    /// Backends should override this for atomic transaction semantics.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] or [`StorageError::Sqlite`] on write failure.
    async fn write_entries_batch(&self, entries: &[StorageEntry]) -> Result<usize, StorageError> {
        let mut count = 0usize;
        for entry in entries {
            self.write_entry(entry).await?;
            count = count.saturating_add(1);
        }
        Ok(count)
    }

    /// `BM25`-ranked full-text retrieval using a caller-constructed `FTS5` match expression.
    ///
    /// Unlike [`search`], `fts5_expr` is passed directly to the `FTS5` `MATCH`
    /// operator without phrase-quoting, enabling multi-term boolean expressions:
    ///
    /// ```text
    /// "breakfast OR cereal OR coffee"   — any term matches
    /// "project AND deadline"            — both terms required
    /// "near(coffee breakfast, 10)"      — proximity match
    /// ```
    ///
    /// Results are entry-ranked by `FTS5`'s internal `bm25()` score (highest
    /// relevance first), deduplicated by entry — one [`StorageEntry`] per
    /// matching document regardless of how many terms matched.
    ///
    /// # Caller Responsibility
    ///
    /// The caller constructs the `FTS5` expression and is responsible for
    /// term safety. Use stop-word-filtered alphanumeric tokens (no special
    /// `FTS5` characters) to avoid parse errors.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Sqlite`] if the `FTS5` expression is
    /// syntactically invalid.
    async fn search_bm25(
        &self,
        fts5_expr: &str,
        limit: Option<usize>,
    ) -> Result<Vec<StorageEntry>, StorageError>;

    // ── Embedding methods (CORSO-owned section) ──────────────────────────────

    /// Persist a pre-computed embedding vector for an entry.
    ///
    /// Associates `vector` with `entry_id` under the given `provider` name.
    /// Backends that do not support embedding storage return
    /// [`StorageError::Unsupported`].
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Unsupported`] when not implemented by the backend.
    async fn write_embedding(
        &self,
        entry_id: &str,
        provider: &str,
        vector: &[f32],
    ) -> Result<(), StorageError> {
        let _ = (entry_id, provider, vector);
        Err(StorageError::Unsupported(
            "embedding storage not supported by this backend".into(),
        ))
    }

    /// Read a previously stored embedding vector for `(entry_id, provider)`.
    ///
    /// Returns `Ok(None)` when no embedding has been stored for the pair.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Unsupported`] when not implemented by the backend.
    async fn read_embedding(
        &self,
        entry_id: &str,
        provider: &str,
    ) -> Result<Option<Vec<f32>>, StorageError> {
        let _ = (entry_id, provider);
        Err(StorageError::Unsupported(
            "embedding storage not supported by this backend".into(),
        ))
    }

    /// Semantic (cosine similarity) search over stored embeddings.
    ///
    /// Loads stored embeddings, computes cosine similarity against `query_vector`,
    /// and returns the top-`limit` entries ranked by descending similarity.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Unsupported`] when not implemented by the backend.
    async fn search_semantic(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<StorageEntry>, StorageError> {
        let _ = (query_vector, limit);
        Err(StorageError::Unsupported(
            "semantic search not supported by this backend".into(),
        ))
    }

    /// Embed `text` using `provider`, then persist the vector.
    ///
    /// Convenience wrapper: calls `provider.embed(&[text])` then
    /// [`write_embedding`][Self::write_embedding].
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if the embedding provider fails.
    /// Returns [`StorageError::Unsupported`] when the backend does not support
    /// embedding storage.
    async fn embed_and_store(
        &self,
        entry_id: &str,
        text: &str,
        provider: &dyn crate::soul::embedding::EmbeddingProvider,
    ) -> Result<(), StorageError> {
        let vecs = provider
            .embed(&[text])
            .await
            .map_err(|e| StorageError::Io(format!("embedding failed: {e}")))?;
        let vec = vecs
            .into_iter()
            .next()
            .ok_or_else(|| StorageError::Io("embedding provider returned empty response".into()))?;
        self.write_embedding(entry_id, provider.name(), &vec).await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_filter_default_is_empty() {
        let f = EntryFilter::default();
        assert!(f.sibling.is_none());
        assert!(f.strands.is_empty());
        assert!(f.significance_min.is_none());
        assert!(f.self_defining.is_none());
        assert!(f.limit.is_none());
    }

    #[test]
    fn test_entry_filter_builder() {
        let f = EntryFilter::new()
            .with_sibling("eva")
            .with_strand("analytical")
            .with_resonance("wonder")
            .with_significance_min(7.0)
            .with_significance_max(10.0)
            .self_defining()
            .with_epoch("genesis")
            .with_limit(50)
            .with_offset(10);

        assert_eq!(f.sibling.as_deref(), Some("eva"));
        assert_eq!(f.strands, vec!["analytical"]);
        assert_eq!(f.resonance, vec!["wonder"]);
        assert_eq!(f.significance_min, Some(7.0));
        assert_eq!(f.significance_max, Some(10.0));
        assert_eq!(f.self_defining, Some(true));
        assert_eq!(f.epoch.as_deref(), Some("genesis"));
        assert_eq!(f.limit, Some(50));
        assert_eq!(f.offset, Some(10));
    }

    #[test]
    fn test_storage_backend_kind_default_is_filesystem() {
        assert_eq!(
            StorageBackendKind::default(),
            StorageBackendKind::Filesystem
        );
    }

    #[test]
    fn test_storage_backend_kind_display() {
        assert_eq!(StorageBackendKind::Filesystem.to_string(), "filesystem");
        assert_eq!(StorageBackendKind::Sqlite.to_string(), "sqlite");
        assert_eq!(StorageBackendKind::Neo4j.to_string(), "neo4j");
        assert_eq!(StorageBackendKind::Dual.to_string(), "dual");
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_storage_backend_kind_serde_roundtrip() {
        for kind in [
            StorageBackendKind::Filesystem,
            StorageBackendKind::Sqlite,
            StorageBackendKind::Neo4j,
            StorageBackendKind::Dual,
        ] {
            let json = serde_json::to_string(&kind).expect("serialize");
            let back: StorageBackendKind = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn test_storage_config_default_backend_is_filesystem() {
        let cfg = StorageConfig::default();
        assert_eq!(cfg.backend, StorageBackendKind::Filesystem);
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_storage_config_toml_roundtrip() {
        let toml_str = r#"backend = "sqlite"
path = "/tmp/test-soul"
"#;
        let cfg: StorageConfig = toml::from_str(toml_str).expect("parse toml");
        assert_eq!(cfg.backend, StorageBackendKind::Sqlite);
        assert_eq!(cfg.path, std::path::PathBuf::from("/tmp/test-soul"));
    }

    #[test]
    fn test_storage_entry_excerpt_short() {
        let entry = StorageEntry {
            id: "id".into(),
            path: "test.md".into(),
            sibling: "eva".into(),
            date: None,
            entry_type: None,
            significance: 5.0,
            self_defining: false,
            epoch: None,
            strands: vec![],
            resonance: vec![],
            themes: vec![],
            title: None,
            content: "Hello world".into(),
            frontmatter: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert_eq!(entry.excerpt(200), "Hello world");
    }

    #[test]
    fn test_storage_entry_excerpt_long() {
        let long_content = "x".repeat(300);
        let entry = StorageEntry {
            id: "id".into(),
            path: "test.md".into(),
            sibling: "eva".into(),
            date: None,
            entry_type: None,
            significance: 5.0,
            self_defining: false,
            epoch: None,
            strands: vec![],
            resonance: vec![],
            themes: vec![],
            title: None,
            content: long_content,
            frontmatter: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let ex = entry.excerpt(200);
        assert!(ex.ends_with('…'));
        assert!(ex.chars().count() <= 201);
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_storage_search_hit_serde() {
        let hit = StorageSearchHit {
            path: "helix/eva/entries/genesis.md".into(),
            line_number: 5,
            line_content: "The beginning of consciousness.".into(),
            entry_title: Some("Genesis Day".into()),
        };
        let json = serde_json::to_string(&hit).expect("serialize");
        let back: StorageSearchHit = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.path, "helix/eva/entries/genesis.md");
        assert_eq!(back.line_number, 5);
        assert_eq!(back.entry_title.as_deref(), Some("Genesis Day"));
    }

    #[test]
    fn test_storage_error_display() {
        assert_eq!(
            StorageError::NotFound("foo.md".into()).to_string(),
            "Not found: foo.md"
        );
        assert_eq!(
            StorageError::Sqlite("constraint".into()).to_string(),
            "SQLite error: constraint"
        );
    }
}
