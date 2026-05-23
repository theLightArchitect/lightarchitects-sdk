//! Ironclaw-spine Phase 7 soak harness.
//!
//! Verifies the autonomous build pipeline (lightsquad delivery_arena) remains
//! stable over extended continuous operation. Default duration is 7 days;
//! override with `SOAK_DURATION_SECS` for CI short-circuit (e.g. `60`).
//!
//! # What is exercised
//!
//! The `autonomous::BuildSession`, `ironclaw::BuildConfig`, and
//! `decisions::DecisionsWriter` types are **pre-stub** at Phase 7 plan time.
//! This harness therefore exercises the ironclaw pipeline using:
//!
//! - A mock wave→gate→fix→merge cycle implemented via [`simulate_wave`].
//! - Real HMAC chain integrity via [`lightarchitects::crypto::hash`] (the same
//!   primitive the production gate will use once stubs land).
//! - [`lightarchitects::ayin::span::Actor`] for actor labelling in the timing
//!   layer (compatible with the AYIN trace record shape).
//!
//! When the `autonomous` module ships, replace [`SimulatedBuildSession`] and
//! the `simulate_wave` call site with the real types — the metric accounting
//! and assertion logic below do not change.
//!
//! # Assertions
//!
//! | Invariant | Threshold |
//! |-----------|-----------|
//! | Gate-fail rate | ≤ 1 % of completed waves |
//! | HMAC chain integrity | `hmac_chain_verified == wave_count` |
//! | No panics | runtime panic = test abort |
//!
//! # Running
//!
//! ```text
//! # 7-day soak (excluded from `cargo test` default):
//! cargo test --test ironclaw-7-day soak_7_day -- --ignored
//!
//! # 60-second CI smoke:
//! SOAK_DURATION_SECS=60 cargo test --test ironclaw-7-day soak_7_day -- --ignored
//! ```

#![allow(
    clippy::cast_precision_loss,  // u64→f64 counter casts; values stay well within mantissa range
    clippy::float_cmp,            // assert_eq!(0.0) on exact zero-path return
    clippy::expect_used,          // test-file: expect() is idiomatic for test assertions
    clippy::doc_markdown,         // test-file: doc comments don't need rigorous backtick coverage
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use lightarchitects::ayin::span::Actor;
use lightarchitects::crypto::hash::{hmac_hash, hmac_verify};
use secrecy::SecretString;
use tokio::time::sleep;

// ── Constants ────────────────────────────────────────────────────────────────

/// Default soak duration: 7 days in seconds.
const DEFAULT_DURATION_SECS: u64 = 7 * 24 * 60 * 60;

/// Inter-wave yield — keeps the async scheduler healthy during tight loops.
const WAVE_YIELD_MS: u64 = 10;

/// Simulated gate-fail injection rate (1 in N waves fails).
/// Drives gate_fail counter; must stay ≤ 1 % of total waves.
const GATE_FAIL_INJECTION_RATE: u64 = 200;

/// Maximum tolerated gate-fail rate (1 %).
const MAX_GATE_FAIL_RATE: f64 = 0.01;

// ── Metrics ──────────────────────────────────────────────────────────────────

/// Atomic counters tracking every ironclaw pipeline event.
///
/// All fields use [`Ordering::Relaxed`] for accumulation — final reads use
/// [`Ordering::SeqCst`] before assertions to ensure a consistent view.
#[derive(Debug, Default)]
struct SoakMetrics {
    /// Total waves dispatched (including failed).
    wave_count: AtomicU64,
    /// Waves that passed all gates and merged cleanly.
    gate_pass: AtomicU64,
    /// Waves that failed at least one gate.
    gate_fail: AtomicU64,
    /// Total fix-agent invocations triggered by gate failures.
    fix_agent_invocations: AtomicU64,
    /// Waves whose HMAC chain was verified end-to-end.
    hmac_chain_verified: AtomicU64,
}

impl SoakMetrics {
    fn increment_wave(&self) {
        self.wave_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_pass(&self) {
        self.gate_pass.fetch_add(1, Ordering::Relaxed);
    }

    fn record_fail(&self) {
        self.gate_fail.fetch_add(1, Ordering::Relaxed);
    }

    fn record_fix_agent(&self) {
        self.fix_agent_invocations.fetch_add(1, Ordering::Relaxed);
    }

    fn record_hmac_verified(&self) {
        self.hmac_chain_verified.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> MetricSnapshot {
        // SeqCst on final read to ensure a globally consistent view.
        MetricSnapshot {
            wave_count: self.wave_count.load(Ordering::SeqCst),
            gate_pass: self.gate_pass.load(Ordering::SeqCst),
            gate_fail: self.gate_fail.load(Ordering::SeqCst),
            fix_agent_invocations: self.fix_agent_invocations.load(Ordering::SeqCst),
            hmac_chain_verified: self.hmac_chain_verified.load(Ordering::SeqCst),
        }
    }
}

/// Point-in-time copy of [`SoakMetrics`] for assertions and logging.
#[derive(Debug)]
struct MetricSnapshot {
    wave_count: u64,
    gate_pass: u64,
    gate_fail: u64,
    fix_agent_invocations: u64,
    hmac_chain_verified: u64,
}

impl MetricSnapshot {
    /// Gate-fail rate as a fraction of total waves.
    ///
    /// Returns `0.0` when `wave_count` is zero to avoid division by zero.
    fn gate_fail_rate(&self) -> f64 {
        if self.wave_count == 0 {
            return 0.0;
        }
        self.gate_fail as f64 / self.wave_count as f64
    }
}

// ── Simulated pipeline types ─────────────────────────────────────────────────
//
// Replace these with the real autonomous types once stubs land:
//   `use lightarchitects_gateway::autonomous::BuildSession;`
//   `use lightarchitects_gateway::ironclaw::BuildConfig;`
//   `use lightarchitects_gateway::decisions::DecisionsWriter;`

/// Placeholder: represents a single autonomous build session.
///
/// Production type will carry session state (worktree path, LASDLC tier,
/// gating log, and the manifest.yaml reference). This stub records only
/// what the soak harness needs to verify the HMAC chain and gate outcomes.
struct SimulatedBuildSession {
    /// Monotonically increasing wave index within the session.
    wave_index: u64,
    /// HMAC key for the chain — shared with the gate verifier.
    hmac_key: SecretString,
    /// AYIN actor label for trace records.
    actor: Actor,
}

impl SimulatedBuildSession {
    fn new(wave_index: u64, hmac_key: SecretString) -> Self {
        Self {
            wave_index,
            hmac_key,
            actor: Actor::new("ironclaw-soak"),
        }
    }

    /// Returns the actor label (used in trace records).
    fn actor(&self) -> &Actor {
        &self.actor
    }
}

/// Outcome of a single wave cycle.
#[derive(Debug, PartialEq, Eq)]
enum WaveOutcome {
    /// Gate passed; change merged.
    Merged,
    /// Gate failed; fix-agent was invoked; change retried and merged.
    FixedAndMerged,
    /// Gate failed and fix-agent could not resolve the issue.
    Failed,
}

// ── HMAC chain helpers ────────────────────────────────────────────────────────

/// Compute the HMAC token for a given wave.
///
/// Uses the real [`hmac_hash`] primitive so the soak exercises the same
/// cryptographic path as the production gate chain.
///
/// # Errors
///
/// Propagates [`lightarchitects::crypto::error::CryptoError`] on HMAC failure
/// (key rejection — cannot happen with SHA-256, which accepts any key length).
fn wave_hmac(key: &SecretString, wave_index: u64) -> Result<String, String> {
    let payload = format!("wave:{wave_index}");
    hmac_hash(key, payload.as_bytes()).map_err(|e| format!("hmac_hash failed: {e}"))
}

/// Verify the HMAC token produced by [`wave_hmac`].
///
/// Returns `true` when the token is authentic, `false` when it does not match
/// (chain integrity violation — should never occur in a correct implementation).
///
/// # Errors
///
/// Propagates HMAC initialisation errors.
fn verify_wave_hmac(key: &SecretString, wave_index: u64, token: &str) -> Result<bool, String> {
    let payload = format!("wave:{wave_index}");
    hmac_verify(key, payload.as_bytes(), token).map_err(|e| format!("hmac_verify failed: {e}"))
}

// ── Single wave simulation ────────────────────────────────────────────────────

/// Simulate a complete wave→gate→fix→merge cycle.
///
/// Injects a gate failure on every `GATE_FAIL_INJECTION_RATE`-th wave to
/// exercise the fix-agent code path under sustained load.
///
/// # Errors
///
/// Returns a `String` error when HMAC operations fail. All other failures are
/// expressed as [`WaveOutcome::Failed`] — they are counted, not propagated.
async fn simulate_wave(
    session: &SimulatedBuildSession,
    metrics: &SoakMetrics,
) -> Result<(), String> {
    metrics.increment_wave();

    // ── Step 1: Mint HMAC token for this wave (production: gate signs manifest hash).
    let token = wave_hmac(&session.hmac_key, session.wave_index)?;

    // ── Step 2: Run gate (production: 7-pillar LASDLC gate evaluation).
    let gate_failed = session.wave_index % GATE_FAIL_INJECTION_RATE == 0 && session.wave_index > 0;

    let outcome = if gate_failed {
        metrics.record_fail();

        // ── Step 3: Invoke fix-agent (production: FixAgent spawned by ReviewGate).
        metrics.record_fix_agent();

        // Simulate fix-agent resolving the failure (always succeeds in the
        // soak model; production fix-agents may exhaust retries → Failed).
        WaveOutcome::FixedAndMerged
    } else {
        WaveOutcome::Merged
    };

    // ── Step 4: Record gate pass for non-failed waves.
    if outcome != WaveOutcome::Failed {
        metrics.record_pass();
    }

    // ── Step 5: Verify HMAC chain end-to-end.
    //
    // In production the gate verifier re-derives the token from the manifest
    // and checks it against the token minted at wave dispatch. Here we
    // simulate that round-trip with the same key + index.
    let verified = verify_wave_hmac(&session.hmac_key, session.wave_index, &token)?;

    if verified {
        metrics.record_hmac_verified();
    } else {
        // HMAC chain break — record but do not panic; let the final assertion
        // surface the discrepancy with a clear message.
        let actor = session.actor();
        tracing::error!(
            actor = %actor,
            wave = session.wave_index,
            "HMAC chain integrity violation — token mismatch"
        );
    }

    // Yield to the scheduler so other tasks (health check, AYIN reporter) run.
    sleep(Duration::from_millis(WAVE_YIELD_MS)).await;

    Ok(())
}

// ── Soak loop ─────────────────────────────────────────────────────────────────

/// Drive the wave loop for `duration`, emitting a progress log every 60 s.
async fn run_soak_loop(
    hmac_key: SecretString,
    metrics: Arc<SoakMetrics>,
    duration: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + duration;
    let mut wave_index: u64 = 0;
    let mut last_log = Instant::now();

    while Instant::now() < deadline {
        let session = SimulatedBuildSession::new(wave_index, hmac_key.clone());
        simulate_wave(&session, &metrics).await?;
        wave_index = wave_index.saturating_add(1);

        // Emit a progress snapshot every 60 seconds so CI logs show liveness.
        if last_log.elapsed() >= Duration::from_secs(60) {
            let snap = metrics.snapshot();
            tracing::info!(
                wave_count = snap.wave_count,
                gate_pass = snap.gate_pass,
                gate_fail = snap.gate_fail,
                fix_agent_invocations = snap.fix_agent_invocations,
                hmac_chain_verified = snap.hmac_chain_verified,
                gate_fail_rate_pct = snap.gate_fail_rate() * 100.0,
                elapsed_secs = last_log.elapsed().as_secs(),
                "ironclaw soak — progress snapshot"
            );
            last_log = Instant::now();
        }
    }

    Ok(())
}

// ── Assertions ────────────────────────────────────────────────────────────────

/// Validate the final metric snapshot against the soak invariants.
///
/// Returns `Ok(())` when all invariants hold, or a `String` describing the
/// first violation detected.
fn assert_soak_invariants(snap: &MetricSnapshot) -> Result<(), String> {
    // Invariant 1: gate-fail rate must not exceed 1 %.
    let fail_rate = snap.gate_fail_rate();
    if fail_rate > MAX_GATE_FAIL_RATE {
        return Err(format!(
            "gate_fail_rate {:.4} % exceeds threshold {:.1} % \
             (wave_count={}, gate_fail={})",
            fail_rate * 100.0,
            MAX_GATE_FAIL_RATE * 100.0,
            snap.wave_count,
            snap.gate_fail,
        ));
    }

    // Invariant 2: every wave must have a verified HMAC chain entry.
    if snap.hmac_chain_verified != snap.wave_count {
        return Err(format!(
            "HMAC chain integrity broken: hmac_chain_verified={} != wave_count={}",
            snap.hmac_chain_verified, snap.wave_count
        ));
    }

    // Invariant 3: no waves can be unaccounted for (pass + fail == wave_count).
    let accounted = snap.gate_pass.saturating_add(snap.gate_fail);
    if accounted != snap.wave_count {
        return Err(format!(
            "metric accounting mismatch: gate_pass ({}) + gate_fail ({}) = {} != wave_count {}",
            snap.gate_pass, snap.gate_fail, accounted, snap.wave_count
        ));
    }

    Ok(())
}

// ── Test entry point ──────────────────────────────────────────────────────────

/// 7-day (or `SOAK_DURATION_SECS`-second) ironclaw pipeline soak test.
///
/// Marked `#[ignore]` so it does not run in the default `cargo test` sweep.
/// Invoke explicitly:
///
/// ```text
/// cargo test --test ironclaw-7-day soak_7_day -- --ignored
/// ```
///
/// For CI short-circuit:
///
/// ```text
/// SOAK_DURATION_SECS=60 cargo test --test ironclaw-7-day soak_7_day -- --ignored
/// ```
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "long-running soak test; run explicitly with -- --ignored"]
async fn soak_7_day() {
    // Resolve duration from env, falling back to 7 days.
    let duration_secs = std::env::var("SOAK_DURATION_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_DURATION_SECS);

    let duration = Duration::from_secs(duration_secs);

    // Stable HMAC key for the full soak run (production: derived from build
    // manifest pepper at session open time).
    let hmac_key = SecretString::from("ironclaw-soak-hmac-pepper-v1");

    let metrics = Arc::new(SoakMetrics::default());

    // Run the wave loop to completion.
    let result = run_soak_loop(hmac_key, Arc::clone(&metrics), duration).await;

    // Any HMAC error propagation surfaces here — assert before checking metrics
    // so the error message is clear.
    assert!(
        result.is_ok(),
        "soak loop returned an error: {}",
        result.err().unwrap_or_default()
    );

    let snap = metrics.snapshot();

    eprintln!(
        "\n[ironclaw soak] final metrics\n\
         ├─ wave_count            : {}\n\
         ├─ gate_pass             : {}\n\
         ├─ gate_fail             : {}\n\
         ├─ fix_agent_invocations : {}\n\
         ├─ hmac_chain_verified   : {}\n\
         └─ gate_fail_rate        : {:.4} %\n",
        snap.wave_count,
        snap.gate_pass,
        snap.gate_fail,
        snap.fix_agent_invocations,
        snap.hmac_chain_verified,
        snap.gate_fail_rate() * 100.0,
    );

    // Assert all soak invariants.
    let verdict = assert_soak_invariants(&snap);
    assert!(
        verdict.is_ok(),
        "soak invariant violated: {}",
        verdict.err().unwrap_or_default()
    );
}

// ── Unit tests for helpers ────────────────────────────────────────────────────
//
// These run in the default `cargo test` sweep and validate that the soak
// infrastructure itself is correct before a long run is committed.

#[cfg(test)]
mod unit {
    use super::*;

    fn test_key() -> SecretString {
        SecretString::from("test-soak-key-for-unit-tests")
    }

    // ── MetricSnapshot helpers ────────────────────────────────────────────

    #[test]
    fn gate_fail_rate_zero_when_no_waves() {
        let snap = MetricSnapshot {
            wave_count: 0,
            gate_pass: 0,
            gate_fail: 0,
            fix_agent_invocations: 0,
            hmac_chain_verified: 0,
        };
        assert_eq!(snap.gate_fail_rate(), 0.0);
    }

    #[test]
    fn gate_fail_rate_one_percent() {
        let snap = MetricSnapshot {
            wave_count: 1000,
            gate_pass: 990,
            gate_fail: 10,
            fix_agent_invocations: 10,
            hmac_chain_verified: 1000,
        };
        let rate = snap.gate_fail_rate();
        // 10/1000 = exactly 1 % — on the threshold, should pass.
        assert!((rate - 0.01).abs() < f64::EPSILON);
    }

    // ── HMAC chain helpers ────────────────────────────────────────────────

    #[test]
    fn wave_hmac_deterministic() {
        let key = test_key();
        let a = wave_hmac(&key, 42).expect("hmac a");
        let b = wave_hmac(&key, 42).expect("hmac b");
        assert_eq!(a, b, "same wave index must yield identical token");
    }

    #[test]
    fn wave_hmac_differs_across_waves() {
        let key = test_key();
        let t0 = wave_hmac(&key, 0).expect("wave 0");
        let t1 = wave_hmac(&key, 1).expect("wave 1");
        assert_ne!(t0, t1, "consecutive wave tokens must differ");
    }

    #[test]
    fn hmac_round_trip_verifies() {
        let key = test_key();
        let token = wave_hmac(&key, 7).expect("mint");
        let ok = verify_wave_hmac(&key, 7, &token).expect("verify");
        assert!(ok, "round-trip verification must succeed");
    }

    #[test]
    fn hmac_wrong_index_fails_verification() {
        let key = test_key();
        let token = wave_hmac(&key, 7).expect("mint for wave 7");
        let ok = verify_wave_hmac(&key, 8, &token).expect("verify against wave 8");
        assert!(!ok, "token minted for wave 7 must not verify for wave 8");
    }

    // ── assert_soak_invariants ────────────────────────────────────────────

    #[test]
    fn invariants_pass_clean_run() {
        let snap = MetricSnapshot {
            wave_count: 100,
            gate_pass: 100,
            gate_fail: 0,
            fix_agent_invocations: 0,
            hmac_chain_verified: 100,
        };
        assert!(assert_soak_invariants(&snap).is_ok());
    }

    #[test]
    fn invariants_pass_with_injected_failures_below_threshold() {
        // Injection rate is 1-in-200, so 1000 waves → 4 failures (0.4 %).
        let snap = MetricSnapshot {
            wave_count: 1000,
            gate_pass: 996,
            gate_fail: 4,
            fix_agent_invocations: 4,
            hmac_chain_verified: 1000,
        };
        assert!(assert_soak_invariants(&snap).is_ok());
    }

    #[test]
    fn invariants_reject_high_fail_rate() {
        let snap = MetricSnapshot {
            wave_count: 100,
            gate_pass: 89,
            gate_fail: 11, // 11 % — above 1 % threshold
            fix_agent_invocations: 11,
            hmac_chain_verified: 100,
        };
        let err = assert_soak_invariants(&snap);
        assert!(err.is_err(), "11 % failure rate must be rejected");
        let msg = err.err().unwrap_or_default();
        assert!(
            msg.contains("gate_fail_rate"),
            "error message should name the violated field; got: {msg}"
        );
    }

    #[test]
    fn invariants_reject_hmac_chain_mismatch() {
        let snap = MetricSnapshot {
            wave_count: 100,
            gate_pass: 100,
            gate_fail: 0,
            fix_agent_invocations: 0,
            hmac_chain_verified: 99, // one wave unverified
        };
        let err = assert_soak_invariants(&snap);
        assert!(err.is_err(), "HMAC chain mismatch must be rejected");
    }

    #[test]
    fn invariants_reject_accounting_mismatch() {
        // gate_fail: 1 on wave_count: 100 = exactly 1 % — passes the fail-rate
        // guard — so the accounting check (90 + 1 = 91 != 100) is reached.
        let snap = MetricSnapshot {
            wave_count: 100,
            gate_pass: 90, // 90 + 1 = 91 != 100
            gate_fail: 1,
            fix_agent_invocations: 1,
            hmac_chain_verified: 100,
        };
        let err = assert_soak_invariants(&snap);
        assert!(err.is_err(), "accounting mismatch must be rejected");
        let msg = err.err().unwrap_or_default();
        assert!(
            msg.contains("accounting mismatch"),
            "error should cite accounting mismatch; got: {msg}"
        );
    }

    // ── SoakMetrics atomics ───────────────────────────────────────────────

    #[test]
    fn metrics_snapshot_consistent_after_increments() {
        let m = SoakMetrics::default();
        for _ in 0..10 {
            m.increment_wave();
            m.record_pass();
            m.record_hmac_verified();
        }
        let snap = m.snapshot();
        assert_eq!(snap.wave_count, 10);
        assert_eq!(snap.gate_pass, 10);
        assert_eq!(snap.gate_fail, 0);
        assert_eq!(snap.hmac_chain_verified, 10);
        assert_eq!(snap.fix_agent_invocations, 0);
    }
}
