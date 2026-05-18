//! `program.toml` schema — serde types for autonomous build programs.
//!
//! Phase 3 implementation: `serde::Deserialize` types only (no runtime logic).
//! Phase 4 adds SHA256 lock: `manifest_id` field + lock verification before each
//! task dispatch (LDB §D5 + §SG-CRYPTO.3 canon amendment).
//!
//! A `program.toml` describes a complete autonomous build: phases, waves, tasks,
//! per-task `ContextBudget { tier1, tier2, tier3 }`, and dependency graph.
