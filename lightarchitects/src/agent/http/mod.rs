//! L0 HTTP providers — direct API callers without subprocess delegation.
//!
//! Enabled by the `loops-core` feature alongside the L1/L2/L3 substrate.
//!
//! # Layer placement
//!
//! ```text
//! L3: Orchestration  ← WorkerPool / Supervisor
//! L2: Conversation   ← ConversationSession, Transport, Hooks
//! L1: Loops          ← Strategy, LoopRunner, EnsembleStrategy
//! L0: Providers      ← AnthropicHttpProvider, VertexHttpProvider  ← this module
//! ```
//!
//! # Security
//!
//! Both providers use Keychain-only key resolution in release builds.
//! See [`auth`] for the enforcement policy (SERAPH OA-12 audit item a + e).

mod anthropic;
mod auth;
mod vertex;

pub use anthropic::AnthropicHttpProvider;
pub use auth::{resolve_anthropic_key, resolve_vertex_key};
pub use vertex::VertexHttpProvider;
