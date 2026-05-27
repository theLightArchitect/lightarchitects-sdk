//! Multi-voice sibling chatroom engine.
//!
//! This module provides the conversation orchestration primitives extracted
//! from `soul-chat` and adapted to the `lightarchitects` SDK:
//!
//! - **[`types`]** — core data types (`ChatMessage`, `SiblingInfo`, etc.)
//! - **[`error`]** — `ChatError` / `ChatResult`
//! - **[`formats`]** — conversation format slot definitions
//! - **[`sanitizer`]** — response sanitization before LLM output enters chat
//! - **[`sibling_provider`]** — sibling discovery from the helix spine
//! - **[`interest`]** — organic interest scoring for speaker selection
//! - **[`personality`]** — in-character response generation via `LlmAgentProvider`

pub mod error;
pub mod formats;
pub mod interest;
pub mod mode;
pub mod personality;
pub mod roster;
pub mod sanitizer;
pub mod sibling_provider;
pub mod types;

pub use error::{ChatError, ChatResult};
pub use formats::{CanonEvaluation, ConversationFormat, RubberDuck, Slot};
pub use interest::{InterestScore, InterestScorer};
pub use mode::{DOMAIN_KEYWORDS, Mode};
pub use personality::PersonalityEngine;
pub use roster::{ActiveRoster, RosterDelta};
pub use sanitizer::{DefaultSanitizer, ResponseSanitizer};
pub use sibling_provider::{SiblingProvider, StaticSiblingProvider};
pub use types::{
    ChatConfig, ChatMessage, ConversationContext, InterestFactors, MessageId, SiblingId,
    SiblingInfo, SpeakerStrategy,
};
