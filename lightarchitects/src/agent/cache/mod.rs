//! Two-tier durable cache substrate for the Light Architects platform.
//!
//! `SoulCache<K, V>` is the single cache primitive that all durable caching in
//! the platform routes through. Operator directive: **caching = SOUL**
//! (Canon XXXIII).
//!
//! # Architecture
//!
//! ```text
//! get(key)
//!   ├─ L1 hit  →  return value  (sub-ms, moka::future::Cache)
//!   └─ L1 miss
//!        ├─ L2 hit  →  promote to L1 → return value  (<50ms, SOUL helix)
//!        └─ L2 miss →  return None
//!
//! put(key, value)
//!   ├─ L1 insert  (sync, immediate)
//!   └─ L2 write   (tokio::spawn fire-and-forget — never blocks caller)
//! ```
//!
//! # Snapshot invalidation
//!
//! `invalidate_snapshot(new_id)` evicts all L1 entries. L2 entries are keyed
//! by content hash — they remain valid across snapshots and will be promoted
//! back on read if the key hash matches. In v1, consumers should drop + rebuild
//! the `SoulCache` on snapshot change when strict staleness avoidance is needed
//! (NG-01 in the build contract).
//!
//! # Feature gate
//!
//! This module is compiled only with `--features soul-cache`.
//! `cargo check -p lightarchitects` (no features) must remain clean — the
//! `soul-cache` feature adds zero symbols to the default feature set.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use lightarchitects::agent::cache::{
//!     SoulCache, NullSoulCacheStore, HelixSnapshotId,
//! };
//!
//! async fn example() {
//!     let store = Arc::new(NullSoulCacheStore);
//!     let snap  = HelixSnapshotId::from_timestamp(chrono::Utc::now());
//!     let cache: SoulCache<String, String> =
//!         SoulCache::new("my-ns", store, snap, 1_000);
//!
//!     cache.put(&"hello".to_owned(), "world".to_owned()).await;
//!     assert_eq!(cache.get(&"hello".to_owned()).await, Some("world".to_owned()));
//! }
//! ```

pub mod key;
pub mod snapshot;
pub mod store;

#[cfg(test)]
mod tests;

pub use key::{CacheKey, sha256};
pub use snapshot::HelixSnapshotId;
pub use store::{HelixSoulCacheStore, NullSoulCacheStore, SoulCacheStore};

use std::{marker::PhantomData, sync::Arc};

use moka::future::Cache;
use serde::{Serialize, de::DeserializeOwned};

// ─── SoulCache ────────────────────────────────────────────────────────────────

/// Two-tier durable cache: L1 moka in-memory + L2 SOUL helix filesystem.
///
/// # Type parameters
///
/// - `K` — key type; must implement [`CacheKey`] (canonical bytes → SHA-256 hash).
/// - `V` — value type; must be serialisable + deserialisable (JSON round-trip
///   through L2) and `Clone + Send + Sync + 'static` for moka L1 storage.
///
/// # Send + Sync
///
/// `SoulCache<K, V>` is `Send + Sync` when `V: Send + Sync`. This is required
/// for concurrent reads from multiple async tasks (e.g. parallel agent workers).
///
/// # Cloning
///
/// `SoulCache` is cheaply clonable — both `moka::future::Cache` and the
/// `Arc<dyn SoulCacheStore>` are reference-counted.
#[derive(Clone)]
pub struct SoulCache<K, V>
where
    K: CacheKey,
    V: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    l1: Cache<[u8; 32], V>,
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
    /// Construct a new `SoulCache`.
    ///
    /// # Parameters
    ///
    /// - `namespace` — L2 subdirectory name; must be a valid path component
    ///   (no `/`, no `..`). Typically the consumer's codename (e.g. `"prompts"`).
    /// - `store` — L2 persistence backend. Use [`HelixSoulCacheStore::new()`] in
    ///   production or [`NullSoulCacheStore`] for tests / offline mode.
    /// - `snapshot` — current helix snapshot id; used as the invalidation anchor.
    /// - `l1_capacity` — maximum number of entries in the L1 moka cache.
    #[must_use]
    pub fn new(
        namespace: &'static str,
        store: Arc<dyn SoulCacheStore>,
        snapshot: HelixSnapshotId,
        l1_capacity: u64,
    ) -> Self {
        let l1 = Cache::builder().max_capacity(l1_capacity).build();
        Self {
            l1,
            store,
            snapshot,
            namespace,
            _phantom: PhantomData,
        }
    }

    /// Look up `key` in the cache.
    ///
    /// Returns `Some(value)` on L1 hit or L2 hit (L2 hit also promotes to L1).
    /// Returns `None` on cache miss.
    ///
    /// # L2 promotion
    ///
    /// When found in L2 but not L1, the entry is inserted into L1 before
    /// returning — subsequent calls for the same key will be L1 hits.
    pub async fn get(&self, key: &K) -> Option<V> {
        let hash = sha256(&key.canonical_bytes());

        // L1 hit — sub-ms path.
        if let Some(v) = self.l1.get(&hash).await {
            return Some(v);
        }

        // L2 hit — promote to L1.
        let bytes = self.store.read(self.namespace, &hash).await?;
        let v: V = serde_json::from_slice(&bytes).ok()?;
        self.l1.insert(hash, v.clone()).await;
        Some(v)
    }

    /// Insert `value` into the cache under `key`.
    ///
    /// L1 insert is immediate. L2 write is dispatched as a
    /// `tokio::spawn` fire-and-forget task — it never blocks the caller.
    /// Serialisation failures are silently dropped (L1 entry still lives).
    pub async fn put(&self, key: &K, value: V) {
        let hash = sha256(&key.canonical_bytes());
        self.l1.insert(hash, value.clone()).await;

        // Fire-and-forget L2 write — never blocks the hot path.
        let store = Arc::clone(&self.store);
        let ns = self.namespace;
        if let Ok(bytes) = serde_json::to_vec(&value) {
            tokio::spawn(async move {
                store.write(ns, &hash, bytes).await;
            });
        }
    }

    /// Invalidate all L1 entries and swap the snapshot anchor.
    ///
    /// Call when the helix snapshot changes (e.g. after a canon update).
    /// L2 entries are **not** deleted — they are keyed by content hash and
    /// will produce correct results on re-read if the consumer includes the
    /// snapshot in their key. For strict staleness avoidance, drop and rebuild
    /// the `SoulCache` (see NG-01 in the build contract).
    pub fn invalidate_snapshot(&mut self, new_snapshot: HelixSnapshotId) {
        // WHY: L1 must be cleared because the old snapshot's semantic meaning
        // has changed. L2 entries survive intentionally — they are content-
        // addressed, not snapshot-addressed.
        self.l1.invalidate_all();
        self.snapshot = new_snapshot;
    }

    /// The snapshot id this cache was last invalidated against.
    #[must_use]
    pub fn snapshot(&self) -> &HelixSnapshotId {
        &self.snapshot
    }

    /// The L2 namespace this cache writes to under the helix root.
    #[must_use]
    pub fn namespace(&self) -> &str {
        self.namespace
    }
}

// ─── Send + Sync bounds ───────────────────────────────────────────────────────

// SAFETY: moka::future::Cache<K, V> is Send + Sync when V: Send + Sync.
// Arc<dyn SoulCacheStore> is Send + Sync by trait bound.
// PhantomData<fn(K) -> V> is always Send + Sync.
// Therefore SoulCache<K, V> is Send + Sync.
//
// The static_assertions check in tests/mod.rs verifies this at compile time.
