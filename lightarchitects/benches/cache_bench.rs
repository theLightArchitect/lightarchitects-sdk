//! Criterion benchmarks for `SoulCache<K, V>`.
//!
//! Measures L1 warm-hit, L2-miss (`NullStore`), and L2-promote latency
//! to verify the latency budget declared in the build contract:
//! - L1 p99 < 5ms
//! - L2 miss adds < 1ms overhead
//!
//! Run: `cargo bench --features soul-cache -p lightarchitects --bench cache_bench`

#![allow(missing_docs, clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use lightarchitects::agent::cache::{
    CacheKey, HelixSnapshotId, HelixSoulCacheStore, NullSoulCacheStore, SoulCache,
};

// ── BenchKey ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct BenchKey {
    id: u32,
}

impl CacheKey for BenchKey {
    fn canonical_bytes(&self) -> Vec<u8> {
        self.id.to_le_bytes().to_vec()
    }
}

fn snap() -> HelixSnapshotId {
    HelixSnapshotId::from_timestamp(chrono::Utc::now())
}

// ── L1 warm-hit ───────────────────────────────────────────────────────────────

fn bench_l1_warm_hit(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    // Pre-populate 1000 entries into L1.
    let cache: SoulCache<BenchKey, String> = rt.block_on(async {
        let store = Arc::new(NullSoulCacheStore);
        let c = SoulCache::new("bench-l1", store, snap(), 2_000);
        for i in 0u32..1_000 {
            c.put(&BenchKey { id: i }, format!("v{i}")).await;
        }
        c
    });

    c.bench_function("l1_warm_hit", |b| {
        b.iter(|| rt.block_on(async { black_box(cache.get(&BenchKey { id: 42 }).await) }));
    });
}

// ── L2 miss with NullStore ────────────────────────────────────────────────────

fn bench_l2_miss_null(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    // Capacity 0 — moka evicts immediately; every get falls through to L2.
    let cache: SoulCache<BenchKey, String> = SoulCache::new(
        "bench-l2-miss",
        Arc::new(NullSoulCacheStore),
        snap(),
        1, // min non-zero capacity; NullStore always returns None so L2 miss always
    );

    c.bench_function("l2_miss_null_store", |b| {
        b.iter(|| rt.block_on(async { black_box(cache.get(&BenchKey { id: 1 }).await) }));
    });
}

// ── L2 promote via HelixStore ─────────────────────────────────────────────────

fn bench_l2_promote_helix(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(HelixSoulCacheStore::with_root(tmp.path().to_owned()));

    // Pre-write 100 entries to L2 via a separate instance.
    rt.block_on(async {
        let writer: SoulCache<BenchKey, String> = SoulCache::new(
            "bench-l2-promote",
            Arc::clone(&store) as Arc<_>,
            snap(),
            2_000,
        );
        for i in 0u32..100 {
            writer.put(&BenchKey { id: i }, format!("v{i}")).await;
        }
        // Wait for fire-and-forget L2 writes to flush.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    });

    // Reader with tiny L1 → L2 promote on every iteration.
    let reader: SoulCache<BenchKey, String> =
        SoulCache::new("bench-l2-promote", Arc::clone(&store) as Arc<_>, snap(), 1);

    c.bench_function("l2_promote_helix", |b| {
        b.iter(|| rt.block_on(async { black_box(reader.get(&BenchKey { id: 42 }).await) }));
    });
}

criterion_group!(
    cache_benches,
    bench_l1_warm_hit,
    bench_l2_miss_null,
    bench_l2_promote_helix
);
criterion_main!(cache_benches);
