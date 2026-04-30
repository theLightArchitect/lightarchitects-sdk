#![allow(missing_docs)]
// Criterion benchmark for the dispatch task classifier.
//
// Exit criterion: classifier_perf_under_5ms — p99 ≤ 5 ms for an 8 KB input
// (HIGH H-8 — aho-corasick only, no ReDoS).
//
// Run: cargo bench --package lightarchitects-webshell --bench classifier

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use lightarchitects_webshell::dispatch::classifier::classify;

fn bench_classifier(c: &mut Criterion) {
    let inputs: &[(&str, &str)] = &[
        ("empty", "hello world"),
        (
            "engineer",
            "implement and refactor the auth module and fix the bug in the pipeline",
        ),
        (
            "squad",
            "implement tests and document the new api security audit deployment pipeline",
        ),
        ("8kb", &"refactor auth security deploy ".repeat(250)), // ~7.5 KB
    ];

    let mut group = c.benchmark_group("classify");
    for (name, task) in inputs {
        group.bench_with_input(BenchmarkId::new("classify", name), *task, |b, t| {
            b.iter(|| classify(black_box(t)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_classifier);
criterion_main!(benches);
