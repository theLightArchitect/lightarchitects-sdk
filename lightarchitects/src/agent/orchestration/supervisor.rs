//! Supervisor — monitors a [`WorkerPool`] for error-rate circuit breaking.
//!
//! Wraps a [`WorkerPool`] with a configurable failure-rate threshold. If the
//! observed failure rate exceeds `max_failure_rate` at any point during
//! execution, the supervisor trips the circuit breaker: pending tasks are
//! abandoned and a [`SupervisorResult`] with `tripped: true` is returned.
//!
//! # Usage
//!
//! ```rust,no_run
//! use lightarchitects::agent::orchestration::{Supervisor, WorkerPool};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let supervisor = Supervisor::new(WorkerPool::new(4), 0.5);
//! let result = supervisor
//!     .run(0u32..8, |n| async move { if n % 3 == 0 { Err("bad") } else { Ok(n) } })
//!     .await;
//! println!("succeeded: {}, failed: {}, tripped: {}", result.succeeded, result.failed, result.tripped);
//! # }
//! ```

use std::future::Future;

use tokio::task::JoinSet;

use super::worker_pool::WorkerPool;

// ── Result type ───────────────────────────────────────────────────────────────

/// Result from a supervised run.
#[derive(Debug, Clone, PartialEq)]
pub struct SupervisorResult {
    /// Number of tasks that completed with `Ok(_)`.
    pub succeeded: usize,
    /// Number of tasks that completed with `Err(_)` or panicked.
    pub failed: usize,
    /// Whether the circuit breaker tripped during this run.
    pub tripped: bool,
}

impl SupervisorResult {
    /// Observed failure rate (0.0 when no tasks ran).
    #[must_use]
    pub fn failure_rate(&self) -> f64 {
        let total = self.succeeded + self.failed;
        if total == 0 {
            0.0
        } else {
            // Cast via u32 to avoid clippy::cast_precision_loss on 64-bit targets.
            // Task counts never reach u32::MAX in practice.
            #[allow(clippy::cast_possible_truncation)]
            let f = f64::from(self.failed as u32);
            #[allow(clippy::cast_possible_truncation)]
            let t = f64::from(total as u32);
            f / t
        }
    }
}

// ── Supervisor ────────────────────────────────────────────────────────────────

/// Circuit-breaker supervisor wrapping a [`WorkerPool`].
pub struct Supervisor {
    pool: WorkerPool,
    /// Trip circuit breaker when observed failure rate exceeds this threshold.
    max_failure_rate: f64,
}

impl Supervisor {
    /// Create a supervisor with a custom pool and failure-rate threshold.
    ///
    /// `max_failure_rate` must be in `0.0..=1.0`; values outside this range
    /// are clamped.
    #[must_use]
    pub fn new(pool: WorkerPool, max_failure_rate: f64) -> Self {
        Self {
            pool,
            max_failure_rate: max_failure_rate.clamp(0.0, 1.0),
        }
    }

    /// Run `worker` for each item, bounded by the pool's capacity.
    ///
    /// Each worker must return `Result<T, E>`. The supervisor tracks successes
    /// and failures in real time. If the failure rate exceeds
    /// `self.max_failure_rate` at any checkpoint, remaining tasks are abandoned
    /// and the run returns with `tripped: true`.
    ///
    /// Outputs are discarded; the caller receives only the aggregate counts.
    pub async fn run<I, Item, F, Fut, T, E>(&self, items: I, worker: F) -> SupervisorResult
    where
        I: IntoIterator<Item = Item>,
        Item: Send + 'static,
        F: Fn(Item) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: Send + 'static,
    {
        let capacity = self.pool.capacity();
        let mut join_set: JoinSet<Result<T, E>> = JoinSet::new();
        let mut items_iter = items.into_iter();

        let mut succeeded = 0usize;
        let mut failed = 0usize;

        // Seed initial window.
        for item in items_iter.by_ref().take(capacity) {
            let w = worker.clone();
            join_set.spawn(async move { w(item).await });
        }

        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(Ok(_)) => succeeded += 1,
                Ok(Err(_)) | Err(_) => failed += 1,
            }

            // Circuit-breaker check after each completion.
            let total = succeeded + failed;
            if total > 0 {
                #[allow(clippy::cast_possible_truncation)]
                let rate = f64::from(failed as u32) / f64::from(total as u32);
                if rate > self.max_failure_rate {
                    join_set.abort_all();
                    return SupervisorResult {
                        succeeded,
                        failed,
                        tripped: true,
                    };
                }
            }

            // Refill one slot.
            if let Some(item) = items_iter.next() {
                let w = worker.clone();
                join_set.spawn(async move { w(item).await });
            }
        }

        SupervisorResult {
            succeeded,
            failed,
            tripped: false,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn all_success_no_trip() {
        let sup = Supervisor::new(WorkerPool::new(4), 0.5);
        let result = sup.run(0u32..8, |_| async { Ok::<_, &str>(()) }).await;
        assert_eq!(result.succeeded, 8);
        assert_eq!(result.failed, 0);
        assert!(!result.tripped);
    }

    #[tokio::test]
    async fn trips_when_failure_rate_exceeded() {
        // All tasks fail → 100% failure rate > 0% threshold → trip on first failure.
        let sup = Supervisor::new(WorkerPool::new(4), 0.0);
        let result = sup.run(0u32..8, |_| async { Err::<(), _>("fail") }).await;
        assert!(result.tripped);
        assert!(result.failed >= 1);
    }

    #[tokio::test]
    async fn does_not_trip_below_threshold() {
        // 3 successes then 1 failure → 25% rate ≤ 50% threshold → no trip.
        // Capacity=1 makes completion order deterministic (items processed in sequence),
        // so the failure is never the first observation.
        let sup = Supervisor::new(WorkerPool::new(1), 0.5);
        let result = sup
            .run(0u32..4, |n| async move {
                if n == 3 {
                    Err::<(), _>("one bad")
                } else {
                    Ok(())
                }
            })
            .await;
        assert!(!result.tripped);
        assert_eq!(result.succeeded, 3);
        assert_eq!(result.failed, 1);
    }

    #[test]
    fn failure_rate_zero_when_no_tasks() {
        let r = SupervisorResult {
            succeeded: 0,
            failed: 0,
            tripped: false,
        };
        assert!(r.failure_rate().abs() < f64::EPSILON);
    }

    #[test]
    fn failure_rate_calculation() {
        let r = SupervisorResult {
            succeeded: 3,
            failed: 1,
            tripped: false,
        };
        assert!((r.failure_rate() - 0.25).abs() < f64::EPSILON);
    }
}
