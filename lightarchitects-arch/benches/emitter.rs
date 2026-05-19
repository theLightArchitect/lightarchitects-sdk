use criterion::{Criterion, criterion_group, criterion_main};

fn bench_emitter_placeholder(c: &mut Criterion) {
    // Phase 3 will replace this with real HTML/Mermaid emission benchmarks.
    c.bench_function("emitter_placeholder", |b| b.iter(|| 0_u64));
}

criterion_group!(benches, bench_emitter_placeholder);
criterion_main!(benches);
