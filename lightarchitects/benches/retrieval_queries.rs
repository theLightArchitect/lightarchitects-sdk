#![allow(missing_docs, clippy::cast_precision_loss, clippy::expect_used)]
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use lightarchitects::helix::soul_search::{
    graphsage::ProjectionWeights, hybrid::RetrievalMode, hybrid::SignalWeights,
};

/// Queries loaded from the 50-entry fixture.
fn load_queries() -> Vec<String> {
    let raw = include_str!("retrieval_queries.json");
    serde_json::from_str(raw).expect("fixture must be valid JSON")
}

/// Synthetic 384-dim query embedding — deterministic, zero-copy.
fn synthetic_query_embed(seed: f32) -> Vec<f32> {
    (0..384).map(|i| (i as f32 * seed).sin()).collect()
}

/// Bench: `RetrievalMode::weights_dynamic` — query-conditioned softmax fusion.
///
/// This is the hot path called once per retrieval request. Measures the cost
/// of the dot-product attention + softmax used to derive RRF weights at
/// query time.
fn bench_query_conditioned_graph_scoring(c: &mut Criterion) {
    let queries = load_queries();
    let embeds: Vec<Vec<f32>> = queries
        .iter()
        .enumerate()
        .map(|(i, _)| synthetic_query_embed(0.1 + i as f32 * 0.01))
        .collect();

    let mut group = c.benchmark_group("query_conditioned_scoring");

    for mode in &[
        RetrievalMode::KeywordDominated,
        RetrievalMode::Balanced,
        RetrievalMode::GraphWeighted,
    ] {
        group.bench_with_input(
            BenchmarkId::new("weights_dynamic", mode.as_str()),
            mode,
            |b, m| {
                let embed = &embeds[0];
                b.iter(|| {
                    let w: SignalWeights = black_box(m).weights_dynamic(Some(black_box(embed)));
                    black_box(w)
                });
            },
        );
    }

    // Batch: all 50 queries × Balanced mode (representative production load).
    group.bench_function("batch_50_balanced", |b| {
        b.iter(|| {
            let mut sum = 0.0_f64;
            for embed in &embeds {
                let w = RetrievalMode::Balanced.weights_dynamic(Some(black_box(embed)));
                sum += w.graph();
            }
            black_box(sum)
        });
    });

    group.finish();
}

/// Bench: `ProjectionWeights::project` — `GraphSAGE` linear projection.
///
/// Measures the 384→128-dim projection applied per node during embedding.
/// Targets ≤1 ms at p99 under representative 50-query load.
fn bench_graphsage_aggregate_p99(c: &mut Criterion) {
    let weights = ProjectionWeights::random_stable();
    let embeds: Vec<Vec<f32>> = (0..50)
        .map(|i| synthetic_query_embed(0.05 + i as f32 * 0.007))
        .collect();

    let mut group = c.benchmark_group("graphsage_projection");

    // Single embedding projection.
    group.bench_function("project_single_384d", |b| {
        let embed = &embeds[0];
        b.iter(|| {
            let out = weights.project(black_box(embed));
            black_box(out)
        });
    });

    // Batch: 50 queries — p99 proxy (slowest of 50 per Criterion iteration).
    group.bench_function("project_batch_50", |b| {
        b.iter(|| {
            let mut last = Vec::new();
            for embed in &embeds {
                last = weights.project(black_box(embed));
            }
            black_box(last)
        });
    });

    group.finish();
}

criterion_group!(
    retrieval_benches,
    bench_query_conditioned_graph_scoring,
    bench_graphsage_aggregate_p99,
);
criterion_main!(retrieval_benches);
