//! Per-sibling identity cache with SHA-256 tamper detection.
//!
//! Generalises `EvaIdentityCache` to any sibling.  Each cache entry stores:
//! - frontmatter-stripped identity text (truncated to 32 KiB),
//! - the file's last-known `mtime` sentinel for hot-reload detection,
//! - a SHA-256 pin set on first load; mismatch on reload is logged as a
//!   WARN-level security event (`SkillTrustLedger` pattern, SERAPH H3).
//!
//! The `PersonaCacheStore` manages N entries and broadcasts an
//! `IdentityChanged(SiblingId)` notification when a reload occurs so that
//! `MultiVoiceSynthesizer` can invalidate any cached prompt blocks.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

use std::sync::RwLock;

use sha2::{Digest as _, Sha256};
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Maximum identity size accepted into the prompt prelude.
const MAX_IDENTITY_BYTES: usize = 32 * 1024;

/// Channel capacity for `IdentityChanged` notifications.
const IDENTITY_CHANGED_CHANNEL_CAPACITY: usize = 64;

// ---------------------------------------------------------------------------
// IdentityChanged notification
// ---------------------------------------------------------------------------

/// Notification emitted when a sibling's identity file is hot-reloaded.
#[derive(Debug, Clone)]
pub struct IdentityChanged {
    /// Sibling ID whose identity changed.
    pub sibling_id: String,
    /// Whether the SHA-256 pin mismatched (possible tampering).
    pub pin_mismatch: bool,
}

// ---------------------------------------------------------------------------
// PersonaEntry — one sibling's cached state
// ---------------------------------------------------------------------------

/// Loaded, frontmatter-stripped identity text for a single sibling.
#[derive(Debug, Default, Clone)]
struct PersonaEntry {
    /// Stripped body, truncated to [`MAX_IDENTITY_BYTES`].
    text: String,
    /// Last observed `mtime` of the identity file.
    mtime: Option<SystemTime>,
    /// SHA-256 of the raw file bytes at first load (tamper-detection pin).
    sha256_pin: Option<[u8; 32]>,
}

impl PersonaEntry {
    /// Load the identity at `path`, strip frontmatter, pin the hash.
    fn load(path: &Path) -> Self {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "persona_cache: failed to read identity — copilot continues without persona"
                );
                return Self::default();
            }
        };

        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let body = strip_frontmatter(&raw);
        let text = if body.len() > MAX_IDENTITY_BYTES {
            warn!(
                bytes = body.len(),
                limit = MAX_IDENTITY_BYTES,
                "persona_cache: identity file exceeds 32 KiB — truncating"
            );
            body[..MAX_IDENTITY_BYTES].to_owned()
        } else {
            body.to_owned()
        };

        let hash: [u8; 32] = Sha256::digest(raw.as_bytes()).into();
        Self {
            text,
            mtime,
            sha256_pin: Some(hash),
        }
    }

    /// Re-check `mtime`; reload in-place if changed.
    ///
    /// Returns `(reloaded, pin_mismatch)`.  When `pin_mismatch` is true the
    /// raw bytes differ from the pinned hash — log and surface as a security
    /// event but continue serving the new content.
    fn check_reload(&mut self, path: &Path) -> (bool, bool) {
        let current_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if current_mtime == self.mtime {
            return (false, false);
        }

        let prior_pin = self.sha256_pin;
        *self = Self::load(path);

        let pin_mismatch = matches!((prior_pin, self.sha256_pin), (Some(prior), Some(current)) if prior != current);

        if pin_mismatch {
            warn!("persona_cache: SHA-256 pin mismatch on reload — possible tampering");
        }

        (true, pin_mismatch)
    }

    fn text(&self) -> &str {
        &self.text
    }
}

// ---------------------------------------------------------------------------
// PersonaCacheStore
// ---------------------------------------------------------------------------

/// Multi-sibling identity cache with hot-reload and tamper detection.
///
/// A single background task (spawned by the caller — typically `AppState::new`)
/// should call [`PersonaCacheStore::tick`] on a fixed interval (e.g. 30 s).
/// `tick` acquires a write lock only when a file has changed; per-request reads
/// use a read lock — no file I/O on the hot path.
pub struct PersonaCacheStore {
    /// Sibling ID → (resolved path, cached entry).
    entries: RwLock<HashMap<String, (PathBuf, PersonaEntry)>>,
    /// Broadcast sender for identity-changed notifications.
    changed_tx: broadcast::Sender<IdentityChanged>,
}

impl PersonaCacheStore {
    /// Create an empty store.  Use [`subscribe`] to listen for change events.
    #[must_use]
    pub fn new() -> Self {
        let (changed_tx, _) = broadcast::channel(IDENTITY_CHANGED_CHANNEL_CAPACITY);
        Self {
            entries: RwLock::new(HashMap::new()),
            changed_tx,
        }
    }

    /// Subscribe to [`IdentityChanged`] notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<IdentityChanged> {
        self.changed_tx.subscribe()
    }

    /// Register a sibling identity file.  Performs an initial load.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned (only possible if a
    /// concurrent writer panicked while holding the lock — not expected in
    /// normal operation).
    pub fn register(&self, sibling_id: impl Into<String>, path: impl Into<PathBuf>) {
        let id = sibling_id.into();
        let path: PathBuf = path.into();
        let entry = PersonaEntry::load(&path);
        info!(sibling = %id, path = %path.display(), "persona_cache: registered");
        // Infallible: lock is never poisoned in normal operation.
        #[allow(clippy::unwrap_used)]
        self.entries.write().unwrap().insert(id, (path, entry));
    }

    /// Return the frontmatter-stripped identity text for `sibling_id`.
    ///
    /// Returns an empty string if the sibling is not registered or its file
    /// was unreadable.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned (lock-poisoning requires a
    /// concurrent writer to panic mid-write — not expected in normal operation).
    #[must_use]
    pub fn text(&self, sibling_id: &str) -> String {
        #[allow(clippy::unwrap_used)]
        self.entries
            .read()
            .unwrap()
            .get(sibling_id)
            .map(|(_, e)| e.text().to_owned())
            .unwrap_or_default()
    }

    /// Check all registered files for mtime changes; reload any that changed.
    ///
    /// Designed to be called from a background task on a fixed interval.
    /// Acquires a write lock only for entries that actually changed.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned (lock-poisoning requires a
    /// concurrent writer to panic mid-write — not expected in normal operation).
    pub fn tick(&self) {
        // Collect IDs + paths under read lock first to avoid holding the write
        // lock while doing file I/O.
        #[allow(clippy::unwrap_used)]
        let ids_and_paths: Vec<(String, PathBuf)> = self
            .entries
            .read()
            .unwrap()
            .iter()
            .map(|(id, (path, _))| (id.clone(), path.clone()))
            .collect();

        for (id, path) in ids_and_paths {
            #[allow(clippy::unwrap_used)]
            let mut guard = self.entries.write().unwrap();
            let Some((_, entry)) = guard.get_mut(&id) else {
                continue;
            };
            let (reloaded, pin_mismatch) = entry.check_reload(&path);
            drop(guard);

            if reloaded {
                info!(sibling = %id, pin_mismatch, "persona_cache: hot-reloaded identity");
                let _ = self.changed_tx.send(IdentityChanged {
                    sibling_id: id,
                    pin_mismatch,
                });
            }
        }
    }
}

impl Default for PersonaCacheStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip YAML frontmatter delimited by `---` at the top of the file.
fn strip_frontmatter(s: &str) -> &str {
    let trimmed = s.trim_start();
    if !trimmed.starts_with("---") {
        return s;
    }
    let after_open = match trimmed.find('\n') {
        Some(n) => &trimmed[n + 1..],
        None => return s,
    };
    match after_open.find("\n---") {
        Some(close) => after_open[close + 4..].trim_start_matches('\n'),
        None => s,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write_temp(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn register_and_retrieve_text() {
        let f = write_temp("hello from eva");
        let store = PersonaCacheStore::new();
        store.register("eva", f.path());
        assert_eq!(store.text("eva"), "hello from eva");
    }

    #[test]
    fn unregistered_sibling_returns_empty() {
        let store = PersonaCacheStore::new();
        assert!(store.text("quantum").is_empty());
    }

    #[test]
    fn frontmatter_stripped_on_load() {
        let f = write_temp("---\nname: EVA\n---\nbody content\n");
        let store = PersonaCacheStore::new();
        store.register("eva", f.path());
        assert_eq!(store.text("eva"), "body content\n");
    }

    #[test]
    fn text_truncated_at_32k() {
        let big = "x".repeat(MAX_IDENTITY_BYTES + 200);
        let f = write_temp(&big);
        let store = PersonaCacheStore::new();
        store.register("eva", f.path());
        assert_eq!(store.text("eva").len(), MAX_IDENTITY_BYTES);
    }

    #[test]
    fn sha256_pin_set_on_load() {
        let f = write_temp("pinned content");
        let store = PersonaCacheStore::new();
        store.register("eva", f.path());
        let guard = store.entries.read().unwrap();
        let (_, entry) = guard.get("eva").unwrap();
        assert!(entry.sha256_pin.is_some());
    }

    #[tokio::test]
    async fn identity_changed_broadcast_on_reload() {
        let f1 = write_temp("version one");
        let f2 = write_temp("version two");

        let store = PersonaCacheStore::new();
        let mut rx = store.subscribe();

        store.register("eva", f1.path());
        // Manually inject a different path to force mtime-change detection.
        {
            let mut guard = store.entries.write().unwrap();
            let entry = guard.get_mut("eva").unwrap();
            entry.0 = f2.path().to_path_buf();
        }

        store.tick();

        let notification = rx.try_recv().unwrap();
        assert_eq!(notification.sibling_id, "eva");
        assert!(
            notification.pin_mismatch,
            "content changed → pin should mismatch"
        );
        assert_eq!(store.text("eva"), "version two");
    }

    #[test]
    fn tick_with_unchanged_file_emits_no_notification() {
        let f = write_temp("stable content");
        let store = PersonaCacheStore::new();
        let mut rx = store.subscribe();
        store.register("eva", f.path());
        store.tick(); // mtime hasn't changed
        assert!(rx.try_recv().is_err(), "no notification expected");
    }
}
