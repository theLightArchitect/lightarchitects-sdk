//! Two-tier SOUL helix cache — L1 `moka` in-process, L2 SOUL helix filesystem.
//!
//! Feature-gated behind `soul-cache`. All platform caching routes through SOUL
//! helix per the operator directive (Canon XXXIII — replayability anchor).
//!
//! | Tier | Implementation | Behaviour |
//! |------|---------------|-----------|
//! | L1 | `moka::future::Cache<[u8;32], V>` | Bounded async cache; LRU eviction. |
//! | L2 | [`SoulCacheStore`] (default: [`HelixSoulCacheStore`]) | SOUL helix filesystem. |
//!
//! L2 writes are fire-and-forget (`tokio::spawn`): callers never block on
//! helix I/O. L2 read-misses are silently treated as cache misses.
//!
//! # Feature isolation
//!
//! `cargo check -p lightarchitects` (without `--features soul-cache`) MUST
//! pass with zero references to this module. Enforced by G-COMPOSE-04.

pub mod key;
pub mod snapshot;
pub mod store;

pub use key::{CacheKey, sha256};
pub use snapshot::HelixSnapshotId;
pub use store::{HelixSoulCacheStore, NullSoulCacheStore, SoulCacheStore};

use std::marker::PhantomData;
use std::sync::Arc;

use moka::future::Cache;
use serde::{Serialize, de::DeserializeOwned};

// ─── SoulCache ────────────────────────────────────────────────────────────────

/// Two-tier cache: L1 [`moka`] in-process + L2 SOUL helix filesystem.
///
/// # Tiers
///
/// - **L1** — `moka::future::Cache<[u8;32], V>`, bounded by `l1_capacity`
///   entries. All reads populate L1 on L2 hit.
/// - **L2** — [`SoulCacheStore`] (default: [`HelixSoulCacheStore`]). Writes
///   are fire-and-forget via `tokio::spawn`; errors silently dropped per the
///   fail-closed contract.
///
/// # Snapshot invalidation
///
/// Call [`SoulCache::invalidate_snapshot`] when the SOUL helix state advances.
/// This replaces the internal L1 cache with a fresh empty instance so all
/// subsequent reads re-hydrate from L2 (or miss).
#[derive(Clone)]
pub struct SoulCache<K, V>
where
    K: CacheKey,
    V: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    l1: Cache<[u8; 32], V>,
    l1_capacity: u64,
    store: Arc<dyn SoulCacheStore>,
    snapshot: HelixSnapshotId,
    namespace: &'static str,
    _phantom: PhantomData<fn(K) -> V>,
}

impl<K, V> SoulCache<K, V>
where
    K: CacheKey,
    V: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    /// Construct a two-tier `SoulCache`.
    ///
    /// - `namespace` — logical namespace; occupies its own subdirectory in L2.
    /// - `store` — L2 persistence impl. Use [`HelixSoulCacheStore`] in
    ///   production; [`NullSoulCacheStore`] in tests needing only L1 semantics.
    /// - `snapshot` — current helix snapshot anchor.
    /// - `l1_capacity` — maximum entries retained by the L1 moka cache.
    #[must_use]
    pub fn new(
        namespace: &'static str,
        store: Arc<dyn SoulCacheStore>,
        snapshot: HelixSnapshotId,
        l1_capacity: u64,
    ) -> Self {
        Self {
            l1: Cache::new(l1_capacity),
            l1_capacity,
            store,
            snapshot,
            namespace,
            _phantom: PhantomData,
        }
    }

    /// Look up `key`.
    ///
    /// Returns `Some(value)` on L1 or L2 hit; `None` on full miss.
    /// On L2 hit the value is promoted into L1 before returning.
    pub async fn get(&self, key: &K) -> Option<V> {
        let hash = sha256(&key.canonical_bytes());
        if let Some(v) = self.l1.get(&hash).await {
            return Some(v);
        }
        // L2 hit — deserialise + promote to L1.
        let bytes = self.store.read(self.namespace, &hash).await?;
        let value: V = serde_json::from_slice(&bytes).ok()?;
        self.l1.insert(hash, value.clone()).await;
        Some(value)
    }

    /// Insert `value` under `key`.
    ///
    /// L1 insert blocks until moka acknowledges the entry. L2 write is
    /// fire-and-forget via `tokio::spawn`; failures are silently dropped.
    pub async fn put(&self, key: &K, value: V) {
        let hash = sha256(&key.canonical_bytes());
        self.l1.insert(hash, value.clone()).await;
        let store = Arc::clone(&self.store);
        let ns = self.namespace;
        if let Ok(bytes) = serde_json::to_vec(&value) {
            tokio::spawn(async move {
                store.write(ns, &hash, bytes).await;
            });
        }
    }

    /// Invalidate all L1 entries by replacing the internal cache.
    ///
    /// Called when the SOUL helix snapshot advances. All subsequent `get()`
    /// calls start cold and re-hydrate from L2 (or miss if not there yet).
    pub fn invalidate_snapshot(&mut self, snapshot: HelixSnapshotId) {
        self.snapshot = snapshot;
        // WHY new Cache: replace rather than invalidate_all to guarantee
        // immediate eviction without relying on moka's async task.
        self.l1 = Cache::new(self.l1_capacity);
    }

    /// The namespace this cache writes into.
    #[must_use]
    pub fn namespace(&self) -> &str {
        self.namespace
    }

    /// The current snapshot anchor.
    #[must_use]
    pub fn snapshot(&self) -> &HelixSnapshotId {
        &self.snapshot
    }
}

#[cfg(test)]
pub mod tests;
