//! L2 cache store trait and concrete implementations.
//!
//! The [`SoulCacheStore`] trait is the plug-point between [`SoulCache`] and
//! the SOUL helix filesystem. Two implementations ship:
//!
//! - [`NullSoulCacheStore`] — fail-closed no-op. L1 still works; L2 writes are
//!   silently dropped and reads return `None`. Used in tests and when the helix
//!   path is unavailable.
//! - [`HelixSoulCacheStore`] — writes JSON entries to
//!   `~/lightarchitects/soul/helix/corso/cache/<ns>/<hex(hash)>.json`.
//!   Operator directive: all platform caching routes through SOUL helix
//!   (Canon XXXIII).
//!
//! [`SoulCache`]: super::SoulCache

use async_trait::async_trait;
use std::path::PathBuf;

// ─── Trait ───────────────────────────────────────────────────────────────────

/// Pluggable L2 cache persistence layer.
///
/// Implementors persist raw bytes keyed by `(namespace, key_hash)`. The hash
/// is a 32-byte SHA-256 digest produced by [`CacheKey::canonical_bytes`] →
/// [`sha256`].
///
/// # Fail-closed contract
///
/// Implementations MUST NOT propagate I/O errors to the caller: write failures
/// are silently dropped (L1 remains the authoritative tier); read failures
/// return `None` (cache miss). This keeps consumers on the happy path and
/// avoids cascading failures when SOUL helix is momentarily unavailable.
///
/// [`CacheKey::canonical_bytes`]: super::key::CacheKey::canonical_bytes
/// [`sha256`]: super::key::sha256
#[async_trait]
pub trait SoulCacheStore: Send + Sync + 'static {
    /// Read raw bytes for `(namespace, hash)`. Returns `None` on miss or error.
    async fn read(&self, namespace: &str, hash: &[u8; 32]) -> Option<Vec<u8>>;

    /// Write raw bytes for `(namespace, hash)`. Failures silently dropped.
    async fn write(&self, namespace: &str, hash: &[u8; 32], value: Vec<u8>);

    /// Remove the entry for `(namespace, hash)`. Failures silently dropped.
    async fn invalidate(&self, namespace: &str, hash: &[u8; 32]);
}

// ─── NullSoulCacheStore ───────────────────────────────────────────────────────

/// Fail-closed L2 fallback.
///
/// - Writes are silently dropped.
/// - Reads always return `None`.
/// - L1 cache continues to operate normally.
///
/// Use when SOUL helix is unavailable, in tests that only need L1 semantics,
/// or as a default until the helix path is configured.
pub struct NullSoulCacheStore;

#[async_trait]
impl SoulCacheStore for NullSoulCacheStore {
    async fn read(&self, _namespace: &str, _hash: &[u8; 32]) -> Option<Vec<u8>> {
        None
    }

    async fn write(&self, _namespace: &str, _hash: &[u8; 32], _value: Vec<u8>) {
        // WHY: intentional no-op — fail-closed L2 fallback per SoulCacheStore contract.
    }

    async fn invalidate(&self, _namespace: &str, _hash: &[u8; 32]) {
        // WHY: intentional no-op — fail-closed L2 fallback per SoulCacheStore contract.
    }
}

// ─── HelixSoulCacheStore ──────────────────────────────────────────────────────

/// SOUL helix filesystem L2 store.
///
/// Persists cache entries as JSON files at:
/// `<helix_root>/<namespace>/<hex(hash)>.json`
///
/// Default root: `~/lightarchitects/soul/helix/corso/cache/`
///
/// Operator directive: all durable platform caching routes through SOUL helix
/// (Canon XXXIII — replayability anchor).
///
/// # Failure behaviour
///
/// All I/O errors are silently dropped per the [`SoulCacheStore`] fail-closed
/// contract. The cache degrades to L1-only when the helix path is unavailable
/// (e.g. directory missing, permissions error, disk full).
pub struct HelixSoulCacheStore {
    /// Root directory — each namespace gets a subdirectory here.
    helix_root: PathBuf,
}

impl HelixSoulCacheStore {
    /// Construct with the default helix cache root:
    /// `$HOME/lightarchitects/soul/helix/corso/cache/`
    ///
    /// Falls back to `/tmp/soul-cache` when `$HOME` is unset.
    #[must_use]
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
        Self {
            helix_root: PathBuf::from(home).join("lightarchitects/soul/helix/corso/cache"),
        }
    }

    /// Construct with an explicit root — primarily for tests.
    #[must_use]
    pub fn with_root(helix_root: PathBuf) -> Self {
        Self { helix_root }
    }

    fn entry_path(&self, namespace: &str, hash: &[u8; 32]) -> PathBuf {
        self.helix_root
            .join(namespace)
            .join(format!("{}.json", hex::encode(hash)))
    }
}

impl Default for HelixSoulCacheStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SoulCacheStore for HelixSoulCacheStore {
    async fn read(&self, namespace: &str, hash: &[u8; 32]) -> Option<Vec<u8>> {
        let path = self.entry_path(namespace, hash);
        tokio::fs::read(&path).await.ok()
    }

    async fn write(&self, namespace: &str, hash: &[u8; 32], value: Vec<u8>) {
        let dir = self.helix_root.join(namespace);
        // Silently drop if directory creation fails (degraded mode).
        if tokio::fs::create_dir_all(&dir).await.is_err() {
            return;
        }
        let path = self.entry_path(namespace, hash);
        // WHY: fire-and-forget — failures are dropped per fail-closed contract.
        let _ = tokio::fs::write(&path, value).await;
    }

    async fn invalidate(&self, namespace: &str, hash: &[u8; 32]) {
        let path = self.entry_path(namespace, hash);
        // Silently drop whether the file existed or not.
        let _ = tokio::fs::remove_file(&path).await;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[tokio::test]
    async fn null_store_reads_none() {
        let store = NullSoulCacheStore;
        let hash = [0u8; 32];
        assert!(store.read("ns", &hash).await.is_none());
    }

    #[tokio::test]
    async fn null_store_write_is_noop() {
        let store = NullSoulCacheStore;
        let hash = [0u8; 32];
        // Should not panic or error.
        store.write("ns", &hash, b"data".to_vec()).await;
        assert!(store.read("ns", &hash).await.is_none());
    }

    #[tokio::test]
    async fn helix_store_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = HelixSoulCacheStore::with_root(tmp.path().to_owned());
        let hash = [1u8; 32];
        let payload = b"hello-helix".to_vec();

        store.write("test-ns", &hash, payload.clone()).await;

        let read_back = store.read("test-ns", &hash).await;
        assert_eq!(read_back, Some(payload));
    }

    #[tokio::test]
    async fn helix_store_invalidate_removes_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = HelixSoulCacheStore::with_root(tmp.path().to_owned());
        let hash = [2u8; 32];

        store.write("test-ns", &hash, b"x".to_vec()).await;
        assert!(store.read("test-ns", &hash).await.is_some());

        store.invalidate("test-ns", &hash).await;
        assert!(store.read("test-ns", &hash).await.is_none());
    }

    #[tokio::test]
    async fn helix_store_missing_entry_returns_none() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = HelixSoulCacheStore::with_root(tmp.path().to_owned());
        let hash = [3u8; 32];
        assert!(store.read("test-ns", &hash).await.is_none());
    }
}
