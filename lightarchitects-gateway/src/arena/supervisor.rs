//! Process supervisor — proactive child monitoring with exponential backoff.
//!
//! Polls each sibling's health every [`POLL_INTERVAL`] via `McpPool::is_alive`.
//! Dead siblings are restarted with exponential backoff (5 s → 60 s cap).
//! Backoff resets after [`HEALTHY_RESET`] of uninterrupted uptime.
//! Consecutive failure counts are exposed for the alerting module.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use super::alerting::Alerter;
use super::mcp_pool::McpPool;

/// How often to check each sibling (seconds).
const POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Minimum restart delay (first retry).
const BACKOFF_MIN: Duration = Duration::from_secs(5);

/// Maximum restart delay (cap).
const BACKOFF_MAX: Duration = Duration::from_secs(60);

/// Backoff multiplier per consecutive failure.
const BACKOFF_FACTOR: u32 = 2;

/// Time of continuous healthy operation before backoff resets.
const HEALTHY_RESET: Duration = Duration::from_secs(300);

/// Per-sibling supervisor state.
#[derive(Debug)]
struct SiblingState {
    /// Current backoff duration for the next restart attempt.
    backoff: Duration,
    /// Number of consecutive restart failures (resets on successful spawn).
    consecutive_failures: u32,
    /// When the sibling was last confirmed alive (for healthy-reset logic).
    last_healthy: Instant,
    /// When the last restart was attempted (for backoff gating).
    last_restart_attempt: Option<Instant>,
}

impl SiblingState {
    fn new() -> Self {
        Self {
            backoff: BACKOFF_MIN,
            consecutive_failures: 0,
            last_healthy: Instant::now(),
            last_restart_attempt: None,
        }
    }

    /// Record a successful health check — may reset backoff.
    fn mark_healthy(&mut self) {
        if self.last_healthy.elapsed() >= HEALTHY_RESET && self.consecutive_failures > 0 {
            tracing::info!("Backoff reset after {}s healthy", HEALTHY_RESET.as_secs());
            self.backoff = BACKOFF_MIN;
            self.consecutive_failures = 0;
        }
        self.last_healthy = Instant::now();
    }

    /// Record a restart attempt — advances backoff.
    fn mark_restart_failed(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.backoff = Duration::from_secs(
            (self
                .backoff
                .as_secs()
                .saturating_mul(u64::from(BACKOFF_FACTOR)))
            .min(BACKOFF_MAX.as_secs()),
        );
        self.last_restart_attempt = Some(Instant::now());
    }

    /// Record a successful restart — reset failure count (but keep backoff
    /// until the healthy-reset window passes).
    fn mark_restart_succeeded(&mut self) {
        self.last_healthy = Instant::now();
        self.last_restart_attempt = Some(Instant::now());
    }

    /// Whether enough time has passed since the last attempt to try again.
    fn backoff_elapsed(&self) -> bool {
        self.last_restart_attempt.is_none()
            || self
                .last_restart_attempt
                .is_some_and(|t| t.elapsed() >= self.backoff)
    }
}

/// Shared supervisor state for all siblings.
pub struct SupervisorHandle {
    states: Mutex<HashMap<String, SiblingState>>,
}

impl SupervisorHandle {
    /// Create a new handle pre-populated with all sibling names.
    fn new(siblings: &[String]) -> Self {
        let mut map = HashMap::new();
        for name in siblings {
            map.insert(name.clone(), SiblingState::new());
        }
        Self {
            states: Mutex::new(map),
        }
    }

    /// Get the consecutive failure count for a sibling (used by alerting).
    pub async fn consecutive_failures(&self, sibling: &str) -> u32 {
        let states = self.states.lock().await;
        states.get(sibling).map_or(0, |s| s.consecutive_failures)
    }

    /// Check whether a sibling is considered healthy by the supervisor.
    pub async fn is_healthy(&self, sibling: &str) -> bool {
        let states = self.states.lock().await;
        states
            .get(sibling)
            .is_some_and(|s| s.consecutive_failures == 0)
    }
}

/// Spawn the supervisor background task. Returns a handle for querying state.
pub fn spawn(pool: Arc<McpPool>, alerter: Option<Arc<Alerter>>) -> Arc<SupervisorHandle> {
    let siblings = pool.sibling_names();
    let handle = Arc::new(SupervisorHandle::new(&siblings));
    let handle_clone = Arc::clone(&handle);

    tokio::spawn(async move {
        run_loop(pool, handle_clone, alerter).await;
    });

    handle
}

/// Main supervisor loop — polls all siblings, restarts dead ones.
async fn run_loop(
    pool: Arc<McpPool>,
    handle: Arc<SupervisorHandle>,
    alerter: Option<Arc<Alerter>>,
) {
    let siblings = pool.sibling_names();
    tracing::info!(count = siblings.len(), "Supervisor started");

    loop {
        for name in &siblings {
            let alive = pool.is_alive(name).await;

            let mut states = handle.states.lock().await;
            let state = states.entry(name.clone()).or_insert_with(SiblingState::new);

            if alive {
                state.mark_healthy();
                continue;
            }

            // Sibling is dead — attempt restart if backoff has elapsed
            if !state.backoff_elapsed() {
                tracing::debug!(
                    sibling = %name,
                    backoff_secs = state.backoff.as_secs(),
                    "Waiting for backoff before restart"
                );
                continue;
            }

            tracing::warn!(
                sibling = %name,
                failures = state.consecutive_failures,
                backoff_secs = state.backoff.as_secs(),
                "Sibling dead — attempting restart"
            );

            // Drop the lock before the potentially slow respawn call
            let failures_before = state.consecutive_failures;
            drop(states);

            match pool.respawn(name).await {
                Ok(()) => {
                    let mut states = handle.states.lock().await;
                    if let Some(s) = states.get_mut(name.as_str()) {
                        s.mark_restart_succeeded();
                    }
                    tracing::info!(sibling = %name, "Supervisor restarted sibling");
                }
                Err(e) => {
                    let mut states = handle.states.lock().await;
                    if let Some(s) = states.get_mut(name.as_str()) {
                        s.mark_restart_failed();
                    }
                    let new_failures = states
                        .get(name.as_str())
                        .map_or(0, |s| s.consecutive_failures);
                    tracing::error!(
                        sibling = %name,
                        error = %e,
                        failures = new_failures,
                        "Supervisor restart failed"
                    );

                    // Alert if threshold crossed
                    if let Some(ref alerter) = alerter {
                        // Only alert on the threshold crossing, not every failure
                        if new_failures >= alerter.threshold()
                            && failures_before < alerter.threshold()
                        {
                            let msg = format!(
                                "Arena: {name} failed {new_failures} consecutive restarts. Last error: {e}"
                            );
                            alerter.send_alert(&msg).await;
                        }
                    }
                }
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
