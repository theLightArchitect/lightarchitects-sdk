//! Squad Dispatch — parallel domain-agent orchestration for the webshell.
//!
//! This module wires a heuristic-only classifier (zero LLM cost, ≤5 ms p99)
//! to an in-process TeamManager from `lightarchitects-cli`, broadcasting
//! [`types::DispatchEvent`] over Server-Sent Events to the Svelte frontend.
//!
//! # Architecture
//!
//! ```text
//! POST /api/dispatch/classify   →  classifier::classify()
//! POST /api/dispatch/execute    →  executor::execute()  →  TeamManager
//! GET  /api/dispatch/status/:id →  SSE stream of DispatchEvent
//! POST /api/dispatch/cancel/:id →  executor::cancel()
//! POST /api/dispatch/retry/:id/:agent → executor::retry()
//! ```
//!
//! All routes require `Authorization: Bearer <token>` (HIGH H-5).
//!
//! # Security constraints
//!
//! - Classifier uses literal substring / aho-corasick only — no `regex` over
//!   user input (HIGH H-8, avoids ReDoS).
//! - Task input capped at 8 KB, control characters stripped (HIGH H-2).
//! - Per-IP rate limit on `/api/dispatch/classify`: ≤10 req/s (HIGH H-8).
//! - `DomainAgent::Security` spawns always bind a synthesised
//!   `EngagementScope` — rejected with 403 if scope cannot be established
//!   (HIGH H-7).
//! - DRY-RUN enforced at spawn time via a read-only `ToolPermissionToken`
//!   (HIGH H-9).

pub mod classifier;
pub mod executor;
pub mod routes;
pub mod state;
pub mod types;

pub use routes::dispatch_router;
pub use state::DispatchRegistry;
pub use types::{DispatchError, DispatchEvent, DispatchId, DomainAgent, ExecutionMode};
