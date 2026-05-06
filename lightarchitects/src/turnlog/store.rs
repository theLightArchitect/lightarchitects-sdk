//! Filesystem layout and path resolution.
//!
//! ```text
//! {root}/
//! ├── genesis/{session_id}.json
//! ├── active/{session_id}.ndjson
//! ├── ended/{YYYY-MM-DD}/{session_id}.ndjson
//! ├── rollups/{YYYY-MM}/{session_id}.rollup.json
//! └── promoted/{session_id}.{seq}.ref
//! ```
//!
//! The default root is `~/lightarchitects/lightarchitects_cli/turnlog/`, but any path is acceptable —
//! tests use a tempdir; production wires `~/lightarchitects/lightarchitects_cli/turnlog/`.

use std::path::{Path, PathBuf};

use crate::turnlog::error::{Result, TurnLogError};

/// On-disk layout for a turnlog store.
///
/// Cheap to clone; all methods return fresh [`PathBuf`] values rather than
/// borrowing, to keep the public API free of lifetime parameters.
#[derive(Debug, Clone)]
pub struct StoreLayout {
    root: PathBuf,
}

impl StoreLayout {
    /// Create a new layout rooted at `root`.
    ///
    /// The directory is not created eagerly — [`Self::ensure_dirs`] is called
    /// lazily by the writer on session open.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Default layout at `~/lightarchitects/lightarchitects_cli/turnlog/`.
    ///
    /// Returns `None` if the home directory cannot be resolved.
    #[must_use]
    pub fn default_for_user() -> Option<Self> {
        Some(Self::new(
            crate::core::paths::lightarchitects_cli()?.join("turnlog"),
        ))
    }

    /// Root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path to the genesis block for a session.
    #[must_use]
    pub fn genesis_path(&self, session_id: &str) -> PathBuf {
        self.root.join("genesis").join(format!("{session_id}.json"))
    }

    /// Path to the active (currently-writing) log file for a session.
    #[must_use]
    pub fn active_path(&self, session_id: &str) -> PathBuf {
        self.root
            .join("active")
            .join(format!("{session_id}.ndjson"))
    }

    /// Path the log is moved to on `SessionEnded`. `date` is `YYYY-MM-DD`.
    #[must_use]
    pub fn ended_path(&self, session_id: &str, date: &str) -> PathBuf {
        self.root
            .join("ended")
            .join(date)
            .join(format!("{session_id}.ndjson"))
    }

    /// Path to a rollup entry produced by retention compaction. `month` is `YYYY-MM`.
    #[must_use]
    pub fn rollup_path(&self, session_id: &str, month: &str) -> PathBuf {
        self.root
            .join("rollups")
            .join(month)
            .join(format!("{session_id}.rollup.json"))
    }

    /// Marker file path recording that `seq` was promoted to Tier-2 helix.
    #[must_use]
    pub fn promoted_marker_path(&self, session_id: &str, seq: u64) -> PathBuf {
        self.root
            .join("promoted")
            .join(format!("{session_id}.{seq}.ref"))
    }

    /// Path to the directory containing active session files.
    #[must_use]
    pub fn active_dir(&self) -> PathBuf {
        self.root.join("active")
    }

    /// Path to the directory containing a given date's ended sessions.
    #[must_use]
    pub fn ended_dir(&self, date: &str) -> PathBuf {
        self.root.join("ended").join(date)
    }

    /// Create the full directory scaffold eagerly.
    ///
    /// Called once by the writer on session open. Idempotent.
    ///
    /// # Errors
    /// Wraps any `std::io::Error` from `create_dir_all` into [`TurnLogError::Io`].
    pub async fn ensure_dirs(&self) -> Result<()> {
        for sub in ["genesis", "active", "ended", "rollups", "promoted"] {
            let p = self.root.join(sub);
            tokio::fs::create_dir_all(&p)
                .await
                .map_err(|e| TurnLogError::io(p, e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_under_root() {
        let layout = StoreLayout::new(PathBuf::from("/tmp/tl"));
        assert!(layout.genesis_path("s").starts_with("/tmp/tl"));
        assert!(layout.active_path("s").starts_with("/tmp/tl"));
        assert!(layout.ended_path("s", "2026-04-12").starts_with("/tmp/tl"));
        assert!(layout.rollup_path("s", "2026-04").starts_with("/tmp/tl"));
        assert!(layout.promoted_marker_path("s", 42).starts_with("/tmp/tl"));
    }

    #[test]
    fn genesis_file_is_json() {
        let layout = StoreLayout::new(PathBuf::from("/x"));
        let p = layout.genesis_path("abc");
        assert!(p.to_string_lossy().ends_with(".json"));
    }

    #[test]
    fn active_file_is_ndjson() {
        let layout = StoreLayout::new(PathBuf::from("/x"));
        let p = layout.active_path("abc");
        assert!(p.to_string_lossy().ends_with(".ndjson"));
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn ensure_dirs_creates_all_subdirectories() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = StoreLayout::new(tmp.path().to_path_buf());
        layout.ensure_dirs().await.unwrap();
        for sub in ["genesis", "active", "ended", "rollups", "promoted"] {
            assert!(tmp.path().join(sub).is_dir(), "missing {sub}");
        }
    }
}
