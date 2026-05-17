//! Local web GUI shell for the active coding agent.
//!
//! Hosts an active coding agent session (Claude Code by default, configurable
//! via `--host-cmd`) inside a browser via an embedded PTY terminal alongside
//! a 3D session-helix panel. The PTY terminal provides 1:1 parity with the
//! native agent by construction — raw bytes are piped through the browser.
//!
//! Phase 1 scaffolds the HTTP server, HMAC auth surface, and rust-embed
//! static asset wiring. Subsequent phases add the PTY bridge (Phase 2),
//! AYIN SSE subscription (Phase 3), filesystem watcher (Phase 4), SSE fan-out
//! to the browser (Phase 5), and the React frontend with the 3D helix scene
//! plus xterm.js terminal (Phases 6-8).
//!
//! Design reference:
//! `~/lightarchitects/soul/helix/corso/builds/luminous-weaving-nautilus/plan.md`.
//!
//! # Local-dev-only boundary
//!
//! The default host command is Anthropic's closed-source `claude` CLI. This
//! crate is intended for local developer use only — public distribution
//! requires swapping the host to an Agent-SDK-built agent or lÆx0 via
//! `--host-cmd` and is gated by the separate licensing review in Phase 9.

pub mod agent;
pub mod auth;
pub mod config;
pub mod container;
/// Squad Comms — HTTP wrapper over the conductor task queue and soul-chat sessions.
pub mod coordination;
pub mod copilot;
/// CSP middleware + violation report endpoint (SEC-3a/3b).
pub mod csp;
/// Squad Dispatch — heuristic classifier + in-process agent orchestration.
pub mod dispatch;
pub mod events;
pub mod init;
pub mod mcp_config;
pub mod memory;
pub mod polytope_data;
pub mod real_data;
pub mod server;
pub mod session;
pub mod session_cwd;
pub mod session_fork;
pub mod session_store;
pub mod setup;
pub mod static_assets;
pub mod supervisor;
pub mod terminal;
pub mod turnlog;
pub mod version;
