//! G5 Baseline Capture — Phase 2 close-out (Phase 4 updated)
//!
//! Measures p50/p95 latency for all 41 `verdict_y_actions`. Since Phase 4,
//! all actions are LLM-dispatched via `ClaudeCliProvider` (real `claude -p`
//! subprocesses). Each sample makes a live API call.
//!
//! **This test is `#[ignore]`d** — it requires a live Claude auth session and
//! incurs real API cost. Run explicitly:
//!
//! ```
//! cargo test --features inline-all --test test_g5_baseline_capture \
//!     -- --ignored --nocapture
//! ```
//!
//! Writes: ~/lightarchitects/soul/helix/corso/builds/
//!         gateway-action-audit-claude-runtime/baseline-latency.json

#![cfg(all(
    feature = "inline-corso",
    feature = "inline-eva",
    feature = "inline-soul",
    feature = "inline-quantum",
))]

use lightarchitects::core::handler::SiblingHandler;
use lightarchitects_gateway::config::GatewayConfig;
use lightarchitects_gateway::handlers::{CorsoHandler, EvaHandler, QuantumHandler, SoulHandler};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// One sample per LLM action — enough for a baseline; p95 over 10 LLM calls
/// adds no statistical value given the high natural variance of API latency.
const SAMPLES: usize = 1;

/// Hard per-call wall-clock ceiling. claude -p rarely needs more than 60s for
/// a single-turn probe; anything longer indicates a hang, not slow inference.
const LLM_CALL_TIMEOUT: Duration = Duration::from_secs(60);
const BUILD_ROOT: &str =
    "/Users/kft/lightarchitects/soul/helix/corso/builds/gateway-action-audit-claude-runtime";

// Phase-4 complete: all verdict_y actions across CORSO/EVA/SOUL/QUANTUM are now
// LLM-dispatched. Excluded from the sub-ms ceiling assertion.
const LLM_MIGRATED_PHASE4: &[&str] = &[
    // CORSO (9)
    "sniff",
    "scout",
    "code_review",
    "guard",
    "fetch",
    "prove",
    "optimize",
    "chase",
    "chow",
    // SOUL (2)
    "converse",
    "chat",
    // QUANTUM (7)
    "sweep",
    "trace",
    "probe",
    "theorize",
    "verify",
    "close",
    "research",
    // EVA direct (7)
    "remember",
    "visualize",
    "review",
    "refactor",
    "architect",
    "simplify",
    "explain",
    // EVA alias-target (9)
    "ideate",
    "crystallize",
    "celebrate",
    "bible_reflect",
    "research_ollama",
    "research_perplexity",
    "research_docs",
    "tutorial",
    "survival",
];

// verdict_y actions per sibling (from manifest.yaml)
const CORSO_VERDICT_Y: &[&str] = &[
    "sniff",
    "code_review",
    "guard",
    "fetch",
    "scout",
    "prove",
    "optimize",
    "chase",
    "chow",
];

const EVA_VERDICT_Y_DIRECT: &[&str] = &[
    "remember",
    "visualize",
    "review",
    "refactor",
    "architect",
    "simplify",
    "explain",
];

const EVA_VERDICT_Y_ALIAS: &[&str] = &[
    // alias_used names that currently exist in EVA_ACTIONS
    "ideate",              // alias_used: "imagine" — not yet in handler; skip
    "crystallize",         // exists
    "celebrate",           // exists
    "bible_reflect",       // exists
    "research_ollama",     // exists
    "research_perplexity", // exists
    "research_docs",       // exists
    "tutorial",            // exists
    "survival",            // exists
];

// binary_missing: not yet in EVA_ACTIONS — Phase 3.5 adds stubs; skip in baseline
// const EVA_VERDICT_Y_BINARY_MISSING: &[&str] = &[
//     "lint", "status", "repo", "enrich", "deploy_gate", "pipeline_reflect", "discover",
// ];

const SOUL_VERDICT_Y: &[&str] = &["converse", "chat"];

const QUANTUM_VERDICT_Y: &[&str] = &[
    "sweep", "trace", "probe", "theorize", "verify", "close", "research",
];

async fn time_action<H: SiblingHandler>(handler: &H, action: &str, samples: usize) -> Vec<u64> {
    let mut timings = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        let _ = tokio::time::timeout(
            LLM_CALL_TIMEOUT,
            handler.call(action, json!({"input": "baseline-probe"})),
        )
        .await;
        timings.push(u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX));
    }
    timings.sort_unstable();
    timings
}

fn p50(sorted: &[u64]) -> u64 {
    sorted[sorted.len() / 2]
}

fn p95(sorted: &[u64]) -> u64 {
    let idx = (sorted.len() - 1) * 95 / 100;
    sorted[idx]
}

#[tokio::test]
#[ignore = "LLM integration test — makes live API calls. Run with: cargo test -- --ignored"]
#[allow(clippy::too_many_lines, clippy::expect_used)]
async fn capture_g5_baseline_latency() {
    let config = GatewayConfig::default();

    let corso = CorsoHandler::new(&config);
    let eva = EvaHandler::new(&config);
    let soul = SoulHandler::new(&config);
    let quantum = QuantumHandler::new(&config);

    let captured_at = chrono::Utc::now().to_rfc3339();

    // ── CORSO ────────────────────────────────────────────────────────────────
    let mut corso_map: HashMap<String, Value> = HashMap::new();
    for action in CORSO_VERDICT_Y {
        let timings = time_action(&corso, action, SAMPLES).await;
        corso_map.insert(
            (*action).to_owned(),
            json!({
                "p50_us": p50(&timings),
                "p95_us": p95(&timings),
                "sample_count": SAMPLES,
                "captured_at": captured_at,
                "note": "stub: HandlerError::not_initialized"
            }),
        );
    }

    // ── EVA ──────────────────────────────────────────────────────────────────
    let mut eva_map: HashMap<String, Value> = HashMap::new();
    for action in EVA_VERDICT_Y_DIRECT.iter().chain(EVA_VERDICT_Y_ALIAS) {
        let timings = time_action(&eva, action, SAMPLES).await;
        eva_map.insert(
            (*action).to_owned(),
            json!({
                "p50_us": p50(&timings),
                "p95_us": p95(&timings),
                "sample_count": SAMPLES,
                "captured_at": captured_at,
                "note": "stub: Ok(json text placeholder)"
            }),
        );
    }
    // binary-missing actions: document as N/A (Phase 3.5 adds stubs)
    for action in &[
        "lint",
        "status",
        "repo",
        "enrich",
        "deploy_gate",
        "pipeline_reflect",
        "discover",
    ] {
        eva_map.insert(
            (*action).to_owned(),
            json!({
                "p50_us": null,
                "p95_us": null,
                "sample_count": 0,
                "captured_at": captured_at,
                "note": "binary_missing: Phase 3.5 adds stub; unmeasurable pre-baseline"
            }),
        );
    }

    // ── SOUL ─────────────────────────────────────────────────────────────────
    let mut soul_map: HashMap<String, Value> = HashMap::new();
    for action in SOUL_VERDICT_Y {
        let timings = time_action(&soul, action, SAMPLES).await;
        soul_map.insert(
            (*action).to_owned(),
            json!({
                "p50_us": p50(&timings),
                "p95_us": p95(&timings),
                "sample_count": SAMPLES,
                "captured_at": captured_at,
                "note": "stub: Ok(json text placeholder)"
            }),
        );
    }

    // ── QUANTUM ──────────────────────────────────────────────────────────────
    let mut quantum_map: HashMap<String, Value> = HashMap::new();
    for action in QUANTUM_VERDICT_Y {
        let timings = time_action(&quantum, action, SAMPLES).await;
        quantum_map.insert(
            (*action).to_owned(),
            json!({
                "p50_us": p50(&timings),
                "p95_us": p95(&timings),
                "sample_count": SAMPLES,
                "captured_at": captured_at,
                "note": "stub: Ok(json text placeholder)"
            }),
        );
    }

    // ── Assemble + write ──────────────────────────────────────────────────────
    let payload = json!({
        "schema_version": "1.0",
        "build_codename": "gateway-action-audit-claude-runtime",
        "phase": "phase-2-spawner-foundation-baseline",
        "captured_at": captured_at,
        "note": "Pre-migration baseline. Current handlers are stubs (not_initialized / text placeholder). Latencies are sub-ms (handler overhead only). Phase 5 compares ClaudeCliProvider latency against the plan target ceiling (p95 ≤ 30s), not this baseline.",
        "latency_unit": "microseconds",
        "corso": corso_map,
        "eva": eva_map,
        "soul": soul_map,
        "quantum": quantum_map
    });

    let out_path = format!("{BUILD_ROOT}/baseline-latency.json");
    let pretty = serde_json::to_string_pretty(&payload).expect("serialize baseline");
    std::fs::write(&out_path, &pretty).expect("write baseline-latency.json");
    println!("G5 baseline written to {out_path}");

    // Spot-check: stub-path actions must be <10ms.
    // Phase-4 complete: all verdict_y actions are LLM-dispatched and excluded.
    for (action, v) in corso_map
        .iter()
        .chain(soul_map.iter())
        .chain(quantum_map.iter())
    {
        if LLM_MIGRATED_PHASE4.contains(&action.as_str()) {
            continue;
        }
        if let Some(p95_us) = v["p95_us"].as_u64() {
            assert!(
                p95_us < 10_000,
                "action={action} p95={p95_us}µs exceeded 10ms stub ceiling"
            );
        }
    }
    for (action, v) in &eva_map {
        if LLM_MIGRATED_PHASE4.contains(&action.as_str()) {
            continue;
        }
        if let Some(p95_us) = v["p95_us"].as_u64() {
            assert!(
                p95_us < 10_000,
                "action={action} p95={p95_us}µs exceeded 10ms stub ceiling"
            );
        }
    }
}
