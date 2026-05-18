//! Delivery arena — autonomous build delivery engine (Pillar 2 spine).
//!
//! Phase 1: module skeleton only. Implementations land in Phases 3–5.
//!
//! # Sub-modules
//!
//! - [`types`] — Core type definitions (TaskStatus, WaveStatus, SharedState, Coordinator)
//! - [`merge_agent`] — Serialised git operations behind `Arc<Mutex<()>>`
//! - [`wave_dispatcher`] — Parallel task dispatch (fan-out) and fan-in via `JoinSet`
//! - [`worker_slot`] — Single AgentRunner worker lifecycle (spawn + await + cleanup)
//! - [`worktree_manager`] — Git worktree CRUD (shared surface with gitforest §2.10c)
//! - [`decision_pipeline`] — 4-layer Canon→Northstar→LightArchitect→User resolution
//! - [`review_gate`] — Blocking sequential gate with `MAX_GATE_ITERATIONS=3` hard cap
//! - [`program`] — `program.toml` schema (serde types, Phase 4 SHA256 lock)

pub mod decision_pipeline;
pub mod merge_agent;
pub mod program;
pub mod review_gate;
pub mod types;
pub mod wave_dispatcher;
pub mod worker_slot;
pub mod worktree_manager;
