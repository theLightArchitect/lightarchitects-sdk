//! lightsquad — Autonomous code-delivery orchestration engine.
//!
//! This module is the LightArchitects platform's autonomous build delivery substrate.
//! It implements the 4-layer Decision Pipeline (Canon → Northstar → LightArchitect → User),
//! the 7-slot worker pool, the ReviewGate moat with `MAX_GATE_ITERATIONS = 3`,
//! and the git-worktree-per-task wave dispatch model.
//!
//! # Architecture
//!
//! Feature-gated SDK module (per workspace convention — matches the 26-feature
//! consumption pattern used by lightarchitects-webshell). Pulls optional deps
//! on the nearai/ironclaw upstream crates (pinned at `4fea8b3546`) for the
//! gate/lease/safety primitives, then extends them with LightArchitects-specific
//! canon-grounded reasoning.
//!
//! # Phase 1 status (stub)
//!
//! Sub-modules declared with intent docs. Implementations land in Phase 3+
//! per the ironclaw-spine LASDLC plan. Reuses existing SDK modules verbatim:
//! `agent::ClaudeCliProvider` (worker spawn), `helix::HelixStore` (backend),
//! `turnlog::TurnEntry` (HMAC-chained decision ledger), `squad_registry`
//! (LightArchitect → sibling routing), `crypto` (Ed25519/HMAC),
//! `lasdlc` (type system), `platform::PlatformClient` (canon resolution).
//!
//! # Future extraction
//!
//! Tracked extraction candidate per memory entry
//! `project_lightarchitects_sdk_extraction_candidates.md` — eligible for
//! promotion to standalone workspace crate when sub-module count ≥ 10 or
//! independent-publish need emerges.

/// Wave dispatcher — fans tasks out to per-worktree workers via Tokio JoinSet.
pub mod wave_dispatcher;

/// Merge agent — serializes all git2 ops via `Arc<Mutex<()>>` (Phase 3).
pub mod merge_agent;

/// Review gate — extends nearai `ironclaw_engine::gate::ExecutionGate` with
/// canon/northstar/domain checks; enforces `MAX_GATE_ITERATIONS = 3`.
pub mod review_gate;

/// 4-layer decision pipeline — Canon → Northstar → LightArchitect → User.
pub mod decision_pipeline;

/// 7-step LASDLC preflight checklist (freeze/validate/repo/disk/api/canon/dry-run).
pub mod preflight;

/// Supervisor — `ironclaw-hitl` channel monitoring, decision-log HMAC chaining via turnlog.
pub mod supervisor;

/// Worker spawn — wraps `crate::agent::ClaudeCliProvider` for autonomous worker pool.
pub mod worker_spawn;

/// LightArchitects — 10 gate-dimension specialists ([A+S+Q+C+O+P+K+D+T+R])
/// routed to existing siblings via `crate::squad_registry`.
pub mod light_architects;

/// Program.toml integrity lock — SHA-256 + Ed25519 signing.
pub mod manifest;

/// Per-wave HKDF subkey derivation from a build master key.
pub mod hmac;

/// Decision log subsystem — HMAC-chained NDJSON gate decisions.
pub mod decisions;

/// PAUSE/drain/resume state machine + atomic-write helper.
pub mod pause;
