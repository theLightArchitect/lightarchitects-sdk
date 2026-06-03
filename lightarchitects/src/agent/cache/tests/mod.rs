//! Unit tests for `agent::cache`.
//!
//! Compile-time invariants (Send+Sync), L1 hit/miss semantics, null-store
//! degraded mode, and snapshot invalidation.

pub mod proptest_cache;

#[cfg(test)]
mod unit {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use std::{sync::Arc, time::Duration};

    use static_assertions::assert_impl_all;

    use crate::agent::cache::{
        CacheKey, HelixSnapshotId, HelixSoulCacheStore, NullSoulCacheStore, SoulCache, sha256,
    };

    // ── TestKey — minimal CacheKey impl ──────────────────────────────────────

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestKey {
        id: u32,
    }

    impl CacheKey for TestKey {
        fn canonical_bytes(&self) -> Vec<u8> {
            self.id.to_le_bytes().to_vec()
        }
    }

    fn test_snap() -> HelixSnapshotId {
        HelixSnapshotId::from_timestamp(chrono::Utc::now())
    }

    fn null_cache(capacity: u64) -> SoulCache<TestKey, String> {
        SoulCache::new(
            "unit-test",
            Arc::new(NullSoulCacheStore),
            test_snap(),
            capacity,
        )
    }

    // ── G-SUBSTRATE-01: Send + Sync invariant ────────────────────────────────

    #[test]
    fn send_sync_invariant() {
        assert_impl_all!(SoulCache<TestKey, String>: Send, Sync);
    }

    // ── L1 hit returns cached value ───────────────────────────────────────────

    #[tokio::test]
    async fn l1_hit_returns_cached_value() {
        let cache = null_cache(100);
        let key = TestKey { id: 1 };
        cache.put(&key, "hello".to_owned()).await;
        assert_eq!(cache.get(&key).await, Some("hello".to_owned()));
    }

    // ── Miss returns None with NullStore ─────────────────────────────────────

    #[tokio::test]
    async fn miss_returns_none() {
        let cache = null_cache(100);
        let key = TestKey { id: 99 };
        assert_eq!(cache.get(&key).await, None);
    }

    // ── G-SUBSTRATE-05: NullStore degraded mode ───────────────────────────────

    #[tokio::test]
    async fn null_store_degraded_mode() {
        let cache = null_cache(100);
        let key = TestKey { id: 2 };
        cache.put(&key, "x".to_owned()).await;
        // L1 hit — NullStore never blocks L1.
        assert_eq!(cache.get(&key).await, Some("x".to_owned()));
    }

    // ── G-SUBSTRATE-04: invalidate_snapshot clears L1 ────────────────────────

    #[tokio::test]
    async fn invalidate_snapshot_clears_stale() {
        let mut cache = null_cache(100);
        let key = TestKey { id: 3 };
        cache.put(&key, "stale".to_owned()).await;

        let new_snap =
            HelixSnapshotId::from_timestamp(chrono::Utc::now() + chrono::Duration::seconds(1));
        cache.invalidate_snapshot(new_snap);

        // L1 cleared; NullStore returns None → overall miss.
        assert_eq!(cache.get(&key).await, None);
    }

    // ── G-SUBSTRATE-03 / G-SUBSTRATE-02: L2 roundtrip via HelixStore ─────────

    #[tokio::test]
    async fn l2_entry_contains_required_fields() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(HelixSoulCacheStore::with_root(tmp.path().to_owned()));
        let cache: SoulCache<TestKey, String> = SoulCache::new("test", store, test_snap(), 100);

        let key = TestKey { id: 42 };
        cache.put(&key, "data".to_owned()).await;

        // Allow fire-and-forget L2 write to complete.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let hash = sha256(&key.canonical_bytes());
        let path = tmp
            .path()
            .join("test")
            .join(format!("{}.json", hex::encode(hash)));
        let raw = tokio::fs::read(&path).await.expect("L2 entry must exist");
        let parsed: String = serde_json::from_slice(&raw).expect("valid JSON");
        assert_eq!(parsed, "data");
    }

    #[tokio::test]
    async fn l2_promote_on_l1_miss() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(HelixSoulCacheStore::with_root(tmp.path().to_owned()));

        // Write via cache A.
        let cache_a: SoulCache<TestKey, String> = SoulCache::new(
            "promote-test",
            Arc::clone(&store) as Arc<_>,
            test_snap(),
            100,
        );
        let key = TestKey { id: 7 };
        cache_a.put(&key, "promoted".to_owned()).await;

        // Flush L2 write.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Read via cache B — cold L1, must hit L2 and promote.
        let cache_b: SoulCache<TestKey, String> =
            SoulCache::new("promote-test", store, test_snap(), 100);
        assert_eq!(cache_b.get(&key).await, Some("promoted".to_owned()));
        // Second get — must be L1 hit (no L2 call needed).
        assert_eq!(cache_b.get(&key).await, Some("promoted".to_owned()));
    }

    // ── snapshot accessor ─────────────────────────────────────────────────────

    #[test]
    fn snapshot_accessor_returns_current() {
        let snap = test_snap();
        let cache: SoulCache<TestKey, String> =
            SoulCache::new("snap-test", Arc::new(NullSoulCacheStore), snap.clone(), 10);
        assert_eq!(cache.snapshot(), &snap);
    }

    // ── namespace accessor ────────────────────────────────────────────────────

    #[test]
    fn namespace_accessor_returns_ns() {
        let cache: SoulCache<TestKey, String> =
            SoulCache::new("my-ns", Arc::new(NullSoulCacheStore), test_snap(), 10);
        assert_eq!(cache.namespace(), "my-ns");
    }
}
