//! Squad Comms — coordination panel HTTP layer for the webshell.
//!
//! This module wraps two existing platform primitives:
//!
//! - **Conductor task queue** (`~/.lightarchitects/tasks/queue.json`) —
//!   file-backed task queue managed by the `lightarchitects-gateway`
//!   conductor subcommand. We expose read/add/claim semantics over HTTP.
//! - **Soul-chat sessions** (`~/lightarchitects/soul/helix/chat/sessions/*.json`) —
//!   multi-sibling conversation registry written by `soul chat`. We expose
//!   list/inject/stream semantics over HTTP.
//!
//! Together these provide a single pane of glass for two parallel agents
//! coordinating asynchronously through a build queue and a chat thread.
//!
//! # Architectural rule (build `bridging-whistling-loom`)
//!
//! Business logic for the queue and chat sessions lives in the **private**
//! crates (`lightarchitects-gateway` and `SOUL-DEV/soul-chat`). This module is
//! a **thin HTTP wrapper** that reads the on-disk artifacts directly (queue
//! JSON, session JSONs) and shells out to the `soul` CLI for the chat-inject
//! side-effect. It contains no business rules of its own.
//!
//! # Endpoints
//!
//! - `GET  /api/coordination/tasks` — list queue snapshot + counts
//! - `POST /api/coordination/tasks/add` — append a task to the queue
//! - `POST /api/coordination/tasks/claim/:id` — soft-claim (annotates source)
//! - `GET  /api/coordination/tasks/:id/logs` — last 200 lines of task log
//! - `POST /api/coordination/sessions/start` — materialise a per-build soul-chat session
//! - `POST /api/coordination/sessions/end` — mark a session stopped
//! - `GET  /api/coordination/chat/sessions` — list known chat sessions
//! - `POST /api/coordination/chat/inject` — inject a message into a session
//! - `GET  /api/coordination/chat/stream?session_id=...` — SSE chat stream
//! - `POST /api/coordination/tasks/spawn-worker` — spawn the conductor daemon or one-shot worker
//!
//! All endpoints require `Authorization: Bearer <token>` against the
//! webshell's HMAC token, same as `/api/events`.

pub mod build_hooks;
pub mod handlers;
pub mod sse;
pub mod types;

pub use handlers::{
    add_task, chat_inject, chat_sessions, claim_task, complete_dispatch, enqueue_dispatch,
    list_tasks, session_end, session_start, spawn_worker, task_logs,
};
pub use sse::chat_stream;
