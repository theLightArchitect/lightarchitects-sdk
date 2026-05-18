//! Parallel task dispatch (fan-out / fan-in) for delivery arena waves.
//!
//! Phase 3 implementation:
//! - `dispatch_wave(coord, feat_branch, tasks: Vec<TaskSpec>) -> Result<WaveResult, _>`
//! - `prepare_wave` — cuts task branches from current feat HEAD
//! - Spawns N parallel `worker_slot::run(task)` futures via `tokio::spawn`
//! - Collects via `JoinSet::join_next`; propagates failures before merge
//!
//! Max parallelism cap: 7 worker slots (Ironclaw §6).
