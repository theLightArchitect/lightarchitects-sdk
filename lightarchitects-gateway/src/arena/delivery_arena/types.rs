//! Core type definitions for the delivery arena.
//!
//! Phase 3 implementation: `TaskStatus`, `WaveStatus`, `BuildStatus`, `AgentStatus`,
//! `SharedState { tasks, builds, agent_statuses }`, `Coordinator`,
//! `ContextBudget { tier1, tier2, tier3 }`, and `can_run(task, state) -> bool`.
//!
//! Ironclaw §15: status types must be enums (no `String` representations).
//! `can_run` must be O(1) via `HashMap::get`.
