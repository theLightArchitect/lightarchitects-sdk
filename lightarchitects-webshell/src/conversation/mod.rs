//! Standalone multi-turn conversation API — operates without a [`BuildSession`].
//!
//! Exposes five HTTP endpoints under `/api/conversation`:
//! - `POST /api/conversation` — create session, returns `{ session_id }`
//! - `GET  /api/conversation/{id}/stream` — SSE event stream
//! - `POST /api/conversation/{id}` — send message (strategy prefix or native)
//! - `POST /api/conversation/{id}/interrupt` — abort current turn
//! - `DELETE /api/conversation/{id}` — end session
//!
//! See `arch/lightspace-conversation-api/` for C2/C3/sequence/state-machine diagrams.

pub mod routes;
pub mod session;
pub mod strategy_bridge;

pub use session::{ConvSessionHandle, ConvSessionStore};
