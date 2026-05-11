//! Wave 3.2 performance gate — Criterion benchmarks.
//!
//! Targets from `wave-3-2.json` `gate_predicates_to_evaluate_at_wave_close`:
//! - HMAC sign / verify: < 5 ms per call
//! - `ScopeGovernor` evaluation: < 50 ms per call
//!
//! Run via (with workspace temporarily enabled):
//! ```bash
//! cargo bench -p lightarchitects-gateway --bench wave32_perf
//! ```
//!
//! Note: `enforce_operator_action` calls `emit_hook_span`, which does
//! `tokio::runtime::Handle::try_current()`. In the benchmark context there is
//! no tokio runtime, so `emit_hook_span` returns immediately after a `tracing::trace!`
//! call. The benchmark therefore measures pure gate evaluation latency — the
//! blocking portion callers actually wait on. The async AYIN span write is
//! fire-and-forget and never on the caller's critical path.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::semicolon_if_nothing_returned  // Criterion bench closures return ()
)]

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use lightarchitects_gateway::{
    governance::{ScopeGovernorContext, enforce_operator_action},
    security::hmac::{HookPayload, replay_window_check, sign_hook_payload, verify_hook_payload},
};

fn fresh_payload() -> HookPayload {
    HookPayload {
        assertion_id: "assert-bench-001".into(),
        action_type: "resolve-assertion".into(),
        operator_id: "op-bench".into(),
        timestamp_iso8601: chrono::Utc::now().to_rfc3339(),
    }
}

const BENCH_SECRET: &[u8] = b"bench-secret-key-32-bytes-pad!!!";

fn bench_hmac_sign(c: &mut Criterion) {
    let payload = fresh_payload();
    c.bench_function("hmac_sign_hook_payload", |b| {
        b.iter(|| sign_hook_payload(black_box(&payload), black_box(BENCH_SECRET)).unwrap())
    });
}

fn bench_hmac_verify_valid(c: &mut Criterion) {
    let payload = fresh_payload();
    let sig = sign_hook_payload(&payload, BENCH_SECRET).unwrap();
    c.bench_function("hmac_verify_hook_payload_valid", |b| {
        b.iter(|| {
            verify_hook_payload(
                black_box(&payload),
                black_box(BENCH_SECRET),
                black_box(&sig),
            )
            .unwrap()
        })
    });
}

fn bench_hmac_verify_invalid(c: &mut Criterion) {
    let payload = fresh_payload();
    // wrong_sig is intentionally 64 chars (correct length, wrong content) so that
    // ct_eq_bytes runs the full content comparison rather than returning early on
    // a length mismatch. This exercises the constant-time content path.
    let wrong_sig = "0".repeat(64);
    c.bench_function("hmac_verify_hook_payload_invalid", |b| {
        b.iter(|| {
            verify_hook_payload(
                black_box(&payload),
                black_box(BENCH_SECRET),
                black_box(&wrong_sig),
            )
            .unwrap()
        })
    });
}

fn bench_replay_window(c: &mut Criterion) {
    // Use iter_batched so the timestamp is refreshed between batches (setup cost
    // excluded from timing). Without this, the timestamp captured before b.iter()
    // ages ~40s while preceding benches run — dangerously close to the 60s window.
    c.bench_function("replay_window_check", |b| {
        b.iter_batched(
            || chrono::Utc::now().to_rfc3339(),
            |ts| replay_window_check(black_box(&ts)).unwrap(),
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_scope_governor_pass(c: &mut Criterion) {
    // Timestamp is captured once. Criterion warmup (3s) + collection (5s) = ~8s,
    // safely inside the 300s OPERATOR_ACTION_TTL_SECS window.
    let ctx = ScopeGovernorContext {
        operator_id: "op-bench".into(),
        build_id: "build-bench-001".into(),
        tool: "resolve-assertion".into(),
        timestamp_iso8601: chrono::Utc::now().to_rfc3339(),
        authorized_builds: vec!["build-bench-001".into()],
        allowed_tools: vec!["resolve-assertion".into()],
        concurrent_count: 0,
        concurrent_limit: 5,
    };
    c.bench_function("scope_governor_enforce_pass", |b| {
        b.iter(|| enforce_operator_action(black_box(&ctx)))
    });
}

fn bench_scope_governor_reject_ttl(c: &mut Criterion) {
    let expired_ts = (chrono::Utc::now() - chrono::Duration::seconds(400)).to_rfc3339();
    let ctx = ScopeGovernorContext {
        operator_id: "op-bench".into(),
        build_id: "build-bench-001".into(),
        tool: "resolve-assertion".into(),
        timestamp_iso8601: expired_ts,
        authorized_builds: vec!["build-bench-001".into()],
        allowed_tools: vec!["resolve-assertion".into()],
        concurrent_count: 0,
        concurrent_limit: 5,
    };
    c.bench_function("scope_governor_enforce_reject_ttl", |b| {
        b.iter(|| enforce_operator_action(black_box(&ctx)))
    });
}

criterion_group!(
    benches,
    bench_hmac_sign,
    bench_hmac_verify_valid,
    bench_hmac_verify_invalid,
    bench_replay_window,
    bench_scope_governor_pass,
    bench_scope_governor_reject_ttl,
);
criterion_main!(benches);
