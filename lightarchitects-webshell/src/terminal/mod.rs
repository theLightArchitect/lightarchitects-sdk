//! PTY terminal bridge — Phase 2.
//!
//! Two sub-modules divide concerns cleanly:
//!
//! - [`ws`]: Axum WebSocket handler — validates the HMAC sub-protocol token,
//!   enforces the concurrent-session cap, and upgrades qualifying connections.
//! - [`session`]: Per-session PTY lifecycle — spawns the configured host
//!   command under [`portable_pty`], bridges bytes bidirectionally to the
//!   WebSocket, handles resize events, and reaps the child on close
//!   (SIGTERM → 2 s wait → SIGKILL).

pub mod session;
pub mod ws;
