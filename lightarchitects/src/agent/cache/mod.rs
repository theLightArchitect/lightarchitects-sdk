//! Stub cache primitives for the `soul-cache` feature gate.
//!
//! This module is compiled only when `--features soul-cache` is active.
//! It provides `SoulCache<K, V>`, `CacheKey`, and `NullSoulCacheStore`
//! — the same surface exposed by the `soul-cache-substrate` build (separate
//! LASDLC build, not yet merged at the time this combinator surface ships).
//!
//! When `soul-cache-substrate` lands and merges, this stub is replaced by
//! the real two-tier (moka L1 + SOUL helix L2) implementation without any
//! change to the combinator API surface.
//!
//! # Feature isolation
//!
//! `cargo check -p lightarchitects` (without `--features soul-cache`) MUST
//! pass with zero references to this module. Enforced by G-COMPOSE-04.

pub mod key;

pub use key::CacheKey;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use serde::{Serialize, de::DeserializeOwned};

// ─── Null store ───────────────────────────────────────────────────────────────

/// No-op L2 cache store — always misses, discards writes.
///
/// Used in tests and when the real SOUL helix L2 store is unavailable.
/// The [`SoulCache`] remains functional as a pure L1 (in-process) cache.
#[non_exhaustive]
pub struct NullSoulCacheStore;

// ─── SoulCache ────────────────────────────────────────────────────────────────

/// Stub two-tier cache: L1 `HashMap` in-memory only.
///
/// Mirrors the API of the full `SoulCache` from `soul-cache-substrate` so
/// the combinator surface compiles and tests pass without the real SOUL L2
/// store. The stub stores serialised JSON bytes in a `HashMap<[u8; 32], Vec<u8>>`.
///
/// Replace with the real implementation when `soul-cache-substrate` merges.
#[derive(Clone)]
pub struct SoulCache<K, V>
where
    K: CacheKey,
    V: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    l1: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    namespace: &'static str,
    _phantom: std::marker::PhantomData<fn(K) -> V>,
}

impl<K, V> SoulCache<K, V>
where
    K: CacheKey + Serialize,
    V: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    /// Construct a new stub `SoulCache`.
    ///
    /// Parameters mirror the real `SoulCache::new()` so consumer code
    /// compiles unchanged when the real substrate lands.
    ///
    /// - `namespace` — logical namespace; stored on the struct.
    /// - `_store` — ignored in the stub (no L2).
    /// - `_snapshot` — ignored in the stub.
    /// - `_l1_capacity` — ignored; `HashMap` is unbounded in the stub.
    #[must_use]
    pub fn new(
        namespace: &'static str,
        _store: Arc<NullSoulCacheStore>,
        _snapshot: HelixSnapshotId,
        _l1_capacity: u64,
    ) -> Self {
        Self {
            l1: Arc::new(Mutex::new(HashMap::new())),
            namespace,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Look up `key`. Returns `Some(value)` on hit, `None` on miss.
    pub async fn get(&self, key: &K) -> Option<V>
    where
        K: Serialize,
    {
        let cache_key = self.make_key(key);
        let guard = self.l1.lock().await;
        let bytes = guard.get(&cache_key)?;
        serde_json::from_slice(bytes).ok()
    }

    /// Insert `value` under `key`.
    pub async fn put(&self, key: &K, value: V)
    where
        K: Serialize,
    {
        let cache_key = self.make_key(key);
        if let Ok(bytes) = serde_json::to_vec(&value) {
            let mut guard = self.l1.lock().await;
            guard.insert(cache_key, bytes);
        }
    }

    /// The namespace this cache writes into.
    #[must_use]
    pub fn namespace(&self) -> &str {
        self.namespace
    }

    fn make_key(&self, key: &K) -> String
    where
        K: Serialize,
    {
        // WHY: use canonical JSON for deterministic key bytes; prepend namespace
        // to avoid collisions across independent SoulCache instances.
        let json = serde_json::to_string(key).unwrap_or_default();
        format!("{}:{}", self.namespace, json)
    }
}

// ─── HelixSnapshotId ─────────────────────────────────────────────────────────

/// Snapshot anchor for cache invalidation (stub — real type in soul-cache-substrate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelixSnapshotId(u64);

impl HelixSnapshotId {
    /// Construct from a Unix timestamp millis (e.g. `chrono::Utc::now().timestamp_millis()`).
    ///
    /// Millis before the Unix epoch (negative `i64`) are treated as epoch-relative
    /// by reinterpreting as `u64` via `wrapping_cast`; this is acceptable for a
    /// stub snapshot anchor whose only use is cache invalidation identity.
    #[must_use]
    pub fn from_timestamp_millis(millis: i64) -> Self {
        // WHY u64::try_from fails for pre-epoch timestamps; use saturate_to_zero
        // so the stub never panics on negative input. Snapshot semantics only
        // require distinct values, not epoch-accurate arithmetic.
        Self(u64::try_from(millis).unwrap_or(0))
    }
}
