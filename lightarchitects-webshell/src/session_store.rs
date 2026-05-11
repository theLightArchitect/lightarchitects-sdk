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

    fn init_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                build_id TEXT PRIMARY KEY,
                cwd TEXT NOT NULL,
                agent_kind TEXT NOT NULL,
                backend TEXT,
                model TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                containerized INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at)",
            [],
        )?;
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
            "SELECT build_id, cwd, agent_kind, backend, model, created_at, updated_at, containerized
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
            })
        })?;
        rows.collect()
    }
}

fn db_path() -> Option<PathBuf> {
    lightarchitects::core::paths::root().map(|r| r.join("webshell").join("sessions.db"))
}
