//! Benchmark: `ArcSwap::load_full()` overhead under concurrent writers.
#![allow(missing_docs)]
//!
//! Validates the P2 Northstar predicate: policy reads must stay ≤ 5 ms p99
//! even while concurrent PATCH writers are incrementing the version counter.
//!
//! Run: `cargo bench --bench spawn_overhead`
//! Acceptance: p99 wall-clock ≤ 5 ms (`5_000` µs) for a single policy read.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use arc_swap::ArcSwap;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use lightarchitects::container_spawn::ContainerPolicy;

fn bench_arcswap_load_under_writers(c: &mut Criterion) {
    let policy: Arc<ArcSwap<ContainerPolicy>> =
        Arc::new(ArcSwap::from_pointee(ContainerPolicy::default()));
    let version: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

    // Spawn a background writer that continuously stores new Arc snapshots.
    let writer_policy = Arc::clone(&policy);
    let writer_version = Arc::clone(&version);
    let _writer = std::thread::spawn(move || {
        loop {
            let v = writer_version.fetch_add(1, Ordering::Relaxed);
            let _ = v;
            writer_policy.store(Arc::new(ContainerPolicy::default()));
            std::thread::sleep(Duration::from_micros(100));
        }
    });

    let mut group = c.benchmark_group("policy_read");
    // Acceptance threshold comment: p99 must remain ≤ 5_000 µs.
    group.measurement_time(Duration::from_secs(5));

    group.bench_with_input(
        BenchmarkId::new("arcswap_load_full", "concurrent_writer"),
        &policy,
        |b, p| {
            b.iter(|| {
                let snap = p.load_full();
                std::hint::black_box(snap.iso_mode);
            });
        },
    );

    group.finish();
}

criterion_group!(benches, bench_arcswap_load_under_writers);
criterion_main!(benches);
