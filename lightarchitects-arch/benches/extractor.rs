use criterion::{Criterion, criterion_group, criterion_main};

fn bench_extractor_placeholder(c: &mut Criterion) {
    // Phase 2 will replace this with real extraction benchmarks over a 10K-file corpus.
    c.bench_function("extractor_placeholder", |b| b.iter(|| 0_u64));
}

criterion_group!(benches, bench_extractor_placeholder);
criterion_main!(benches);
