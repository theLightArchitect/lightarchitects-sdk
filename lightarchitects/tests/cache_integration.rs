//! Integration tests for `SoulCache` — exercises the full L1 + L2 stack
//! against a real temporary filesystem (no mocks beyond `NullSoulCacheStore`).
//!
//! Run: `cargo test -p lightarchitects --features soul-cache --test cache_integration`

#![cfg(feature = "soul-cache")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{sync::Arc, time::Duration};

use lightarchitects::agent::cache::{
    CacheKey, HelixSnapshotId, HelixSoulCacheStore, NullSoulCacheStore, SoulCache,
};

// ── shared helpers ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct IntKey(String);

impl CacheKey for IntKey {
    fn canonical_bytes(&self) -> Vec<u8> {
        self.0.as_bytes().to_vec()
    }
}

fn snap() -> HelixSnapshotId {
    HelixSnapshotId::from_timestamp(chrono::Utc::now())
}

// ── Test 1: cache_persists_across_instances ───────────────────────────────────

/// G-SUBSTRATE-02 — L2 write survives process-restart simulation:
/// a second `SoulCache` instance using the same store reads back the value
/// written by the first.
#[tokio::test]
async fn cache_persists_across_instances() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(HelixSoulCacheStore::with_root(tmp.path().to_owned()));

    // Instance A — write.
    {
        let cache_a: SoulCache<IntKey, String> =
            SoulCache::new("persist-test", Arc::clone(&store) as Arc<_>, snap(), 100);
        cache_a
            .put(&IntKey("restart-key".to_owned()), "survived".to_owned())
            .await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    } // cache_a dropped — L1 gone.

    // Instance B — read from L2 only.
    let cache_b: SoulCache<IntKey, String> = SoulCache::new("persist-test", store, snap(), 100);
    let result = cache_b.get(&IntKey("restart-key".to_owned())).await;
    assert_eq!(result, Some("survived".to_owned()));
}

// ── Test 2: null_store_degraded_mode ─────────────────────────────────────────

/// G-SUBSTRATE-05 — when SOUL is unavailable, L1 still functions and L2
/// silently degrades (no panic, no error).
#[tokio::test]
async fn null_store_degraded_mode() {
    let cache: SoulCache<IntKey, String> =
        SoulCache::new("degraded-test", Arc::new(NullSoulCacheStore), snap(), 100);

    cache
        .put(&IntKey("k".to_owned()), "in-l1-only".to_owned())
        .await;
    assert_eq!(
        cache.get(&IntKey("k".to_owned())).await,
        Some("in-l1-only".to_owned())
    );
    // Key not in L1 → L2 returns None → overall None (no panic).
    assert_eq!(cache.get(&IntKey("absent".to_owned())).await, None);
}

// ── Test 3: l1_eviction_triggers_l2_promote ───────────────────────────────────

/// After an L1 eviction (capacity 1), a second write pushes the first entry
/// out; reading the first key falls through to L2 and re-promotes it.
#[tokio::test]
async fn l1_eviction_triggers_l2_promote() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(HelixSoulCacheStore::with_root(tmp.path().to_owned()));

    // Capacity 1 — inserting a second key evicts the first from L1.
    let cache: SoulCache<IntKey, String> =
        SoulCache::new("evict-test", Arc::clone(&store) as Arc<_>, snap(), 1);

    let key_a = IntKey("evict-a".to_owned());
    let key_b = IntKey("evict-b".to_owned());

    cache.put(&key_a, "alpha".to_owned()).await;
    // Flush L2 write for key_a before evicting.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Inserting key_b evicts key_a from L1 (capacity = 1).
    cache.put(&key_b, "beta".to_owned()).await;
    // Flush L2 write for key_b.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Reading key_a must find it in L2 and promote.
    let result = cache.get(&key_a).await;
    assert_eq!(
        result,
        Some("alpha".to_owned()),
        "L2 promote after L1 eviction"
    );
}

// ── Test 4: concurrent_readers_get_same_value ─────────────────────────────────

/// Multiple concurrent readers all see the same value — verifies Send + Sync
/// under contention.
#[tokio::test]
async fn concurrent_readers_get_same_value() {
    let cache: SoulCache<IntKey, String> = SoulCache::new(
        "concurrent-test",
        Arc::new(NullSoulCacheStore),
        snap(),
        1_000,
    );

    let key = IntKey("shared".to_owned());
    cache.put(&key, "shared-value".to_owned()).await;

    // Spawn 16 concurrent readers.
    let handles: Vec<_> = (0..16)
        .map(|_| {
            let c = cache.clone();
            let k = key.clone();
            tokio::spawn(async move { c.get(&k).await })
        })
        .collect();

    for handle in handles {
        let result = handle.await.expect("task panicked");
        assert_eq!(result, Some("shared-value".to_owned()));
    }
}
