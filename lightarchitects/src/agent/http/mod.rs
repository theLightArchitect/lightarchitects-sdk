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
//! L0: Providers      ← AnthropicHttpProvider, GoogleAiStudioProvider,
//!                      VertexAiGeminiProvider (stub), VertexAiClaudeProvider (stub)
//!                    ← this module
//! ```
//!
//! # Security
//!
//! Implemented providers use Keychain-only key resolution in release builds.
//! See [`auth`] for the enforcement policy (SERAPH OA-12 audit item a + e).
//! Vertex AI stubs (which will use OAuth2 ADC, not API key) do not yet
//! dispatch — see [`vertex_ai`] for the planned wire shapes and contract
//! references.

mod anthropic;
mod auth;
mod google_ai_studio;
mod vertex_ai;

pub use anthropic::AnthropicHttpProvider;
pub use auth::{resolve_anthropic_key, resolve_google_ai_studio_key};
pub use google_ai_studio::GoogleAiStudioProvider;
pub use vertex_ai::{VERTEX_ANTHROPIC_VERSION, VertexAiClaudeProvider, VertexAiGeminiProvider};
