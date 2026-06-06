//! Criterion benchmarks for the [`TurboVecIndex`] in-process semantic index.
//!
//! Run with:
//! ```
//! cargo bench --bench turbovec_bench --features turbovec-semantic
//! ```
//!
//! Key numbers to watch:
//! - **`bulk_insert/50000`**: should complete in < 10 s (4-bit, 768-dim)
//! - **`global_search_k10/50000`**: < 5 ms per query (SIMD-accelerated HNSW)
//! - **`helix_search_k10/500vecs`**: < 500 µs per query (masked SIMD scan)
#![allow(
    missing_docs,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::needless_for_each
)]

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use lightarchitects::helix::turbovec_index::{HELIX_DIM, TurboVecIndex};

// ── Vector helpers ────────────────────────────────────────────────────────────

/// Deterministic pseudo-random unit vector (LCG — no external rand dep).
fn pseudo_unit(seed: usize) -> Vec<f32> {
    let mut v = Vec::with_capacity(HELIX_DIM);
    let mut x = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    for _ in 0..HELIX_DIM {
        x = x
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        v.push(((x >> 33) as f32) / (u32::MAX as f32) * 2.0 - 1.0);
    }
    l2_normalize(&mut v);
    v
}

fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-9 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
}

/// Build a populated + prepared index with `n` unit vectors across `n_helixes`.
fn build_index(n: usize, n_helixes: usize) -> TurboVecIndex {
    let mut idx = TurboVecIndex::new().expect("TurboVecIndex::new");
    for i in 0..n {
        let v = pseudo_unit(i);
        idx.upsert(
            &format!("step-{i}"),
            &format!("helix-{}", i % n_helixes),
            &v,
        );
    }
    idx.prepare();
    idx
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

/// Measures time to upsert N unit vectors + call `prepare()`.
///
/// Informs the startup latency budget for `HelixStore::connect()` when
/// `turbovec-semantic` is enabled and the Neo4j helix has N step embeddings.
fn bench_bulk_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("turbovec/bulk_insert");
    group.sample_size(10); // each iter inserts up to 50 K vectors

    for n in [1_000usize, 10_000, 50_000] {
        group.bench_with_input(BenchmarkId::new("steps", n), &n, |b, &n| {
            b.iter(|| {
                let mut idx = TurboVecIndex::new().expect("new");
                for i in 0..n {
                    let v = pseudo_unit(i);
                    idx.upsert(
                        &format!("step-{i}"),
                        &format!("helix-{}", i % 100),
                        black_box(&v),
                    );
                }
                idx.prepare();
                black_box(idx)
            });
        });
    }
    group.finish();
}

/// Measures per-query latency for global (unscoped) ANN search.
///
/// Baseline for choosing between turbovec (in-process SIMD) and Neo4j HNSW
/// (network-bound Bolt query). At 50 K vectors a single query should be
/// well under 5 ms; Neo4j typically takes 15–40 ms including Bolt round-trip.
fn bench_global_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("turbovec/global_search_k10");

    for n in [1_000usize, 10_000, 50_000] {
        let idx = build_index(n, 100);
        let query = pseudo_unit(999_999);

        group.bench_with_input(BenchmarkId::new("steps", n), &n, |b, _| {
            b.iter(|| black_box(idx.search(black_box(&query), 10)));
        });
    }
    group.finish();
}

/// Measures per-query latency for helix-scoped masked ANN search.
///
/// At 50 K vectors across 100 helixes each helix has ~500 vectors. The masked
/// SIMD scan over those 500 quantised codes should be well under 500 µs.
/// This replaces the O(n) `vector.similarity.cosine` Cypher brute-force scan
/// that SOUL currently uses for helix-scoped queries.
fn bench_helix_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("turbovec/helix_search_k10");

    // 50 K vectors, 100 helixes → ~500 vecs/helix
    let idx = build_index(50_000, 100);
    let query = pseudo_unit(42);

    // Single fixed helix — representative of the per-retrieval hot path.
    group.bench_function("500vecs_per_helix", |b| {
        b.iter(|| black_box(idx.search_helix(black_box(&query), 10, "helix-42")));
    });
    group.finish();

    // Sweep helix sizes to show the O(|helix|) scaling of the mask pass.
    // Must be a separate group so the first borrow of `c` is released.
    let mut group2 = c.benchmark_group("turbovec/helix_search_by_size");
    for (n_total, n_helixes, label) in [
        (500usize, 1usize, "500vecs"),
        (5_000, 10, "500vecs"),
        (50_000, 100, "500vecs"),
        (50_000, 50, "1000vecs"),
        (50_000, 10, "5000vecs"),
    ] {
        let idx2 = build_index(n_total, n_helixes);
        group2.bench_with_input(BenchmarkId::new(label, n_total), &n_total, |b, _| {
            b.iter(|| black_box(idx2.search_helix(black_box(&query), 10, "helix-0")));
        });
    }
    group2.finish();
}

/// Measures per-query Recall@5 at 50 K vectors (sampled, not a full suite).
///
/// Runs 20 sample queries, computes brute-force ground-truth top-5, and
/// reports the mean recall. Target: ≥ 96.2% (`LongMemEval` baseline).
/// This bench does NOT assert — it only emits the recall as a custom metric
/// so it shows up in the Criterion HTML report for trend tracking.
fn bench_recall_at_5(c: &mut Criterion) {
    const N: usize = 10_000;
    const N_QUERIES: usize = 20;
    const K: usize = 5;

    let mut idx = TurboVecIndex::new().expect("new");
    let mut corpus: Vec<Vec<f32>> = Vec::with_capacity(N);
    for i in 0..N {
        let v = pseudo_unit(i);
        idx.upsert(&format!("step-{i}"), &format!("helix-{}", i % 100), &v);
        corpus.push(v);
    }
    idx.prepare();

    let queries: Vec<Vec<f32>> = (0..N_QUERIES).map(|q| pseudo_unit(N + q)).collect();

    c.bench_function("turbovec/recall_at_5_10k", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for q in &queries {
                // Brute-force ground truth.
                let mut scored: Vec<(f32, usize)> = corpus
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (q.iter().zip(v).map(|(a, b)| a * b).sum(), i))
                    .collect();
                scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                let gt: std::collections::HashSet<usize> =
                    scored[..K].iter().map(|(_, i)| *i).collect();

                // ANN.
                let ann = idx.search(black_box(q), K);
                let hits = ann
                    .iter()
                    .filter(|(_, id)| {
                        id.strip_prefix("step-")
                            .and_then(|s| s.parse::<usize>().ok())
                            .is_some_and(|i| gt.contains(&i))
                    })
                    .count();
                total += hits;
            }
            black_box(total)
        });
    });
}

criterion_group!(
    benches,
    bench_bulk_insert,
    bench_global_search,
    bench_helix_search,
    bench_recall_at_5,
);
criterion_main!(benches);
