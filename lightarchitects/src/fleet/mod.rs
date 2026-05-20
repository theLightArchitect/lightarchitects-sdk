//! Fleet module — live agent execution tracking for the webshell dashboard.
//!
//! # Overview
//!
//! The fleet module provides a lightweight, in-process view of all Claude Code
//! agent invocations within the current session.  It consumes the session JSONL
//! file produced by Claude Code and maintains a live state machine of agent
//! spans, which the webshell broadcasts to connected dashboard clients via SSE.
//!
//! # Components
//!
//! | Component | Description |
//! |-----------|-------------|
//! | [`FleetSpan`] | Immutable agent lifecycle record (Gate 1 OQ decisions baked in) |
//! | [`FleetStatus`] | Lifecycle state machine (`Running → Completed / Failed / Stalled`) |
//! | [`ExitPath`] | How an agent exited (`Completed`, `Error`, `WatchdogStall`) |
//! | [`FleetTracker`] | `Arc`-wrapped DashMap state machine; cheap to clone |
//! | [`FleetNode`] | Serialisable SSE payload derived from `FleetSpan` |
//! | [`FleetSnapshot`] | Point-in-time snapshot of all nodes |
//! | [`ClaudeJsonlTailer`] | JSONL file tailer — feeds events into `FleetTracker` |
//! | [`FleetError`] | Error type (404 / 422 / 500 HTTP semantics) |
//!
//! # Gate 1 OQ decisions (locked)
//!
//! - `worktree_path`: always `None` in V1 (OQ2 resolved).
//! - `turns`: always `0` in V1 (SCR1-F2 / OQ4 resolved).
//! - `parent_agent_id`: inferred from `FleetTracker` active-stack (OQ1 resolved).
//! - No `deny_unknown_fields` on `AgentToolInput` (SCR1-F1 — forward compat).
//!
//! # Security
//!
//! [`ClaudeJsonlTailer`] reads only an explicit allowlist of fields from the
//! session JSONL — the `prompt` field and all undeclared fields are silently
//! discarded.  The JSONL path is validated to be a descendant of
//! `$HOME/.claude/projects/` before being opened.

pub mod error;
pub mod jsonl;
pub mod span;
pub mod tracker;

// ── Public re-exports ─────────────────────────────────────────────────────────

pub use error::FleetError;
pub use jsonl::{ClaudeJsonlTailer, find_jsonl_for_session};
pub use span::{ExitPath, FleetSpan, FleetStatus};
pub use tracker::{FleetNode, FleetSnapshot, FleetTracker};
