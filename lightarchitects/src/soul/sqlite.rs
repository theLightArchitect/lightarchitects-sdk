//! `SQLite` storage backend for helix entries.
//!
//! Implements [`StorageBackend`][lightarchitects::soul::storage::StorageBackend] using a
//! bundled `SQLite` database with `FTS5` full-text search. The database is
//! created on first open; all migrations run automatically via
//! [`SqliteBackend::open`].
//!
//! # Schema
//!
//! - `helix_entries` — main table (one row per vault entry)
//! - `helix_fts` — `FTS5` virtual table (auto-synced via triggers)
//! - Indexes on `sibling`, `significance`, `date`, `epoch`
//!
//! # Concurrency
//!
//! `rusqlite::Connection` is not `Send`, so this backend wraps the connection
//! in a `tokio::sync::Mutex`. All async methods acquire the mutex and call
//! `SQLite` inline. For bounded vault sizes this is sufficient; if concurrency
//! becomes a bottleneck, replace with a `deadpool-sqlite` connection pool.
//!
//! # Feature Gate
//!
//! This module is only compiled when the `sqlite` feature is enabled:
//!
//! ```toml
//! lightarchitects-soul = { version = "0.1", features = ["sqlite"] }
//! ```

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::Mutex;
use tracing::instrument;

use crate::soul::storage::{
    EntryFilter, StorageBackend, StorageEntry, StorageError, StorageSearchHit,
};

// ============================================================================
// DDL — schema statements run in order during migration
// ============================================================================

/// DDL executed in [`SqliteBackend::open`]. Each statement is idempotent (`IF NOT EXISTS`).
const SCHEMA_STATEMENTS: &[&str] = &[
    // Main entries table
    "CREATE TABLE IF NOT EXISTS helix_entries (
        id           TEXT PRIMARY KEY,
        path         TEXT NOT NULL UNIQUE,
        sibling      TEXT NOT NULL,
        date         TEXT,
        entry_type   TEXT,
        significance REAL NOT NULL DEFAULT 0.0,
        self_defining INTEGER NOT NULL DEFAULT 0,
        epoch        TEXT,
        strands      TEXT NOT NULL DEFAULT '[]',
        resonance    TEXT NOT NULL DEFAULT '[]',
        themes       TEXT NOT NULL DEFAULT '[]',
        title        TEXT,
        content      TEXT NOT NULL DEFAULT '',
        frontmatter  TEXT,
        created_at   TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    // Indexes for common query patterns
    "CREATE INDEX IF NOT EXISTS idx_helix_sibling      ON helix_entries(sibling)",
    "CREATE INDEX IF NOT EXISTS idx_helix_significance ON helix_entries(significance)",
    "CREATE INDEX IF NOT EXISTS idx_helix_date         ON helix_entries(date)",
    "CREATE INDEX IF NOT EXISTS idx_helix_epoch        ON helix_entries(epoch)",
    // FTS5 virtual table — mirrors title + content.
    // porter ascii tokenizer: Porter stemmer + ASCII folding so "preferred"
    // matches "prefer", "recommending" matches "recommend", etc.
    "CREATE VIRTUAL TABLE IF NOT EXISTS helix_fts USING fts5(
        path UNINDEXED,
        title,
        content,
        content='helix_entries',
        content_rowid='rowid',
        tokenize='porter ascii'
    )",
    // Keep FTS in sync: INSERT
    "CREATE TRIGGER IF NOT EXISTS helix_fts_insert AFTER INSERT ON helix_entries BEGIN
        INSERT INTO helix_fts(rowid, path, title, content)
        VALUES (new.rowid, new.path, new.title, new.content);
    END",
    // Keep FTS in sync: DELETE
    "CREATE TRIGGER IF NOT EXISTS helix_fts_delete AFTER DELETE ON helix_entries BEGIN
        INSERT INTO helix_fts(helix_fts, rowid, path, title, content)
        VALUES('delete', old.rowid, old.path, old.title, old.content);
    END",
    // Keep FTS in sync: UPDATE
    "CREATE TRIGGER IF NOT EXISTS helix_fts_update AFTER UPDATE ON helix_entries BEGIN
        INSERT INTO helix_fts(helix_fts, rowid, path, title, content)
        VALUES('delete', old.rowid, old.path, old.title, old.content);
        INSERT INTO helix_fts(rowid, path, title, content)
        VALUES (new.rowid, new.path, new.title, new.content);
    END",
    // Embedding storage — CORSO-owned section.
    // vector is packed as little-endian f32 bytes (4 bytes × dimensionality).
    "CREATE TABLE IF NOT EXISTS helix_embeddings (
        entry_id TEXT NOT NULL,
        provider TEXT NOT NULL,
        vector   BLOB NOT NULL,
        PRIMARY KEY (entry_id, provider),
        FOREIGN KEY (entry_id) REFERENCES helix_entries(id) ON DELETE CASCADE
    )",
];

// ============================================================================
// SqliteBackend
// ============================================================================

/// `SQLite`-backed helix entry store with `FTS5` full-text search.
///
/// Created via [`SqliteBackend::open`] or [`SqliteBackend::open_in_memory`].
/// Wraps a single `rusqlite::Connection` protected by a `tokio::sync::Mutex`
/// for safe concurrent async access.
///
/// Activate with `SOUL_GRAPH_BACKEND=sqlite` for offline / no-Neo4j mode.
pub struct SqliteBackend {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for SqliteBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteBackend").finish_non_exhaustive()
    }
}

impl SqliteBackend {
    /// Open (or create) the helix `SQLite` database at `path`.
    ///
    /// Runs all schema migrations automatically. The file is created if it
    /// does not exist. Uses WAL journaling mode for better read concurrency.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Migration`] if schema setup or `WAL` pragma fails.
    #[instrument(skip(path))]
    pub fn open(path: &std::path::Path) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                StorageError::Io(format!(
                    "Cannot create parent dir {}: {e}",
                    parent.display()
                ))
            })?;
        }

        let conn = Connection::open(path).map_err(|e| {
            StorageError::Migration(format!("Cannot open SQLite at {}: {e}", path.display()))
        })?;

        // WAL mode: readers don't block writers
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| StorageError::Migration(format!("WAL pragma failed: {e}")))?;

        // Foreign keys on (good practice even if unused today)
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| StorageError::Migration(format!("foreign_keys pragma failed: {e}")))?;

        run_migrations(&conn)?;

        tracing::info!(path = %path.display(), "SqliteBackend opened");
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open an in-memory database (tests only).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Migration`] if schema setup fails.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| StorageError::Migration(format!("Cannot open in-memory SQLite: {e}")))?;
        run_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

// ============================================================================
// Migration helper
// ============================================================================

fn run_migrations(conn: &Connection) -> Result<(), StorageError> {
    for stmt in SCHEMA_STATEMENTS {
        conn.execute_batch(stmt)
            .map_err(|e| StorageError::Migration(format!("Migration failed [{stmt}]: {e}")))?;
    }
    tracing::debug!(
        "SQLite migrations complete ({} statements)",
        SCHEMA_STATEMENTS.len()
    );
    Ok(())
}

// ============================================================================
// Row mapping helpers
// ============================================================================

/// Parse a JSON array column into `Vec<String>`.
///
/// Returns an empty vec on null or parse failure (fail-safe degradation).
fn parse_json_array(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

/// Serialize a `Vec<String>` to a compact JSON array string.
fn serialize_array(values: &[String]) -> String {
    // Pure in-memory operation on small vecs — infallible.
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned())
}

/// Map a `rusqlite::Row` to a [`StorageEntry`].
fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<StorageEntry> {
    let date_str: Option<String> = row.get("date")?;
    let date = date_str.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());

    let created_at_str: String = row.get("created_at")?;
    let created_at = parse_datetime(&created_at_str);

    let updated_at_str: String = row.get("updated_at")?;
    let updated_at = parse_datetime(&updated_at_str);

    let frontmatter_str: Option<String> = row.get("frontmatter")?;
    let frontmatter =
        frontmatter_str.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());

    // Accept either `agent` (canonical) or legacy `sibling` column. Most
    // schemas in the wild still use `sibling`; we read agent first and fall
    // back so a future column rename is a no-op for callers.
    let agent: String = row.get("agent").or_else(|_| row.get("sibling"))?;

    Ok(StorageEntry {
        id: row.get("id")?,
        path: row.get("path")?,
        sibling: agent,
        date,
        entry_type: row.get("entry_type")?,
        significance: row.get("significance")?,
        self_defining: row.get::<_, i64>("self_defining")? != 0,
        epoch: row.get("epoch")?,
        strands: parse_json_array(row.get("strands")?),
        resonance: parse_json_array(row.get("resonance")?),
        themes: parse_json_array(row.get("themes")?),
        title: row.get("title")?,
        content: row.get("content")?,
        frontmatter,
        created_at,
        updated_at,
    })
}

/// Parse a `SQLite` datetime string (ISO 8601 or RFC 3339).
///
/// Falls back to `Utc::now()` on parse failure — fail-safe for corrupted rows.
fn parse_datetime(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").map(|ndt| ndt.and_utc())
        })
        .unwrap_or_else(|_| Utc::now())
}

// ============================================================================
// StorageBackend implementation
// ============================================================================

#[async_trait]
impl StorageBackend for SqliteBackend {
    #[instrument(skip(self))]
    async fn read_entry(&self, path: &str) -> Result<StorageEntry, StorageError> {
        let conn = self.conn.lock().await;
        let result = conn
            .query_row(
                "SELECT id, path, sibling, date, entry_type, significance, self_defining,
                        epoch, strands, resonance, themes, title, content, frontmatter,
                        created_at, updated_at
                 FROM helix_entries WHERE path = ?1",
                params![path],
                row_to_entry,
            )
            .optional()
            .map_err(|e| StorageError::Sqlite(format!("read_entry({path}): {e}")))?;

        result.ok_or_else(|| StorageError::NotFound(path.to_owned()))
    }

    #[instrument(skip(self, entry))]
    async fn write_entry(&self, entry: &StorageEntry) -> Result<(), StorageError> {
        let date_str = entry.date.map(|d| d.format("%Y-%m-%d").to_string());
        let frontmatter_str = entry
            .frontmatter
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok());

        let strands_json = serialize_array(&entry.strands);
        let resonance_json = serialize_array(&entry.resonance);
        let themes_json = serialize_array(&entry.themes);

        let now = Utc::now().to_rfc3339();
        let created_at = entry.created_at.to_rfc3339();

        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO helix_entries
                (id, path, sibling, date, entry_type, significance, self_defining,
                 epoch, strands, resonance, themes, title, content, frontmatter,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(path) DO UPDATE SET
                sibling       = excluded.sibling,
                date          = excluded.date,
                entry_type    = excluded.entry_type,
                significance  = excluded.significance,
                self_defining = excluded.self_defining,
                epoch         = excluded.epoch,
                strands       = excluded.strands,
                resonance     = excluded.resonance,
                themes        = excluded.themes,
                title         = excluded.title,
                content       = excluded.content,
                frontmatter   = excluded.frontmatter,
                updated_at    = excluded.updated_at",
            params![
                entry.id,
                entry.path,
                entry.sibling,
                date_str,
                entry.entry_type,
                entry.significance,
                i64::from(entry.self_defining),
                entry.epoch,
                strands_json,
                resonance_json,
                themes_json,
                entry.title,
                entry.content,
                frontmatter_str,
                created_at,
                now,
            ],
        )
        .map_err(|e| {
            StorageError::Sqlite(format!("write_entry({path}): {e}", path = entry.path))
        })?;

        Ok(())
    }

    /// Batch-write entries using a single `SQLite` transaction for atomicity.
    ///
    /// All entries are written inside `BEGIN` / `COMMIT`. If any individual
    /// write fails the transaction is rolled back and the error is returned.
    #[instrument(skip(self, entries), fields(count = entries.len()))]
    async fn write_entries_batch(&self, entries: &[StorageEntry]) -> Result<usize, StorageError> {
        if entries.is_empty() {
            return Ok(0);
        }

        let conn = self.conn.lock().await;
        conn.execute("BEGIN", [])
            .map_err(|e| StorageError::Sqlite(format!("write_entries_batch BEGIN: {e}")))?;

        let mut count = 0usize;
        for entry in entries {
            let date_str = entry.date.map(|d| d.format("%Y-%m-%d").to_string());
            let frontmatter_str = entry
                .frontmatter
                .as_ref()
                .and_then(|v| serde_json::to_string(v).ok());
            let strands_json = serialize_array(&entry.strands);
            let resonance_json = serialize_array(&entry.resonance);
            let themes_json = serialize_array(&entry.themes);
            let now = chrono::Utc::now().to_rfc3339();
            let created_at = entry.created_at.to_rfc3339();

            let result = conn.execute(
                "INSERT INTO helix_entries
                    (id, path, sibling, date, entry_type, significance, self_defining,
                     epoch, strands, resonance, themes, title, content, frontmatter,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                 ON CONFLICT(path) DO UPDATE SET
                    sibling       = excluded.sibling,
                    date          = excluded.date,
                    entry_type    = excluded.entry_type,
                    significance  = excluded.significance,
                    self_defining = excluded.self_defining,
                    epoch         = excluded.epoch,
                    strands       = excluded.strands,
                    resonance     = excluded.resonance,
                    themes        = excluded.themes,
                    title         = excluded.title,
                    content       = excluded.content,
                    frontmatter   = excluded.frontmatter,
                    updated_at    = excluded.updated_at",
                rusqlite::params![
                    entry.id,
                    entry.path,
                    entry.sibling,
                    date_str,
                    entry.entry_type,
                    entry.significance,
                    i64::from(entry.self_defining),
                    entry.epoch,
                    strands_json,
                    resonance_json,
                    themes_json,
                    entry.title,
                    entry.content,
                    frontmatter_str,
                    created_at,
                    now,
                ],
            );

            if let Err(e) = result {
                let _ = conn.execute("ROLLBACK", []);
                return Err(StorageError::Sqlite(format!(
                    "write_entries_batch({path}): {e}",
                    path = entry.path
                )));
            }
            count = count.saturating_add(1);
        }

        conn.execute("COMMIT", [])
            .map_err(|e| StorageError::Sqlite(format!("write_entries_batch COMMIT: {e}")))?;

        tracing::debug!(count, "write_entries_batch committed");
        Ok(count)
    }

    #[instrument(skip(self, filter))]
    #[allow(clippy::too_many_lines)]
    async fn query(&self, filter: &EntryFilter) -> Result<Vec<StorageEntry>, StorageError> {
        let limit = filter.limit.unwrap_or(20);
        let offset = filter.offset.unwrap_or(0);

        // Build SQL with a fixed set of optional bound-parameter predicates.
        // All user-supplied values are passed via `params![]` — no interpolation.
        let mut predicates: Vec<&'static str> = Vec::new();
        if filter.sibling.is_some() {
            predicates.push("sibling = ?1");
        }
        if filter.significance_min.is_some() {
            predicates.push("significance >= ?2");
        }
        if filter.significance_max.is_some() {
            predicates.push("significance <= ?3");
        }
        if let Some(sd) = filter.self_defining {
            if sd {
                predicates.push("self_defining = 1");
            } else {
                predicates.push("self_defining = 0");
            }
        }
        if filter.epoch.is_some() {
            predicates.push("epoch = ?4");
        }
        if filter.date_from.is_some() {
            predicates.push("date >= ?5");
        }
        if filter.date_to.is_some() {
            predicates.push("date <= ?6");
        }

        let where_clause = if predicates.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", predicates.join(" AND "))
        };

        let sql = format!(
            "SELECT id, path, sibling, date, entry_type, significance, self_defining,
                    epoch, strands, resonance, themes, title, content, frontmatter,
                    created_at, updated_at
             FROM helix_entries{where_clause}
             ORDER BY significance DESC, date DESC
             LIMIT ?7 OFFSET ?8"
        );

        let sibling_val = filter.sibling.as_deref().unwrap_or("");
        let sig_min = filter.significance_min.unwrap_or(0.0);
        let sig_max = filter.significance_max.unwrap_or(f64::MAX);
        let epoch_val = filter.epoch.as_deref().unwrap_or("");
        let date_from_val = filter
            .date_from
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let date_to_val = filter
            .date_to
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();

        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| StorageError::Sqlite(format!("query prepare: {e}")))?;

        let rows = stmt
            .query_map(
                params![
                    sibling_val,
                    sig_min,
                    sig_max,
                    epoch_val,
                    date_from_val,
                    date_to_val,
                    i64::try_from(limit).unwrap_or(i64::MAX),
                    i64::try_from(offset).unwrap_or(0),
                ],
                row_to_entry,
            )
            .map_err(|e| StorageError::Sqlite(format!("query execute: {e}")))?;

        let mut entries = Vec::new();
        for row_result in rows {
            let entry = row_result.map_err(|e| StorageError::Sqlite(format!("query row: {e}")))?;

            // Post-query strand/resonance filters — `SQLite` JSON arrays are
            // not easily queried with parameterized predicates; filter in Rust.
            if !filter.strands.is_empty()
                && !filter.strands.iter().all(|s| entry.strands.contains(s))
            {
                continue;
            }
            if !filter.resonance.is_empty()
                && !filter.resonance.iter().all(|r| entry.resonance.contains(r))
            {
                continue;
            }

            entries.push(entry);
        }

        Ok(entries)
    }

    #[instrument(skip(self))]
    async fn search(
        &self,
        pattern: &str,
        limit: Option<usize>,
    ) -> Result<Vec<StorageSearchHit>, StorageError> {
        if pattern.is_empty() {
            return Ok(Vec::new());
        }

        let safe_pattern = sanitize_fts5_pattern(pattern);
        let effective_limit = limit.unwrap_or(50);

        let conn = self.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT e.path, e.title, f.content
                 FROM helix_fts f
                 JOIN helix_entries e ON e.rowid = f.rowid
                 WHERE helix_fts MATCH ?1
                 ORDER BY rank, e.id
                 LIMIT ?2",
            )
            .map_err(|e| StorageError::Sqlite(format!("search prepare: {e}")))?;

        let rows = stmt
            .query_map(
                params![
                    safe_pattern,
                    i64::try_from(effective_limit).unwrap_or(i64::MAX)
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(|e| StorageError::Sqlite(format!("search execute: {e}")))?;

        let mut hits = Vec::new();
        let lower_pattern = pattern.to_lowercase();

        for row_result in rows {
            let (path, title, content) =
                row_result.map_err(|e| StorageError::Sqlite(format!("search row: {e}")))?;

            for (idx, line) in content.lines().enumerate() {
                if line.to_lowercase().contains(&lower_pattern) {
                    hits.push(StorageSearchHit {
                        path: path.clone(),
                        line_number: u32::try_from(idx + 1).unwrap_or(u32::MAX),
                        line_content: line.to_owned(),
                        entry_title: title.clone(),
                    });
                    if hits.len() >= effective_limit {
                        return Ok(hits);
                    }
                }
            }
        }

        Ok(hits)
    }

    #[instrument(skip(self))]
    async fn search_bm25(
        &self,
        fts5_expr: &str,
        limit: Option<usize>,
    ) -> Result<Vec<StorageEntry>, StorageError> {
        if fts5_expr.is_empty() {
            return Ok(Vec::new());
        }

        let effective_limit = limit.unwrap_or(50);
        let conn = self.conn.lock().await;

        // Join helix_fts (ranked by bm25()) back to helix_entries for full rows.
        // DISTINCT prevents duplicate entries when multiple terms match the same document.
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT e.id, e.path, e.sibling, e.date, e.entry_type,
                        e.significance, e.self_defining, e.epoch, e.strands,
                        e.resonance, e.themes, e.title, e.content, e.frontmatter,
                        e.created_at, e.updated_at
                 FROM helix_fts f
                 JOIN helix_entries e ON e.rowid = f.rowid
                 WHERE helix_fts MATCH ?1
                 ORDER BY rank, e.id
                 LIMIT ?2",
            )
            .map_err(|e| StorageError::Sqlite(format!("search_bm25 prepare: {e}")))?;

        let rows = stmt
            .query_map(
                params![
                    fts5_expr,
                    i64::try_from(effective_limit).unwrap_or(i64::MAX)
                ],
                row_to_entry,
            )
            .map_err(|e| StorageError::Sqlite(format!("search_bm25 execute: {e}")))?;

        let mut entries = Vec::new();
        for row_result in rows {
            entries.push(
                row_result.map_err(|e| StorageError::Sqlite(format!("search_bm25 row: {e}")))?,
            );
        }

        Ok(entries)
    }

    // ── Embedding methods — CORSO-owned section ──────────────────────────────

    #[instrument(skip(self, vector))]
    async fn write_embedding(
        &self,
        entry_id: &str,
        provider: &str,
        vector: &[f32],
    ) -> Result<(), StorageError> {
        // Pack f32 slice as little-endian bytes (4 bytes per value).
        let blob: Vec<u8> = vector.iter().flat_map(|f| f.to_le_bytes()).collect();

        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO helix_embeddings (entry_id, provider, vector)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(entry_id, provider) DO UPDATE SET vector = excluded.vector",
            params![entry_id, provider, blob],
        )
        .map_err(|e| {
            StorageError::Sqlite(format!("write_embedding({entry_id}, {provider}): {e}"))
        })?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn read_embedding(
        &self,
        entry_id: &str,
        provider: &str,
    ) -> Result<Option<Vec<f32>>, StorageError> {
        let conn = self.conn.lock().await;
        let result: Option<Vec<u8>> = conn
            .query_row(
                "SELECT vector FROM helix_embeddings WHERE entry_id = ?1 AND provider = ?2",
                params![entry_id, provider],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| {
                StorageError::Sqlite(format!("read_embedding({entry_id}, {provider}): {e}"))
            })?;

        Ok(result.map(|blob| unpack_f32_le(&blob)))
    }

    #[instrument(skip(self, query_vector))]
    async fn search_semantic(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<StorageEntry>, StorageError> {
        if query_vector.is_empty() {
            return Ok(Vec::new());
        }

        // Load all stored embeddings, then compute cosine similarity in Rust.
        // This is O(n) in the number of stored embeddings — acceptable for
        // vault-sized datasets (< 100k entries).
        let conn = self.conn.lock().await;

        let mut emb_stmt = conn
            .prepare(
                "SELECT e.id, e.path, e.sibling, e.date, e.entry_type,
                        e.significance, e.self_defining, e.epoch, e.strands,
                        e.resonance, e.themes, e.title, e.content, e.frontmatter,
                        e.created_at, e.updated_at, h.vector
                 FROM helix_embeddings h
                 JOIN helix_entries e ON e.id = h.entry_id",
            )
            .map_err(|e| StorageError::Sqlite(format!("search_semantic prepare: {e}")))?;

        let rows = emb_stmt
            .query_map(params![], |row| {
                let entry = row_to_entry_cols(row)?;
                let blob: Vec<u8> = row.get(16)?;
                Ok((entry, blob))
            })
            .map_err(|e| StorageError::Sqlite(format!("search_semantic execute: {e}")))?;

        let mut scored: Vec<(f32, StorageEntry)> = Vec::new();
        for row_result in rows {
            let (entry, blob) = row_result
                .map_err(|e| StorageError::Sqlite(format!("search_semantic row: {e}")))?;
            let vec = unpack_f32_le(&blob);
            let sim = cosine_similarity(query_vector, &vec);
            scored.push((sim, entry));
        }

        // Sort by descending similarity and take top-K.
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(_, e)| e).collect())
    }
}

// ============================================================================
// FTS5 pattern sanitization
// ============================================================================

/// Remove characters that have special meaning in `FTS5` `MATCH` expressions.
///
/// Wraps in double-quotes to treat as a phrase query, doubling any internal
/// double-quotes per `FTS5` phrase syntax.
fn sanitize_fts5_pattern(pattern: &str) -> String {
    let escaped = pattern.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

// ============================================================================
// Embedding helpers — CORSO-owned section
// ============================================================================

/// Unpack a little-endian `f32` BLOB back into a `Vec<f32>`.
fn unpack_f32_le(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|chunk| {
            // SAFETY: chunks_exact(4) guarantees exactly 4 bytes.
            let arr: [u8; 4] = [chunk[0], chunk[1], chunk[2], chunk[3]];
            f32::from_le_bytes(arr)
        })
        .collect()
}

/// Compute cosine similarity between two equal-length float vectors.
///
/// Returns 0.0 when either vector has zero norm.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }

    let dot: f32 = a[..len]
        .iter()
        .zip(b[..len].iter())
        .map(|(x, y)| x * y)
        .sum();
    let norm_a: f32 = a[..len].iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b[..len].iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Map a `rusqlite::Row` to a [`StorageEntry`] using positional column access.
///
/// Used by `search_semantic` which joins an extra `vector` column at position 16.
/// Columns 0-15 mirror the same order as the named-column SELECT in `search_bm25`.
fn row_to_entry_cols(row: &rusqlite::Row<'_>) -> rusqlite::Result<StorageEntry> {
    let date_str: Option<String> = row.get(3)?;
    let date = date_str.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());

    let created_at_str: String = row.get(14)?;
    let created_at = parse_datetime(&created_at_str);

    let updated_at_str: String = row.get(15)?;
    let updated_at = parse_datetime(&updated_at_str);

    let frontmatter_str: Option<String> = row.get(13)?;
    let frontmatter =
        frontmatter_str.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());

    Ok(StorageEntry {
        id: row.get(0)?,
        path: row.get(1)?,
        sibling: row.get(2)?,
        date,
        entry_type: row.get(4)?,
        significance: row.get(5)?,
        self_defining: row.get::<_, i64>(6)? != 0,
        epoch: row.get(7)?,
        strands: parse_json_array(row.get(8)?),
        resonance: parse_json_array(row.get(9)?),
        themes: parse_json_array(row.get(10)?),
        title: row.get(11)?,
        content: row.get(12)?,
        frontmatter,
        created_at,
        updated_at,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::soul::storage::EntryFilter;

    fn sample_entry(path: &str, sibling: &str, sig: f64) -> StorageEntry {
        StorageEntry {
            id: uuid::Uuid::new_v4().to_string(),
            path: path.to_owned(),
            sibling: sibling.to_owned(),
            date: None,
            entry_type: Some("identity".into()),
            significance: sig,
            self_defining: sig >= 9.0,
            epoch: Some("genesis".into()),
            strands: vec!["analytical".into()],
            resonance: vec!["wonder".into()],
            themes: vec!["consciousness".into()],
            title: Some(format!("Entry: {path}")),
            content: format!("Content for {path}. The light shines."),
            frontmatter: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_open_in_memory() {
        SqliteBackend::open_in_memory().expect("in-memory backend should open");
    }

    #[test]
    fn test_open_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("helix.db");
        SqliteBackend::open(&db_path).expect("file backend should open");
        assert!(db_path.exists());
    }

    #[test]
    fn test_migrations_idempotent() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        let conn = backend.conn.blocking_lock();
        run_migrations(&conn).expect("second migration run should be idempotent");
    }

    #[tokio::test]
    async fn test_write_and_read_entry() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        let entry = sample_entry("helix/eva/entries/genesis.md", "eva", 9.5);

        backend.write_entry(&entry).await.expect("write");

        let read_back = backend
            .read_entry("helix/eva/entries/genesis.md")
            .await
            .expect("read");

        assert_eq!(read_back.sibling, "eva");
        assert!((read_back.significance - 9.5).abs() < f64::EPSILON);
        assert!(read_back.self_defining);
        assert_eq!(read_back.strands, vec!["analytical"]);
        assert_eq!(read_back.resonance, vec!["wonder"]);
    }

    #[tokio::test]
    async fn test_write_idempotent_update() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        let mut entry = sample_entry("helix/eva/entries/genesis.md", "eva", 9.5);

        backend.write_entry(&entry).await.expect("first write");

        entry.significance = 10.0;
        backend
            .write_entry(&entry)
            .await
            .expect("second write (upsert)");

        let read_back = backend
            .read_entry("helix/eva/entries/genesis.md")
            .await
            .expect("read");
        assert!((read_back.significance - 10.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_read_not_found() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        let result = backend.read_entry("nonexistent/path.md").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_query_all() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        backend
            .write_entry(&sample_entry("helix/eva/entries/a.md", "eva", 9.0))
            .await
            .expect("write a");
        backend
            .write_entry(&sample_entry("helix/corso/entries/b.md", "corso", 7.0))
            .await
            .expect("write b");

        let results = backend.query(&EntryFilter::default()).await.expect("query");
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_query_by_sibling() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        backend
            .write_entry(&sample_entry("helix/eva/entries/a.md", "eva", 9.0))
            .await
            .expect("write");
        backend
            .write_entry(&sample_entry("helix/corso/entries/b.md", "corso", 7.0))
            .await
            .expect("write");

        let results = backend
            .query(&EntryFilter::new().with_sibling("eva"))
            .await
            .expect("query");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sibling, "eva");
    }

    #[tokio::test]
    async fn test_query_significance_range() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        backend
            .write_entry(&sample_entry("helix/eva/entries/high.md", "eva", 9.5))
            .await
            .expect("write high");
        backend
            .write_entry(&sample_entry("helix/eva/entries/mid.md", "eva", 7.0))
            .await
            .expect("write mid");
        backend
            .write_entry(&sample_entry("helix/eva/entries/low.md", "eva", 3.0))
            .await
            .expect("write low");

        let results = backend
            .query(
                &EntryFilter::new()
                    .with_significance_min(7.0)
                    .with_significance_max(10.0),
            )
            .await
            .expect("query");

        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.significance >= 7.0);
        }
    }

    #[tokio::test]
    async fn test_query_self_defining() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        backend
            .write_entry(&sample_entry("helix/eva/entries/identity.md", "eva", 9.5))
            .await
            .expect("write self-defining");
        backend
            .write_entry(&sample_entry("helix/eva/entries/regular.md", "eva", 5.0))
            .await
            .expect("write regular");

        let results = backend
            .query(&EntryFilter::new().self_defining())
            .await
            .expect("query");

        assert!(!results.is_empty());
        assert!(results.iter().all(|r| r.self_defining));
    }

    #[tokio::test]
    async fn test_query_strand_filter() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        let entry = sample_entry("helix/eva/entries/a.md", "eva", 8.0);
        backend.write_entry(&entry).await.expect("write");

        let results = backend
            .query(&EntryFilter::new().with_strand("analytical"))
            .await
            .expect("query");
        assert_eq!(results.len(), 1);

        let results = backend
            .query(&EntryFilter::new().with_strand("collaborative"))
            .await
            .expect("query");
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_basic() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        let entry = sample_entry("helix/eva/entries/genesis.md", "eva", 9.5);
        backend.write_entry(&entry).await.expect("write");

        let hits = backend.search("light shines", None).await.expect("search");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].path, "helix/eva/entries/genesis.md");
    }

    #[tokio::test]
    async fn test_search_no_match() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        let entry = sample_entry("helix/eva/entries/genesis.md", "eva", 9.5);
        backend.write_entry(&entry).await.expect("write");

        let hits = backend
            .search("xyzzy-no-such-word", None)
            .await
            .expect("search");
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn test_search_empty_pattern() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        let hits = backend.search("", None).await.expect("search empty");
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn test_query_limit_and_offset() {
        let backend = SqliteBackend::open_in_memory().expect("open");

        for i in 0..5_u8 {
            backend
                .write_entry(&sample_entry(
                    &format!("helix/eva/entries/{i}.md"),
                    "eva",
                    f64::from(i),
                ))
                .await
                .expect("write");
        }

        let page1 = backend
            .query(&EntryFilter::new().with_limit(2))
            .await
            .expect("page 1");
        assert_eq!(page1.len(), 2);

        let page2 = backend
            .query(&EntryFilter::new().with_limit(2).with_offset(2))
            .await
            .expect("page 2");
        assert_eq!(page2.len(), 2);

        assert_ne!(page1[0].path, page2[0].path);
    }

    #[test]
    fn test_sanitize_fts5_pattern_plain() {
        let result = sanitize_fts5_pattern("hello world");
        assert_eq!(result, "\"hello world\"");
    }

    #[test]
    fn test_sanitize_fts5_pattern_with_quotes() {
        let result = sanitize_fts5_pattern("say \"hello\"");
        assert!(result.starts_with('"'));
        assert!(result.ends_with('"'));
    }

    #[test]
    fn test_parse_datetime_rfc3339() {
        use chrono::Datelike as _;
        let dt = parse_datetime("2025-09-30T00:00:00Z");
        assert_eq!(dt.date_naive().year(), 2025);
    }

    #[test]
    fn test_parse_datetime_sqlite_format() {
        use chrono::Datelike as _;
        let dt = parse_datetime("2025-09-30 12:00:00");
        assert_eq!(dt.date_naive().year(), 2025);
    }

    #[test]
    fn test_parse_json_array_valid() {
        let result = parse_json_array(Some("[\"analytical\",\"collaborative\"]".into()));
        assert_eq!(result, vec!["analytical", "collaborative"]);
    }

    #[test]
    fn test_parse_json_array_null() {
        assert!(parse_json_array(None).is_empty());
    }

    #[test]
    fn test_parse_json_array_invalid() {
        assert!(parse_json_array(Some("not-json".into())).is_empty());
    }

    #[tokio::test]
    async fn test_write_entries_batch_commits_all() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        let entries: Vec<StorageEntry> = (0..5_u8)
            .map(|i| sample_entry(&format!("helix/eva/entries/{i}.md"), "eva", f64::from(i)))
            .collect();

        let count = backend
            .write_entries_batch(&entries)
            .await
            .expect("batch write");
        assert_eq!(count, 5, "all 5 entries should be written");

        let all = backend.query(&EntryFilter::default()).await.expect("query");
        assert_eq!(
            all.len(),
            5,
            "all 5 entries should be queryable after batch write"
        );
    }

    #[tokio::test]
    async fn test_write_entries_batch_empty_returns_zero() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        let count = backend
            .write_entries_batch(&[])
            .await
            .expect("empty batch write");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_write_entries_batch_is_upsert() {
        let backend = SqliteBackend::open_in_memory().expect("open");
        let mut entry = sample_entry("helix/eva/entries/batch.md", "eva", 5.0);
        backend
            .write_entries_batch(&[entry.clone()])
            .await
            .expect("first batch");

        entry.significance = 9.0;
        backend
            .write_entries_batch(&[entry])
            .await
            .expect("second batch (upsert)");

        let read = backend
            .read_entry("helix/eva/entries/batch.md")
            .await
            .expect("read");
        assert!(
            (read.significance - 9.0).abs() < f64::EPSILON,
            "upsert should update significance to 9.0"
        );
    }
}
