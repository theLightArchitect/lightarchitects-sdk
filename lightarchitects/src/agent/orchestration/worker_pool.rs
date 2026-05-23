//! Bounded-concurrency worker pool.
//!
//! Lifted from `lightsquad::wave_dispatcher` — generalises the 7-slot
//! `JoinSet` pattern to any async work item, removing the git-worktree
//! coupling.
//!
//! # Concurrency contract
//!
//! At most `capacity` tasks run concurrently. Items beyond the window queue
//! until a slot opens. Outputs are collected in completion order (not
//! submission order).

use std::future::Future;

use tokio::task::JoinSet;

/// Default slot count — mirrors IRONCLAW §7-Slot Agent Pool.
pub const DEFAULT_CAPACITY: usize = 7;

/// Bounded-concurrency pool.
///
/// Drive at most `capacity` async tasks concurrently. Excess tasks queue
/// internally until a slot opens.
pub struct WorkerPool {
    capacity: usize,
}

impl WorkerPool {
    /// Create a pool with a custom capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self { capacity }
    }

    /// Create a pool using the default capacity (7).
    #[must_use]
    pub fn default_capacity() -> Self {
        Self {
            capacity: DEFAULT_CAPACITY,
        }
    }

    /// Return the pool's concurrency capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Run `worker` for each item in `items`, bounded to `self.capacity`
    /// concurrent tasks.
    ///
    /// Outputs are returned in completion order (not submission order). Items
    /// that panic are silently dropped — the slot is reclaimed and the next
    /// queued item starts.
    pub async fn run_all<I, Item, F, Fut>(&self, items: I, worker: F) -> Vec<Fut::Output>
    where
        I: IntoIterator<Item = Item>,
        Item: Send + 'static,
        F: Fn(Item) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        let mut join_set: JoinSet<Fut::Output> = JoinSet::new();
        let mut items_iter = items.into_iter();
        let mut results = Vec::new();

        // Seed the initial window.
        for item in items_iter.by_ref().take(self.capacity) {
            let w = worker.clone();
            join_set.spawn(async move { w(item).await });
        }

        // Drain the set, refilling one slot per completion.
        while let Some(join_result) = join_set.join_next().await {
            if let Ok(output) = join_result {
                results.push(output);
            }
            if let Some(item) = items_iter.next() {
                let w = worker.clone();
                join_set.spawn(async move { w(item).await });
            }
        }

        results
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[tokio::test]
    async fn runs_all_items_and_collects_outputs() {
        let pool = WorkerPool::new(3);
        let items = 0u32..10;
        let outputs = pool.run_all(items, |n| async move { n * 2 }).await;

        let mut sorted = outputs.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0u32..10).map(|n| n * 2).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn respects_capacity_bound() {
        let capacity = 3;
        let pool = WorkerPool::new(capacity);
        let peak = Arc::new(Mutex::new(0usize));
        let current = Arc::new(Mutex::new(0usize));

        let items: Vec<_> = (0..10).collect();
        let peak_c = Arc::clone(&peak);
        let current_c = Arc::clone(&current);

        pool.run_all(items, move |_| {
            let peak_c = Arc::clone(&peak_c);
            let current_c = Arc::clone(&current_c);
            async move {
                {
                    let mut cur = current_c.lock().unwrap();
                    *cur += 1;
                    let mut pk = peak_c.lock().unwrap();
                    if *cur > *pk {
                        *pk = *cur;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                {
                    let mut cur = current_c.lock().unwrap();
                    *cur -= 1;
                }
            }
        })
        .await;

        let observed_peak = *peak.lock().unwrap();
        assert!(
            observed_peak <= capacity,
            "peak concurrency {observed_peak} exceeded capacity {capacity}"
        );
    }

    #[tokio::test]
    async fn empty_input_returns_empty() {
        let pool = WorkerPool::new(4);
        let out: Vec<u32> = pool
            .run_all(std::iter::empty::<u32>(), |n| async move { n })
            .await;
        assert!(out.is_empty());
    }

    #[test]
    fn default_capacity_is_seven() {
        assert_eq!(WorkerPool::default_capacity().capacity(), DEFAULT_CAPACITY);
        assert_eq!(DEFAULT_CAPACITY, 7);
    }
}
