//! Neo4j circuit breaker — protects handlers from cascading pool exhaustion.
//!
//! State machine:
//! - **Closed** — normal operation; all queries proceed.
//! - **Open** — tripped after 5 consecutive failures; queries are rejected with
//!   HTTP 503 until the recovery window elapses.
//! - **HalfOpen** — one probe is permitted after 30 s; success → Closed,
//!   failure → Open (timer reset).

use std::time::{Duration, Instant};

/// Consecutive failures before the circuit trips to Open.
const OPEN_THRESHOLD: u32 = 5;
/// Duration after which an Open circuit transitions to HalfOpen.
const HALF_OPEN_AFTER: Duration = Duration::from_secs(30);

/// Current state of the Neo4j circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CbState {
    /// All queries permitted; no recent failures.
    Closed,
    /// Circuit tripped; queries rejected until recovery window elapses.
    Open,
    /// Recovery probe window; one query permitted to test Neo4j liveness.
    HalfOpen,
}

/// Per-process circuit breaker for the Neo4j connection pool.
///
/// Wrap in `Arc<tokio::sync::Mutex<CircuitBreaker>>` and store in
/// [`PlatformState`](super::state::PlatformState) so all HTTP handlers
/// share a single circuit-breaker instance.
pub struct CircuitBreaker {
    /// Current circuit state.
    pub state: CbState,
    consecutive_failures: u32,
    opened_at: Option<Instant>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker in the `Closed` state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: CbState::Closed,
            consecutive_failures: 0,
            opened_at: None,
        }
    }

    /// Returns `true` if a query may proceed; transitions Open → HalfOpen when
    /// the recovery window has elapsed.
    pub fn is_available(&mut self) -> bool {
        match self.state {
            CbState::Closed | CbState::HalfOpen => true,
            CbState::Open => {
                if self.opened_at.is_some_and(|t| t.elapsed() >= HALF_OPEN_AFTER) {
                    self.state = CbState::HalfOpen;
                    tracing::info!("Neo4j circuit breaker → HalfOpen; probe query permitted");
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Record a successful Neo4j query — reset failure count and close the circuit.
    pub fn record_success(&mut self) {
        if self.state != CbState::Closed {
            tracing::info!(
                previous = ?self.state,
                "Neo4j circuit breaker → Closed"
            );
        }
        self.consecutive_failures = 0;
        self.opened_at = None;
        self.state = CbState::Closed;
    }

    /// Record a failed Neo4j query — trip the circuit when the threshold is reached.
    ///
    /// In [`CbState::HalfOpen`] a single failure immediately re-Opens the circuit
    /// (reset timer) so that one bad probe does not require 5 cumulative failures.
    pub fn record_failure(&mut self) {
        if self.state == CbState::HalfOpen {
            tracing::warn!(
                "Neo4j circuit breaker → Open (HalfOpen probe failed; resetting timer)",
            );
            self.state = CbState::Open;
            self.opened_at = Some(Instant::now());
            return;
        }
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures >= OPEN_THRESHOLD {
            tracing::warn!(
                consecutive_failures = self.consecutive_failures,
                recovery_after_secs = HALF_OPEN_AFTER.as_secs(),
                "Neo4j circuit breaker → Open (potential pool exhaustion)",
            );
            self.state = CbState::Open;
            self.opened_at = Some(Instant::now());
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}
