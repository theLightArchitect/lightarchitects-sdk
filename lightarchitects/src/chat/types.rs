//! Core types for the chat conversation engine.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a sibling participant.
pub type SiblingId = String;

/// Unique identifier for a chat message.
pub type MessageId = String;

// ---------------------------------------------------------------------------
// Chat Message
// ---------------------------------------------------------------------------

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Unique message identifier.
    pub id: MessageId,
    /// Which sibling spoke (or "kevin" for injected messages).
    pub speaker: SiblingId,
    /// The message content (raw text).
    pub content: String,
    /// When this message was generated.
    pub timestamp: DateTime<Utc>,
    /// TTS-formatted version of the content (with audio tags).
    pub tts_formatted: Option<String>,
    /// Significance score (0.0-10.0) if evaluated.
    pub significance: Option<f32>,
}

impl ChatMessage {
    /// Create a new message from a speaker.
    #[must_use]
    pub fn new(speaker: SiblingId, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            speaker,
            content,
            timestamp: Utc::now(),
            tts_formatted: None,
            significance: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Speaker Strategy
// ---------------------------------------------------------------------------

/// How the orchestrator selects the next speaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerStrategy {
    /// LLM-based selection — send context to neural-engine, parse suggested speaker.
    Auto,
    /// Cycle through participants in order.
    RoundRobin,
    /// Match current topic keywords to sibling strands/expertise.
    ContextDriven,
    /// Weighted random (heavier weight for siblings who haven't spoken recently).
    Random,
    /// Organic interest scoring — 4 reactive factors (stake, stimulus, novelty,
    /// urgency), no cooldowns/penalties, novelty depletion curve, squared
    /// weighted random selection.
    OrganicInterest,
}

impl Default for SpeakerStrategy {
    fn default() -> Self {
        Self::Auto
    }
}

// ---------------------------------------------------------------------------
// Interest Factors (organic model transparency)
// ---------------------------------------------------------------------------

/// Per-sibling interest scoring breakdown for organic speaker selection.
///
/// Weights (Kevin-confirmed, updated 2026-03-31): Stake 0.35, Stimulus 0.25,
/// Urgency 0.25, Novelty 0.15. Urgency outweighs novelty because if an agent
/// is directly asked something, they speak; an agent with nothing new can stay
/// quiet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterestFactors {
    /// How much this topic affects them personally (0.0-1.0).
    pub stake: f32,
    /// How much the last turn specifically stimulated them (0.0-1.0).
    pub stimulus: f32,
    /// Do they have something genuinely new to add (0.0-1.0).
    /// Depletes after speaking, rebuilds when others add new threads.
    pub novelty: f32,
    /// Is something unresolved directed at them (0.0-1.0).
    pub urgency: f32,
    /// Weighted sum: stake*0.35 + stimulus*0.25 + urgency*0.25 + novelty*0.15.
    pub total: f32,
}

// ---------------------------------------------------------------------------
// Chat Config
// ---------------------------------------------------------------------------

/// Configuration for a chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Speaker selection strategy.
    #[serde(default)]
    pub strategy: SpeakerStrategy,
    /// Maximum turns before auto-stop (0 = unlimited).
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// Optional topic seed to start the conversation.
    pub topic_seed: Option<String>,
    /// Optional filter to restrict which siblings participate.
    pub participant_filter: Option<Vec<SiblingId>>,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            strategy: SpeakerStrategy::default(),
            max_turns: 50,
            topic_seed: None,
            participant_filter: None,
        }
    }
}

fn default_max_turns() -> u32 {
    50
}

// ---------------------------------------------------------------------------
// Conversation Context
// ---------------------------------------------------------------------------

/// Sliding window of conversation state passed to strategy engines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationContext {
    /// Recent messages (sliding window, typically last 20).
    pub messages: Vec<ChatMessage>,
    /// Current conversation topic (extracted from recent messages).
    pub current_topic: Option<String>,
    /// Detected emotional state of the conversation.
    pub emotional_state: Option<String>,
    /// Active participants in this session.
    pub participants: Vec<SiblingId>,
    /// W3C traceparent span ID for distributed tracing (forwarded to `AgentRequest`).
    pub span_id: Option<String>,
}

impl ConversationContext {
    /// Create an empty context for a new session.
    #[must_use]
    pub fn new(participants: Vec<SiblingId>) -> Self {
        Self {
            messages: Vec::new(),
            current_topic: None,
            emotional_state: None,
            participants,
            span_id: None,
        }
    }

    /// Add a message to the context, maintaining the sliding window.
    pub fn push_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
        // Keep last 20 messages as context window
        if self.messages.len() > 20 {
            self.messages.drain(..self.messages.len() - 20);
        }
    }
}

// ---------------------------------------------------------------------------
// Sibling Info
// ---------------------------------------------------------------------------

/// Metadata about a discovered sibling, loaded from identity.md.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiblingInfo {
    /// Sibling name (lowercase: "eva", "corso", "claude", "quantum", "seraph").
    pub name: SiblingId,
    /// Role description from identity.md.
    pub role: Option<String>,
    /// Active strands (personality dimensions).
    pub strands: Vec<String>,
    /// Path to the sibling's identity.md file.
    pub identity_path: String,
    /// `ElevenLabs` voice ID for this sibling, if configured.
    pub voice: Option<String>,
}
