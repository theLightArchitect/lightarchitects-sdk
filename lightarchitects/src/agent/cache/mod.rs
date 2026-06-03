//! Stub cache primitives for the `soul-cache` feature gate.
//!
//! This module is compiled only when `--features soul-cache` is active.
//! It provides the same public API surface as the `soul-cache-substrate`
//! build: `SoulCache<K, V>`, `CacheKey`, `SoulCacheStore`,
//! `NullSoulCacheStore`, `HelixSoulCacheStore`, `HelixSnapshotId`.
//!
//! When `soul-cache-substrate` lands and fully merges, this stub is replaced
//! by the real two-tier (moka L1 + SOUL helix L2) implementation without any
//! change to the public API. The stub uses a `HashMap`-backed L1 (no moka
//! dep) and a simple filesystem L2 (`HelixSoulCacheStore`) so integration
//! tests can exercise the write-survive-read path without moka.
//!
//! # Feature isolation
//!
//! `cargo check -p lightarchitects` (without `--features soul-cache`) MUST
//! pass with zero references to this module. Enforced by G-COMPOSE-04.

pub mod key;

pub use key::CacheKey;

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::Mutex;

// ── HelixSnapshotId ───────────────────────────────────────────────────────────

/// Snapshot anchor for cache invalidation.
///
/// Mirrors `soul-cache-substrate::agent::cache::HelixSnapshotId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HelixSnapshotId(u64);

impl HelixSnapshotId {
    /// Construct from a `chrono::DateTime<chrono::Utc>`.
    ///
    /// Uses milliseconds since epoch as the snapshot discriminant.
    #[must_use]
    pub fn from_timestamp(ts: chrono::DateTime<chrono::Utc>) -> Self {
        // WHY saturating: negative millis (pre-epoch) map to 0, not panic.
        let ms = ts.timestamp_millis();
        Self(u64::try_from(ms).unwrap_or(0))
    }

    /// Construct from raw millis.
    #[must_use]
    pub fn from_timestamp_millis(millis: i64) -> Self {
        Self(u64::try_from(millis).unwrap_or(0))
    }
}

// ── SoulCacheStore ─────────────────────────────────────────────────────────────

/// L2 persistence backend trait for `SoulCache`.
///
/// Mirrors `soul-cache-substrate::agent::cache::SoulCacheStore`.
#[async_trait::async_trait]
pub trait SoulCacheStore: Send + Sync + 'static {
    /// Read serialised bytes for `(namespace, hash)`. Returns `None` on miss.
    async fn read(&self, namespace: &str, hash: &[u8; 32]) -> Option<Vec<u8>>;
    /// Write serialised bytes under `(namespace, hash)`.
    async fn write(&self, namespace: &str, hash: &[u8; 32], bytes: Vec<u8>);
}

// ── NullSoulCacheStore ────────────────────────────────────────────────────────

/// No-op L2 store — always misses; writes discarded.
///
/// Use in tests or when SOUL helix is unavailable.
pub struct NullSoulCacheStore;

#[async_trait::async_trait]
impl SoulCacheStore for NullSoulCacheStore {
    async fn read(&self, _: &str, _: &[u8; 32]) -> Option<Vec<u8>> {
        None
    }

    async fn write(&self, _: &str, _: &[u8; 32], _: Vec<u8>) {}
}

// ── HelixSoulCacheStore ───────────────────────────────────────────────────────

/// Filesystem-backed L2 store — writes JSON to `root/{namespace}/{hex(hash)}`.
///
/// This is the stub version; the real implementation in `soul-cache-substrate`
/// uses the SOUL helix graph. The stub allows integration tests to verify the
/// write-survive-read path without the full SOUL stack.
pub struct HelixSoulCacheStore {
    root: PathBuf,
}

impl HelixSoulCacheStore {
    /// Construct with a filesystem root for L2 storage.
    #[must_use]
    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait::async_trait]
impl SoulCacheStore for HelixSoulCacheStore {
    async fn read(&self, namespace: &str, hash: &[u8; 32]) -> Option<Vec<u8>> {
        let path = self.root.join(namespace).join(hex_encode(hash));
        tokio::fs::read(path).await.ok()
    }

    async fn write(&self, namespace: &str, hash: &[u8; 32], bytes: Vec<u8>) {
        let dir = self.root.join(namespace);
        // Best-effort: ignore errors (L2 write is fire-and-forget).
        let _ = tokio::fs::create_dir_all(&dir).await;
        let _ = tokio::fs::write(dir.join(hex_encode(hash)), bytes).await;
    }
}

fn hex_encode(hash: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    hash.iter().fold(String::with_capacity(64), |mut s, b| {
        // WHY write! not format!: avoids intermediate allocation per byte.
        let _ = write!(s, "{b:02x}");
        s
    })
}

// ── SoulCache ─────────────────────────────────────────────────────────────────

/// Two-tier cache: L1 `HashMap` in-memory + L2 `SoulCacheStore` backend.
///
/// Stub implementation — `moka` not required. API-compatible with the real
/// `soul-cache-substrate` `SoulCache` so consumers compile without changes.
#[derive(Clone)]
pub struct SoulCache<K, V>
where
    K: CacheKey,
    V: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    l1: Arc<Mutex<HashMap<[u8; 32], Vec<u8>>>>,
    store: Arc<dyn SoulCacheStore>,
    namespace: &'static str,
    _phantom: std::marker::PhantomData<fn(K) -> V>,
}

impl<K, V> SoulCache<K, V>
where
    K: CacheKey,
    V: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    /// Construct a new `SoulCache`.
    ///
    /// - `namespace` — logical namespace / L2 subdirectory.
    /// - `store` — L2 backend; use `NullSoulCacheStore` for tests.
    /// - `_snapshot` — snapshot anchor (stub ignores invalidation).
    /// - `_l1_capacity` — stub is unbounded; parameter kept for API compat.
    #[must_use]
    pub fn new(
        namespace: &'static str,
        store: Arc<dyn SoulCacheStore>,
        _snapshot: HelixSnapshotId,
        _l1_capacity: u64,
    ) -> Self {
        Self {
            l1: Arc::new(Mutex::new(HashMap::new())),
            store,
            namespace,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Look up `key`. Returns `Some(value)` on L1 or L2 hit.
    pub async fn get(&self, key: &K) -> Option<V> {
        let hash = sha256_key(key);

        // L1 hit.
        {
            let guard = self.l1.lock().await;
            if let Some(bytes) = guard.get(&hash) {
                return serde_json::from_slice(bytes).ok();
            }
        }

        // L2 hit — promote to L1.
        let bytes = self.store.read(self.namespace, &hash).await?;
        let v: V = serde_json::from_slice(&bytes).ok()?;
        {
            let mut guard = self.l1.lock().await;
            guard.insert(hash, bytes);
        }
        Some(v)
    }

    /// Insert `value` under `key` into L1 and fire-and-forget to L2.
    pub async fn put(&self, key: &K, value: V) {
        let hash = sha256_key(key);
        if let Ok(bytes) = serde_json::to_vec(&value) {
            {
                let mut guard = self.l1.lock().await;
                guard.insert(hash, bytes.clone());
            }
            let store = Arc::clone(&self.store);
            let ns = self.namespace;
            tokio::spawn(async move {
                store.write(ns, &hash, bytes).await;
            });
        }
    }

    /// The namespace this cache writes to.
    #[must_use]
    pub fn namespace(&self) -> &str {
        self.namespace
    }
}

fn sha256_key<K: CacheKey>(key: &K) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(key.canonical_bytes());
    h.finalize().into()
}
