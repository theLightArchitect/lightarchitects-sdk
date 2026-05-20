# ADR-015: wave_dispatcher critical-path scheduling (LAMaS pattern)

**Status**: Accepted
**Date**: 2026-05-18
**Authors**: Kevin (architect), Claude (engineer)
**Phase prerequisite**: Phase 3 (wave_dispatcher.rs)
**Related**: ADR-009 (SDK), ADR-013 (serialized git ops), ADR-010 (worker spawn)
**Prior art (VALIDATED)**: LAMaS arXiv 2601.10560 (accessed 2026-05-18 via HuggingFace paper_search)

---

## Context

lightsquad's `wave_dispatcher` runs N tasks per wave using a 7-slot worker pool. Tasks
within a wave can have `depends_on` relationships (e.g., `p3-w2-merge-agent` depends on
`p3-w1-types`). Without dependency awareness, the dispatcher either (a) runs all tasks
in parallel regardless of dependencies (produces races) or (b) runs them serially (wastes
the worker pool).

The question: what scheduling algorithm minimises wall-clock wave latency while respecting task
dependencies?

## Decision

**Critical-path scheduling** following the LAMaS pattern (arXiv 2601.10560): tasks are
dispatched in dependency order, with eligible (unblocked) tasks scheduled onto available
worker slots immediately as their dependencies complete.

> **Validation status**: Core architectural claim VALIDATED (LAMaS arXiv 2601.10560 abstract
> + HuggingFace paper metadata confirmed ID). Specific 38–46% latency reduction metric is
> UNVALIDATED until paper-text quote obtained. Design adopts the structural pattern
> (dependency-aware dispatch) without claiming the specific metric.

```rust
// wave_dispatcher.rs

pub struct WaveDispatcher {
    worker_pool: WorkerPool,  // max 7 concurrent slots
    git_lock: Arc<Mutex<()>>, // shared with MergeAgent (ADR-013)
}

impl WaveDispatcher {
    pub async fn dispatch_wave(&self, wave: &Wave) -> WaveResult {
        let mut dag = TaskDag::from_wave(wave);
        let mut in_flight: JoinSet<TaskResult> = JoinSet::new();
        let mut completed: HashSet<TaskId> = HashSet::new();

        loop {
            // Schedule all eligible (depends_on ⊆ completed) tasks onto free slots
            while let Some(task) = dag.next_eligible(&completed) {
                if self.worker_pool.has_free_slot() {
                    let slot = self.worker_pool.acquire().await;
                    in_flight.spawn(self.run_task(slot, task));
                } else {
                    break;  // no free slots — wait for completions
                }
            }

            if in_flight.is_empty() {
                break;  // all tasks done or no eligible tasks remain
            }

            // Await next completion
            match in_flight.join_next().await {
                Some(Ok(result)) => {
                    completed.insert(result.task_id.clone());
                    dag.mark_done(&result.task_id);
                }
                Some(Err(e)) => return WaveResult::WorkerPanic(e),
                None => break,
            }
        }

        WaveResult::from_completed(completed, wave)
    }
}
```

## Consequences

- **Dependency-aware dispatch** — `TaskDag` tracks `depends_on` edges. A task only enters
  the dispatch queue when all its dependencies are in `completed`. No races from premature dispatch.
- **Worker pool cap = 7 slots** — R5 empirical test (macOS APFS, 2026-05-18) confirmed max
  8 concurrent worktrees before inode pressure begins. 7 = max − 1 (safety margin).
- **`JoinSet` fan-out / fan-in** — Tokio's `JoinSet` is the correct primitive for dynamic
  task sets where the total count is not known at compile time. (Alternate citation: AutoGen
  `DiGraphBuilder` uses equivalent JoinSet fan-in, Context7 verified 2026-05-18.)
- **Critical-path order** — eligible tasks are scheduled from longest-remaining-path first
  (critical path priority). In a wave with a 2-task chain A→B and a 1-task leaf C, B is
  scheduled before C to minimise the critical path blocking the next wave.
- **Wave latency** is bounded by the critical path, not total task count. In a fully-parallel
  wave of 7 independent tasks, wall-clock ≈ max(task_latency), not sum(task_latency).

## Alternatives rejected

- **Serial execution (one task at a time)**: Correct but leaves worker slots idle. At 7 tasks
  per wave, wastes 6/7 of available concurrency. Rejected.
- **Fully parallel (ignore depends_on)**: Produces races when later tasks read outputs of
  earlier tasks. Rejected.
- **Static topological sort (dispatch in fixed order)**: Does not react to partial completions.
  A slow task on the critical path blocks tasks that could run in parallel. Rejected in favour
  of the dynamic eligible-task-ready loop.
