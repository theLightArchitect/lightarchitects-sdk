//! `SoulPersistence` — Phase 10 tiered storage handle.
//!
//! Wraps the three persistence tiers the `SOUL` vault exposes:
//!
//! | Tier         | Backing                                                | Role                                    |
//! |--------------|--------------------------------------------------------|-----------------------------------------|
//! | `filesystem` | `~/lightarchitects/soul/helix/{sibling}/entries/*.md`  | always-present authoritative record     |
//! | `sqlite`     | `~/lightarchitects/soul/helix.db` (`FTS5` + BM25)        | fast search; shared with `SOUL` `MCP`       |
//! | `neo4j`      | `bolt://...` (opt-in via `WEBSHELL_NEO4J_URI`)         | graph retrieval + hybrid ranking        |
//!
//! **Parity contract**: `SQLite` is the same `helix.db` file the `SOUL` `MCP` plugin
//! writes through. An entry ingested from Claude Code (e.g. via
//! `soulTools action:ingest`) is immediately visible to the webshell without a
//! filesystem rescan. The filesystem tier is read-through; `SQLite` + `Neo4j` are
//! read/write tiers with `SQLite` as the primary.
//!
//! # Why a wrapper instead of passing `SqliteBackend` directly?
//!
//! The webshell's hot path has to degrade gracefully when any tier is absent.
//! A single `SoulPersistence` value encodes "try sqlite first, then fall back
//! to filesystem" uniformly — callers don't repeat that branching across every
//! handler.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lightarchitects::helix::HelixStore;
use lightarchitects::soul::sqlite::SqliteBackend;
use lightarchitects::soul::storage::{
    EntryFilter, StorageBackend as _, StorageEntry, StorageError,
};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Env var that opts in to `Neo4j` dual-write + graph retrieval.
///
/// When set, the webshell tries to open a bolt connection on startup.
/// Missing var = filesystem+`SQLite` tier only. Connection failure = logged
/// WARN + filesystem+`SQLite` only; never blocks startup.
const NEO4J_URI_ENV: &str = "WEBSHELL_NEO4J_URI";

/// Env var that disables `SQLite` dual-writes when set to any value.
///
/// Set `SOUL_DISABLE_SQLITE_WRITES=1` to skip [`SoulPersistence::write_entry`]
/// calls without recompiling. Used to verify `SOUL` `MCP` read parity before
/// permanently dropping the webshell write path (Phase 20b.3).
///
/// The `SOUL` `MCP` plugin's own read path from `helix.db` is unaffected.
pub const DISABLE_SQLITE_WRITES_ENV: &str = "SOUL_DISABLE_SQLITE_WRITES";
const NEO4J_USER_ENV: &str = "WEBSHELL_NEO4J_USER";
const NEO4J_PASS_ENV: &str = "WEBSHELL_NEO4J_PASS";

/// Soft cap to prevent walking an unbounded helix in one call.
const DEFAULT_LIMIT_CAP: usize = 500;

/// Tiered `SOUL` persistence handle. Cheap to clone (inner state is `Arc`-ed).
pub struct SoulPersistence {
    /// Absolute path to the helix filesystem root
    /// (`~/lightarchitects/soul/helix`).
    helix_root: PathBuf,
    /// Open `SQLite` backend — `None` when `helix.db` is missing or unreadable.
    sqlite: Option<Arc<SqliteBackend>>,
    /// Open `Neo4j` graph store — `None` until the background connect task
    /// succeeds. Written once via `try_attach_neo4j`; readers `clone()` the
    /// inner `Arc<HelixStore>` so the `RwLock` is never held across `.await`.
    neo4j: Arc<RwLock<Option<Arc<HelixStore>>>>,
}

impl SoulPersistence {
    /// Try to open every available tier. Never panics — missing tiers are
    /// silently skipped so the webshell always boots.
    ///
    /// Reads the helix root from the SDK's canonical
    /// [`lightarchitects::core::paths::helix_root_or_fallback`] helper so
    /// webshell + `SOUL` binary + SDK consumers always agree on the path.
    #[must_use]
    pub fn open() -> Self {
        let helix_root = lightarchitects::core::paths::helix_root_or_fallback();
        let sqlite_path = helix_root.parent().map_or_else(
            || helix_root.join("helix.db"),
            |vault_root| vault_root.join("helix.db"),
        );
        let sqlite = if sqlite_path.exists() {
            match SqliteBackend::open(&sqlite_path) {
                Ok(backend) => {
                    info!(
                        target: "soul",
                        path = %sqlite_path.display(),
                        "SOUL SQLite backend opened"
                    );
                    Some(Arc::new(backend))
                }
                Err(e) => {
                    warn!(
                        target: "soul",
                        error = %e,
                        path = %sqlite_path.display(),
                        "SOUL SQLite open failed — filesystem fallback only"
                    );
                    None
                }
            }
        } else {
            info!(
                target: "soul",
                path = %sqlite_path.display(),
                "SOUL helix.db absent — filesystem-only mode"
            );
            None
        };
        Self {
            helix_root,
            sqlite,
            neo4j: Arc::new(RwLock::new(None)),
        }
    }

    /// Construct a test fixture pointing at an isolated helix root + no DB.
    ///
    /// For integration tests that don't want to touch the user's real vault.
    #[must_use]
    pub fn for_test(helix_root: PathBuf) -> Self {
        Self {
            helix_root,
            sqlite: None,
            neo4j: Arc::new(RwLock::new(None)),
        }
    }

    /// Phase 11.3 — try to attach a `Neo4j` `HelixStore` using env credentials.
    ///
    /// Reads `WEBSHELL_NEO4J_URI` / `_USER` / `_PASS`. On success stores the
    /// store behind the internal `RwLock`. Never errors — missing env,
    /// refused connection, or migration failure all just leave the tier
    /// disabled. Designed for fire-and-forget `tokio::spawn` from startup.
    pub async fn try_attach_neo4j(self: Arc<Self>) {
        let Ok(uri) = std::env::var(NEO4J_URI_ENV) else {
            info!(target: "soul", "Neo4j tier disabled — {NEO4J_URI_ENV} not set");
            return;
        };
        let user = std::env::var(NEO4J_USER_ENV).unwrap_or_else(|_| "neo4j".to_owned());
        let Ok(pass) = std::env::var(NEO4J_PASS_ENV) else {
            warn!(
                target: "soul",
                "{NEO4J_URI_ENV} set but {NEO4J_PASS_ENV} missing — Neo4j tier disabled"
            );
            return;
        };

        match HelixStore::connect(&uri, &user, &pass).await {
            Ok(store) => {
                info!(target: "soul", uri = %uri, "Neo4j tier attached");
                let mut guard = self.neo4j.write().await;
                *guard = Some(Arc::new(store));
            }
            Err(e) => warn!(target: "soul", error = %e, uri = %uri, "Neo4j connect failed"),
        }
    }

    /// Whether the `Neo4j` tier is currently live.
    pub async fn has_neo4j(&self) -> bool {
        self.neo4j.read().await.is_some()
    }

    /// Clone-out a handle to the live `Neo4j` store, if any.
    pub async fn neo4j_arc(&self) -> Option<Arc<HelixStore>> {
        self.neo4j.read().await.clone()
    }

    /// Absolute helix root path.
    #[must_use]
    pub fn helix_root(&self) -> &Path {
        &self.helix_root
    }

    /// Whether the `SQLite` backend is live.
    #[must_use]
    pub fn has_sqlite(&self) -> bool {
        self.sqlite.is_some()
    }

    /// Per-tier health snapshot — consumed by `/api/soul/health`.
    ///
    /// Async because the `Neo4j` tier status is guarded by an `RwLock`; the lock
    /// is held only for the read + clone, never across `.await`.
    pub async fn tier_status(&self) -> TierStatus {
        TierStatus {
            filesystem: self.helix_root.is_dir(),
            sqlite: self.sqlite.is_some(),
            neo4j: self.neo4j.read().await.is_some(),
        }
    }

    /// Query entries via the `SQLite` backend.
    ///
    /// Returns `None` when `SQLite` is unavailable — callers should fall back
    /// to a filesystem walk in that case.
    #[allow(clippy::missing_errors_doc)]
    pub async fn query_sqlite(
        &self,
        filter: &EntryFilter,
    ) -> Option<Result<Vec<StorageEntry>, StorageError>> {
        let backend = self.sqlite.as_ref()?;
        Some(backend.query(filter).await)
    }

    /// `FTS5` BM25-ranked search via `SQLite`. Returns entries ranked by relevance,
    /// deduplicated by path. `None` when `SQLite` is unavailable.
    ///
    /// The caller is responsible for supplying a safe `FTS5` expression — simple
    /// alphanumeric tokens work out of the box; boolean/proximity syntax is
    /// also accepted per [`StorageBackend::search_bm25`].
    #[allow(clippy::missing_errors_doc)]
    pub async fn search_sqlite(
        &self,
        fts5_expr: &str,
        limit: usize,
    ) -> Option<Result<Vec<StorageEntry>, StorageError>> {
        let backend = self.sqlite.as_ref()?;
        let limit = limit.min(DEFAULT_LIMIT_CAP);
        Some(backend.search_bm25(fts5_expr, Some(limit)).await)
    }

    /// Read one entry by vault-relative path from `SQLite`.
    ///
    /// The `SOUL` `MCP` plugin stores paths as `helix/{sibling}/entries/{file}.md`
    /// (with the `helix/` prefix). We accept either form and normalise.
    #[allow(clippy::missing_errors_doc)]
    pub async fn read_entry_sqlite(&self, rel_path: &str) -> Option<StorageEntry> {
        let backend = self.sqlite.as_ref()?;
        for candidate in [rel_path.to_owned(), format!("helix/{rel_path}")] {
            match backend.read_entry(&candidate).await {
                Ok(entry) => return Some(entry),
                Err(StorageError::NotFound(_)) => {}
                Err(e) => {
                    warn!(
                        target: "soul",
                        error = %e,
                        path = %candidate,
                        "SQLite read_entry failure"
                    );
                    return None;
                }
            }
        }
        None
    }

    /// Dual-write: insert or update a promoted entry into `SQLite`.
    ///
    /// Best-effort — on error, the filesystem write still stands. Returns
    /// `Ok(true)` when written, `Ok(false)` when `SQLite` isn't available or
    /// when [`DISABLE_SQLITE_WRITES_ENV`] is set (Phase 20b.3 gate).
    #[allow(clippy::missing_errors_doc)]
    pub async fn write_entry(&self, entry: &StorageEntry) -> Result<bool, StorageError> {
        if std::env::var(DISABLE_SQLITE_WRITES_ENV).is_ok() {
            return Ok(false);
        }
        let Some(backend) = self.sqlite.as_ref() else {
            return Ok(false);
        };
        backend.write_entry(entry).await?;
        Ok(true)
    }

    /// Whether `SQLite` writes are currently disabled via env var.
    #[must_use]
    pub fn sqlite_writes_disabled() -> bool {
        std::env::var(DISABLE_SQLITE_WRITES_ENV).is_ok()
    }

    /// Phase 11.1 — walk the filesystem helix and upsert every entry into
    /// `SQLite`. No-op when `SQLite` isn't available. Returns a per-sibling
    /// [`crate::memory::backfill::BackfillReport`] on success.
    pub async fn reindex(&self) -> Option<crate::memory::backfill::BackfillReport> {
        let backend = self.sqlite.as_ref()?;
        Some(crate::memory::backfill::run(&self.helix_root, backend).await)
    }

    /// Borrow the `SQLite` backend (internal use only).
    pub(crate) fn sqlite_arc(&self) -> Option<Arc<SqliteBackend>> {
        self.sqlite.clone()
    }
}

/// Snapshot of which persistence tiers are available. Serialised on
/// `/api/soul/health`.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct TierStatus {
    /// Helix root directory exists and is readable.
    pub filesystem: bool,
    /// `SQLite` backend opened successfully.
    pub sqlite: bool,
    /// `Neo4j` bolt connection was established at startup. Phase 10.4.
    pub neo4j: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn for_test_has_no_sqlite_and_no_neo4j() {
        let p = SoulPersistence::for_test(PathBuf::from("/tmp/nonexistent"));
        let status = p.tier_status().await;
        assert!(!status.sqlite);
        assert!(!status.neo4j);
        // filesystem flag reflects disk reality, not test fixture.
    }

    #[tokio::test]
    async fn query_sqlite_returns_none_when_unavailable() {
        let p = SoulPersistence::for_test(PathBuf::from("/tmp/x"));
        let r = p.query_sqlite(&EntryFilter::default()).await;
        assert!(r.is_none(), "expected None (no sqlite)");
    }

    #[tokio::test]
    async fn write_entry_returns_false_when_unavailable() {
        let p = SoulPersistence::for_test(PathBuf::from("/tmp/x"));
        let r = p.write_entry(&StorageEntry::default()).await;
        assert!(matches!(r, Ok(false)));
    }
}
