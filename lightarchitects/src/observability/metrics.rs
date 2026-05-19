//! Google SRE Golden Signals + Apdex metrics for lightsquad wave execution.
//!
//! [`WaveMetrics`] is a plain typed struct (no OTEL SDK dependency) that the
//! caller populates after a wave completes and forwards to AYIN via HTTP.
//! The three computed signals correspond directly to the Google SRE Golden
//! Signals model:
//!
//! | Signal | Method |
//! |---|---|
//! | Latency | embedded in `latency_ms` field |
//! | Traffic | [`WaveMetrics::throughput_per_sec`] |
//! | Errors | [`WaveMetrics::error_rate`] |
//! | Saturation | derived externally from `worker_count` |
//!
//! Apdex is computed per the ITIL/Apdex Alliance formula for a **single
//! measurement** (one wave = one sample):
//!
//! | Condition | Score |
//! |---|---|
//! | `latency_ms ≤ target_ms` | `1.0` (satisfied) |
//! | `target_ms < latency_ms ≤ tolerating_ms` | `0.5` (tolerating) |
//! | `latency_ms > tolerating_ms` | `0.0` (frustrated) |

/// Execution metrics for a single lightsquad wave.
///
/// Populate after the wave's `JoinSet` drains, then forward to AYIN's
/// `/api/metrics` ingest endpoint. All fields use unsigned integers to
/// keep serialization simple — callers must ensure values fit before
/// construction.
///
/// # Example
///
/// ```
/// use lightarchitects::observability::metrics::WaveMetrics;
///
/// let m = WaveMetrics {
///     build_id: "ironclaw-spine".to_owned(),
///     wave_index: 0,
///     latency_ms: 8_500,
///     worker_count: 7,
///     tool_calls: 42,
///     errors: 1,
///     tokens_input: 18_000,
///     tokens_output: 4_200,
/// };
///
/// assert!(m.apdex(10_000, 30_000) > 0.9);
/// assert!(m.error_rate() < 0.1);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct WaveMetrics {
    /// Stable identifier for the lightsquad build.
    pub build_id: String,

    /// Zero-based index of this wave within the build.
    pub wave_index: u32,

    /// Total elapsed wall-clock time for the wave in milliseconds.
    pub latency_ms: u64,

    /// Number of worker slots active during this wave (1–7).
    pub worker_count: u8,

    /// Total tool calls dispatched across all workers in this wave.
    pub tool_calls: u32,

    /// Number of tool calls that returned an error or were aborted.
    pub errors: u32,

    /// Total input tokens consumed by all workers in this wave.
    pub tokens_input: u64,

    /// Total output tokens produced by all workers in this wave.
    pub tokens_output: u64,
}

impl WaveMetrics {
    /// Compute the Apdex score for this wave against the given thresholds.
    ///
    /// Uses the single-sample Apdex formula:
    /// - Returns `1.0` when `latency_ms ≤ target_ms` (satisfied).
    /// - Returns `0.5` when `target_ms < latency_ms ≤ tolerating_ms` (tolerating).
    /// - Returns `0.0` when `latency_ms > tolerating_ms` (frustrated).
    ///
    /// The result is always in `[0.0, 1.0]`.
    ///
    /// # Arguments
    ///
    /// * `target_ms` — the "satisfied" latency threshold in milliseconds.
    /// * `tolerating_ms` — the "tolerating" upper bound; values above this
    ///   are "frustrated". Must be `≥ target_ms` for meaningful results.
    pub fn apdex(&self, target_ms: u64, tolerating_ms: u64) -> f64 {
        if self.latency_ms <= target_ms {
            1.0
        } else if self.latency_ms <= tolerating_ms {
            0.5
        } else {
            0.0
        }
    }

    /// Compute the error rate as a fraction of total tool calls.
    ///
    /// Returns `errors / max(tool_calls, 1)` to avoid division by zero when
    /// no tool calls were recorded. Result is in `[0.0, 1.0]`.
    pub fn error_rate(&self) -> f64 {
        let denominator = f64::from(self.tool_calls.max(1));
        f64::from(self.errors) / denominator
    }

    /// Compute throughput as tool calls per second over `duration_ms`.
    ///
    /// Returns `tool_calls / (duration_ms / 1000.0)`. When `duration_ms` is
    /// zero the function returns `0.0` rather than panicking.
    ///
    /// # Arguments
    ///
    /// * `duration_ms` — elapsed time to use as the denominator. Callers may
    ///   pass `self.latency_ms` to get wave-scoped throughput.
    pub fn throughput_per_sec(&self, duration_ms: u64) -> f64 {
        if duration_ms == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let duration_secs = duration_ms as f64 / 1_000.0;
        f64::from(self.tool_calls) / duration_secs
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    fn wave(latency_ms: u64, tool_calls: u32, errors: u32) -> WaveMetrics {
        WaveMetrics {
            build_id: "test-build".to_owned(),
            wave_index: 0,
            latency_ms,
            worker_count: 4,
            tool_calls,
            errors,
            tokens_input: 1_000,
            tokens_output: 500,
        }
    }

    // ── apdex ─────────────────────────────────────────────────────────────────

    #[test]
    fn apdex_satisfied_returns_one() {
        let m = wave(5_000, 10, 0);
        assert_eq!(m.apdex(10_000, 30_000), 1.0);
    }

    #[test]
    fn apdex_at_target_boundary_returns_one() {
        let m = wave(10_000, 10, 0);
        assert_eq!(m.apdex(10_000, 30_000), 1.0);
    }

    #[test]
    fn apdex_tolerating_returns_half() {
        let m = wave(20_000, 10, 0);
        assert_eq!(m.apdex(10_000, 30_000), 0.5);
    }

    #[test]
    fn apdex_at_tolerating_boundary_returns_half() {
        let m = wave(30_000, 10, 0);
        assert_eq!(m.apdex(10_000, 30_000), 0.5);
    }

    #[test]
    fn apdex_frustrated_returns_zero() {
        let m = wave(60_000, 10, 0);
        assert_eq!(m.apdex(10_000, 30_000), 0.0);
    }

    #[test]
    fn apdex_in_range() {
        for &lat in &[0u64, 5_000, 10_000, 20_000, 30_000, 60_000] {
            let score = wave(lat, 1, 0).apdex(10_000, 30_000);
            assert!(
                (0.0..=1.0).contains(&score),
                "apdex({lat}) = {score} not in [0,1]"
            );
        }
    }

    // ── error_rate ────────────────────────────────────────────────────────────

    #[test]
    fn error_rate_zero_when_no_errors() {
        let m = wave(1_000, 10, 0);
        assert_eq!(m.error_rate(), 0.0);
    }

    #[test]
    fn error_rate_one_when_all_errors() {
        let m = wave(1_000, 5, 5);
        assert_eq!(m.error_rate(), 1.0);
    }

    #[test]
    fn error_rate_partial() {
        let m = wave(1_000, 10, 2);
        let rate = m.error_rate();
        assert!((rate - 0.2).abs() < f64::EPSILON * 10.0);
    }

    #[test]
    fn error_rate_no_tool_calls_returns_zero() {
        // Division-by-zero guard: max(0, 1) = 1 denominator.
        let m = wave(1_000, 0, 0);
        assert_eq!(m.error_rate(), 0.0);
    }

    // ── throughput_per_sec ────────────────────────────────────────────────────

    #[test]
    fn throughput_one_second() {
        let m = wave(1_000, 10, 0);
        let tps = m.throughput_per_sec(1_000);
        assert!((tps - 10.0).abs() < f64::EPSILON * 100.0);
    }

    #[test]
    fn throughput_two_seconds() {
        let m = wave(2_000, 20, 0);
        let tps = m.throughput_per_sec(2_000);
        assert!((tps - 10.0).abs() < f64::EPSILON * 100.0);
    }

    #[test]
    fn throughput_zero_duration_returns_zero() {
        let m = wave(0, 5, 0);
        assert_eq!(m.throughput_per_sec(0), 0.0);
    }
}
