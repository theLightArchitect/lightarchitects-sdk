use criterion::{Criterion, criterion_group, criterion_main};

fn bench_verifier_placeholder(c: &mut Criterion) {
    // Phase 4 will replace this with real drift-verifier benchmarks.
    c.bench_function("verifier_placeholder", |b| b.iter(|| 0_u64));
}

criterion_group!(benches, bench_verifier_placeholder);
criterion_main!(benches);
