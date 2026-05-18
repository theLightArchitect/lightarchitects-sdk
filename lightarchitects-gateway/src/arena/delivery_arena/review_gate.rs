//! Blocking sequential ReviewGate — the Pillar 2 correctness moat.
//!
//! Phase 5 implementation:
//! - `ReviewGate` — runs Sonnet + Canon gate-pass check on each wave before merge
//! - `MAX_GATE_ITERATIONS: u32 = 3` hard cap (Ironclaw §7 + Cookbook §64.3)
//! - `FixAgent` integration: spawns FixAgents in the same worktree as the origin task
//! - Blocks the next wave until gate passes or cap is reached
//!
//! A gate failure after 3 iterations escalates to L4 (user HITL) via `decision_pipeline`.
