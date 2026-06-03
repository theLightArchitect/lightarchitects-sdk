//! Property tests for `SoulCache<K, V>`.
//!
//! 1000 random `(key, value)` pairs verify:
//!   - `get(put(k, v)) == Some(v)` — round-trip invariant.
//!   - After `invalidate_snapshot()`, `get(k) == None` with `NullSoulCacheStore`.

#[cfg(test)]
mod proptest_suite {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use std::sync::Arc;

    use proptest::prelude::*;

    use crate::agent::cache::{CacheKey, HelixSnapshotId, NullSoulCacheStore, SoulCache};

    // ── Arbitrary key type ────────────────────────────────────────────────────

    #[derive(Debug, Clone)]
    struct ArbitraryKey(Vec<u8>);

    impl CacheKey for ArbitraryKey {
        fn canonical_bytes(&self) -> Vec<u8> {
            self.0.clone()
        }
    }

    fn snap() -> HelixSnapshotId {
        HelixSnapshotId::from_timestamp(chrono::Utc::now())
    }

    // ── Property: get(put(k, v)) == Some(v) ──────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1_000))]

        #[test]
        fn put_then_get_roundtrip(
            key_bytes in proptest::collection::vec(any::<u8>(), 1..=64),
            value in ".*",
        ) {
            let rt = tokio::runtime::Runtime::new().expect("runtime");
            rt.block_on(async {
                let cache: SoulCache<ArbitraryKey, String> = SoulCache::new(
                    "proptest",
                    Arc::new(NullSoulCacheStore),
                    snap(),
                    10_000,
                );
                let key = ArbitraryKey(key_bytes);
                cache.put(&key, value.clone()).await;
                prop_assert_eq!(cache.get(&key).await, Some(value));
                Ok(())
            })?;
        }

        // ── Property: invalidate_snapshot → L1 cleared ───────────────────────

        #[test]
        fn invalidate_snapshot_evicts_all(
            key_bytes in proptest::collection::vec(any::<u8>(), 1..=64),
            value in ".*",
        ) {
            let rt = tokio::runtime::Runtime::new().expect("runtime");
            rt.block_on(async {
                let mut cache: SoulCache<ArbitraryKey, String> = SoulCache::new(
                    "proptest-inv",
                    Arc::new(NullSoulCacheStore),
                    snap(),
                    10_000,
                );
                let key = ArbitraryKey(key_bytes);
                cache.put(&key, value).await;

                let new_snap = HelixSnapshotId::from_timestamp(
                    chrono::Utc::now() + chrono::Duration::seconds(1),
                );
                cache.invalidate_snapshot(new_snap);

                // L1 cleared; NullStore returns None → overall miss.
                prop_assert_eq!(cache.get(&key).await, None);
                Ok(())
            })?;
        }
    }
}
