//! ReviewGate — the moat.
//!
//! Per canonical IRONCLAW PDF spec (ReviewGate §):
//! > "Runs after every wave, after every build, at program completion.
//! > Blocking and sequential. No merge happens without a passing verdict.
//! > Weakening the gate is Northstar-violating."
//!
//! Phase 5 implementation:
//! - Implements nearai `ironclaw_engine::gate::ExecutionGate` trait (upstream)
//! - Extends with canon-check, northstar-check, domain-failure-classification
//! - `GateVerdict { passed: bool, score: f32, domain_failures: Vec<String>, required_fixes: Vec<String> }`
//! - `const MAX_GATE_ITERATIONS: u8 = 3` — hard cap, never infinite
//! - Fail-closed by construction — `GateDecision` has no `None` variant
//!   (matches nearai/ironclaw `crates/ironclaw_engine/src/gate/mod.rs:8` invariant)
//! - Canon docs as ReviewGate system prompt (cached at ~10% per-call cost)
//! - FixAgents receive `domain_failures` + `required_fixes`, work in same worktrees
//!
//! Phase 1 stub — types declared in Phase 5.
