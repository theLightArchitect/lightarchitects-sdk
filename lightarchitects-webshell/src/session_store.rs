//! Native session persistence — `SQLite` `sessions` table.
//!
//! Owned by the webshell so session continuity survives browser refreshes
//! and server restarts. Each build gets one row; updated on agent switches,
//! model changes, and graceful shutdown.

use std::path::PathBuf;

use rusqlite::{Connection, params};

/// A single row from the `sessions` table, returned by [`SessionStore::list`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionRow {
    /// Build UUID (primary key).
    pub build_id: String,
    /// Working directory for the build.
    pub cwd: String,
    /// Agent kind (`lightarchitects`, `codex`, etc.).
    pub agent_kind: String,
    /// Backend name, if recorded.
    pub backend: Option<String>,
    /// Model override, if recorded.
    pub model: Option<String>,
    /// Unix timestamp when the row was created.
    pub created_at: i64,
    /// Unix timestamp when the row was last updated.
    pub updated_at: i64,
    /// Whether the session runs in a container.
    pub containerized: bool,
    /// Northstar text captured at build creation; injected into supervisor evaluation
    /// prompts so each wave is scored against the operator's declared intent.
    pub northstar_text: Option<String>,
}

/// SQLite-backed session store.
pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    /// Open the canonical session database at `~/.lightarchitects/webshell/sessions.db`.
    ///
    /// Returns a no-op store if the path is unavailable or opening fails.
    ///
    /// # Errors
    ///
    /// Returns [`rusqlite::Error`] if the database cannot be opened or the
    /// schema cannot be initialised.
    pub fn open() -> Result<Self, rusqlite::Error> {
        let path = db_path().unwrap_or_else(|| {
            std::env::temp_dir().join(format!("la_sessions_{}.db", std::process::id()))
        });
        let conn = Connection::open(&path)?;
        Self::init_schema(&conn)?;
        tracing::info!(target: "session_store", path = %path.display(), "SQLite session store opened");
        Ok(Self { conn })
    }

    /// Create a no-op store for tests or when `SQLite` is unavailable.
    ///
    /// # Panics
    ///
    /// Panics only if in-memory `SQLite` fails to open (extremely unlikely).
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn noop() -> Self {
        let conn = Connection::open_in_memory().expect("in-memory SQLite always opens");
        let _ = Self::init_schema(&conn);
        Self { conn }
    }

    /// Initialises the sessions schema.
    ///
    /// Sets WAL journaling mode for crash-safe operation (P7 check 4): data committed
    /// before an abnormal process termination is preserved on restart. Idempotent —
    /// safe to call on an existing WAL-mode database.
    fn init_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                build_id TEXT PRIMARY KEY,
                cwd TEXT NOT NULL,
                agent_kind TEXT NOT NULL,
                backend TEXT,
                model TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                containerized INTEGER NOT NULL DEFAULT 0,
                northstar_text TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at)",
            [],
        )?;
        // Idempotent migration for databases created before the northstar_text column.
        // pragma_table_info is available in all SQLite versions used by rusqlite.
        let col_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'northstar_text'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .unwrap_or(0)
            != 0;
        if !col_exists {
            conn.execute("ALTER TABLE sessions ADD COLUMN northstar_text TEXT", [])?;
        }
        Ok(())
    }

    /// Insert or replace a session row.
    ///
    /// # Errors
    ///
    /// Returns [`rusqlite::Error`] on SQL execution failure.
    pub fn insert(
        &self,
        build_id: &str,
        cwd: &str,
        agent_kind: &str,
        backend: Option<&str>,
        model: Option<&str>,
        containerized: bool,
    ) -> Result<(), rusqlite::Error> {
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "INSERT OR REPLACE INTO sessions
             (build_id, cwd, agent_kind, backend, model, created_at, updated_at, containerized)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                build_id,
                cwd,
                agent_kind,
                backend,
                model,
                now,
                now,
                i32::from(containerized),
            ],
        )?;
        Ok(())
    }

    /// Update the `updated_at` timestamp and optional fields for a session.
    ///
    /// # Errors
    ///
    /// Returns [`rusqlite::Error`] on SQL execution failure.
    pub fn touch(
        &self,
        build_id: &str,
        backend: Option<&str>,
        model: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "UPDATE sessions SET updated_at = ?1, backend = COALESCE(?2, backend), model = COALESCE(?3, model)
             WHERE build_id = ?4",
            params![now, backend, model, build_id],
        )?;
        Ok(())
    }

    /// Persist the northstar text for a build session.
    ///
    /// Replaces any previously stored value. Called after the operator confirms
    /// the northstar during build creation; the supervisor evaluation loop reads
    /// this value from [`SessionRow::northstar_text`] to score each wave.
    ///
    /// # Errors
    ///
    /// Returns [`rusqlite::Error`] on SQL execution failure.
    pub fn set_northstar_text(
        &self,
        build_id: &str,
        northstar_text: &str,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE sessions SET northstar_text = ?1 WHERE build_id = ?2",
            params![northstar_text, build_id],
        )?;
        Ok(())
    }

    /// Remove a session row.
    ///
    /// # Errors
    ///
    /// Returns [`rusqlite::Error`] on SQL execution failure.
    pub fn remove(&self, build_id: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "DELETE FROM sessions WHERE build_id = ?1",
            params![build_id],
        )?;
        Ok(())
    }

    /// Count active sessions.
    ///
    /// # Errors
    ///
    /// Returns [`rusqlite::Error`] on SQL execution failure.
    pub fn count(&self) -> Result<i64, rusqlite::Error> {
        self.conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
    }

    /// List all persisted sessions ordered by most recently updated first.
    ///
    /// # Errors
    ///
    /// Returns [`rusqlite::Error`] on SQL execution failure.
    pub fn list(&self) -> Result<Vec<SessionRow>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT build_id, cwd, agent_kind, backend, model, created_at, updated_at, containerized, northstar_text
             FROM sessions ORDER BY updated_at DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SessionRow {
                build_id: row.get(0)?,
                cwd: row.get(1)?,
                agent_kind: row.get(2)?,
                backend: row.get(3)?,
                model: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                containerized: row.get::<_, i32>(7)? != 0,
                northstar_text: row.get(8)?,
            })
        })?;
        rows.collect()
    }
}

fn db_path() -> Option<PathBuf> {
    lightarchitects::core::paths::root().map(|r| r.join("webshell").join("sessions.db"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn store_with_row() -> SessionStore {
        let store = SessionStore::noop();
        store
            .insert("build-1", "/tmp/proj", "lightarchitects", None, None, false)
            .unwrap();
        store
    }

    #[test]
    fn northstar_text_starts_null_and_round_trips_via_set() {
        let store = store_with_row();

        let rows = store.list().unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].northstar_text.is_none(), "should be NULL on insert");

        store
            .set_northstar_text("build-1", "Ship E2E webshell without terminal fallback")
            .unwrap();

        let rows = store.list().unwrap();
        assert_eq!(
            rows[0].northstar_text.as_deref(),
            Some("Ship E2E webshell without terminal fallback")
        );
    }

    #[test]
    fn set_northstar_text_overwrites_previous_value() {
        let store = store_with_row();
        store.set_northstar_text("build-1", "first").unwrap();
        store.set_northstar_text("build-1", "second").unwrap();

        let rows = store.list().unwrap();
        assert_eq!(rows[0].northstar_text.as_deref(), Some("second"));
    }

    #[test]
    fn init_schema_migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        // First init creates the table including northstar_text.
        SessionStore::init_schema(&conn).unwrap();
        // Second init must not fail (ALTER TABLE is skipped for existing column).
        SessionStore::init_schema(&conn).unwrap();

        // Column must be queryable.
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'northstar_text'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn set_northstar_text_on_missing_build_is_noop() {
        let store = SessionStore::noop();
        // No row for "ghost" — UPDATE affects 0 rows, should not error.
        store
            .set_northstar_text("ghost-build", "irrelevant")
            .unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn wal_mode_is_active_after_init_schema() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let conn = Connection::open(tmp.path()).unwrap();
        SessionStore::init_schema(&conn).unwrap();
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            mode, "wal",
            "journal_mode must be WAL after init_schema (P7 check 4)"
        );
    }

    #[test]
    fn uncommitted_transaction_is_rolled_back_on_reopen() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        {
            let conn = Connection::open(tmp.path()).unwrap();
            SessionStore::init_schema(&conn).unwrap();
            conn.execute(
                "INSERT INTO sessions (build_id, cwd, agent_kind, created_at, updated_at, containerized)
                 VALUES ('committed', '/tmp', 'la', 1, 1, 0)",
                [],
            )
            .unwrap();
            // Begin a transaction, insert, then drop without committing — WAL rollback path.
            let tx = conn.unchecked_transaction().unwrap();
            tx.execute(
                "INSERT INTO sessions (build_id, cwd, agent_kind, created_at, updated_at, containerized)
                 VALUES ('uncommitted', '/tmp', 'la', 1, 1, 0)",
                [],
            )
            .unwrap();
            drop(tx); // rollback
        }
        // Reopen — committed row must survive; uncommitted row must not.
        let conn2 = Connection::open(tmp.path()).unwrap();
        let count: i64 = conn2
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 1,
            "only the committed row should survive after WAL rollback"
        );
        let id: String = conn2
            .query_row("SELECT build_id FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(id, "committed");
    }
}
