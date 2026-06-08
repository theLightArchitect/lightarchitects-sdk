//! Reducer p99 latency test — verifies `Lightspace::reduce()` stays ≤ 1 ms
//! at the 99th percentile under a 10 000-call workload.
//!
//! Pattern: restore from a warm snapshot → reduce one Update event → measure.
//! Using `Lightspace::restore()` instead of `new()` per iteration avoids
//! measuring allocation cost, isolating the reduce path.
//!
//! Run with `--test-threads=1` for deterministic timing:
//!   `cargo test -p lightarchitects-webshell --test lightspace_perf -- --test-threads=1`

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::Utc;
use lightarchitects_lightspace::{
    CanvasEvent, Lightspace,
    types::{CardData, CardKind, CardState, Provenance, UpdateMode},
};
use std::time::Instant;
use uuid::Uuid;

fn test_provenance() -> Provenance {
    Provenance {
        agent: "perf-agent".to_owned(),
        source_uri: "helix://perf/test".to_owned(),
        span_id: None,
        ts: Utc::now(),
    }
}

fn card_event(id: &str) -> CanvasEvent {
    CanvasEvent::Card(CardData {
        id: id.to_owned(),
        kind: CardKind::Monitor,
        title: format!("perf-{id}"),
        content: serde_json::json!({"x": 0}),
        provenance: test_provenance(),
        state: CardState::Attached,
        attribution: None,
    })
}

/// Build a warm snapshot with `n` pre-inserted cards.
fn warm_snapshot(n: usize) -> lightarchitects_lightspace::snapshot::Snapshot {
    let mut ls = Lightspace::new(Uuid::new_v4());
    for i in 0..n {
        ls = ls
            .reduce(card_event(&format!("card-{i:03}")))
            .expect("insert warm card");
    }
    ls.snapshot()
}

/// P99 reducer latency must be ≤ 1 ms (1 000 000 ns) under 10 000 calls.
///
/// Each iteration: restore from a warm snapshot → reduce one Update event.
/// The `reduce()` path is pure (no I/O, no syscalls) so 1 ms is a generous budget.
#[test]
fn reducer_p99_under_1ms() {
    const ITERS: usize = 10_000;
    const P99_IDX: usize = 9_900; // index of the 99th percentile in a sorted vec
    const BUDGET_NS: u128 = 1_000_000; // 1 ms

    let snap = warm_snapshot(10);
    let card_ids: Vec<String> = (0..10).map(|i| format!("card-{i:03}")).collect();

    let mut times = Vec::with_capacity(ITERS);

    for i in 0u64..ITERS as u64 {
        let ls = Lightspace::restore(snap.clone());
        #[allow(clippy::cast_possible_truncation)]
        let idx = (i as usize) % card_ids.len();
        let event = CanvasEvent::Update {
            card_id: card_ids[idx].clone(),
            seq: i + 100_000,
            mode: UpdateMode::Replace,
            path: None,
            payload: serde_json::json!({"v": i}),
        };
        let t0 = Instant::now();
        let _ = ls.reduce(event);
        times.push(t0.elapsed().as_nanos());
    }

    times.sort_unstable();
    let p99 = times[P99_IDX];

    assert!(
        p99 <= BUDGET_NS,
        "reducer p99 = {p99} ns exceeds budget of {BUDGET_NS} ns (1 ms). \
         Run with --test-threads=1 if timing is unstable."
    );
}
