//! Wire protocol types for conversation streaming.
//!
//! [`ConversationEvent`] is the SDK-native counterpart of the gateway's
//! `AgentEvent` — emitted outward through a [`Transport`] during a session turn.
//!
//! [`Transport`]: super::transport::Transport

use serde::{Deserialize, Serialize};

// ── TerminationReason ─────────────────────────────────────────────────────────

/// Why a [`ConversationSession`] turn terminated.
///
/// [`ConversationSession`]: super::session::ConversationSession
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TerminationReason {
    /// Turn completed normally.
    Complete,
    /// Per-session turn cap reached.
    MaxTurns,
    /// Token budget exhausted.
    TokenBudgetExhausted,
    /// Operator interrupted mid-turn.
    UserCancelled,
    /// Wall-clock timeout exceeded.
    Timeout,
    /// Provider returned an unrecoverable error.
    Error {
        /// Error detail forwarded from the provider.
        message: String,
    },
}

// ── ConversationEvent ─────────────────────────────────────────────────────────

/// One event emitted by the session runner during a turn.
///
/// Serialises as `{"type":"<snake_case_variant_name>", …}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationEvent {
    /// Streaming text chunk from the LLM.
    Text {
        /// Partial text content.
        chunk: String,
    },

    /// Reasoning / thinking content from the LLM.
    Thinking {
        /// Thought content (chain-of-thought or extended thinking).
        content: String,
    },

    /// A tool call has been parsed and is about to execute.
    ToolStart {
        /// Tool name.
        name: String,
        /// Opaque call identifier, echoed in [`ToolComplete`].
        ///
        /// [`ToolComplete`]: ConversationEvent::ToolComplete
        id: String,
        /// Serialised input arguments.
        input: serde_json::Value,
    },

    /// A tool call finished executing.
    ToolComplete {
        /// Call identifier matching [`ToolStart`].
        ///
        /// [`ToolStart`]: ConversationEvent::ToolStart
        id: String,
        /// `true` when the tool returned without error.
        success: bool,
        /// Wall-clock execution time in milliseconds.
        duration_ms: u64,
        /// Tool output, if any.
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
    },

    /// The turn completed.
    Complete {
        /// Reason for completion.
        reason: TerminationReason,
    },

    /// A non-fatal error occurred.
    Error {
        /// Human-readable error message.
        message: String,
        /// `true` when the session may continue after this error.
        #[serde(skip_serializing_if = "Option::is_none")]
        recoverable: Option<bool>,
    },

    /// Token usage for the completed turn.
    TokenUsage {
        /// Approximate input token count.
        input: u64,
        /// Approximate output token count.
        output: u64,
    },

    /// Status bar update (progress indicator).
    StatusUpdate {
        /// Human-readable status text.
        text: String,
    },

    /// Keepalive / heartbeat.
    Heartbeat,

    /// An indirect prompt injection pattern was detected in a tool result.
    ///
    /// Emitted by [`IndirectInjectionShield`] before the result is re-injected.
    /// The operator should review and decide whether to proceed.
    ///
    /// [`IndirectInjectionShield`]: crate::agent::indirect_injection_shield::IndirectInjectionShield
    IndirectInjectionWarning {
        /// The tool call whose result triggered the warning.
        tool_use_id: String,
        /// The matched injection pattern.
        pattern: String,
        /// Severity of the detected pattern.
        severity: crate::agent::indirect_injection_shield::InjectionSeverity,
    },

    /// Webshell-render metadata stub — wired in Phase 4 (SSE + P1 gate).
    WebshellRender {
        /// View identifier for the target panel.
        view_id: String,
        /// Render payload (format TBD in Phase 4).
        payload: serde_json::Value,
    },
}

impl ConversationEvent {
    /// SSE `event:` field name for this variant.
    #[must_use]
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::Text { .. } => "text",
            Self::Thinking { .. } => "thinking",
            Self::ToolStart { .. } => "tool_start",
            Self::ToolComplete { .. } => "tool_complete",
            Self::Complete { .. } => "complete",
            Self::Error { .. } => "error",
            Self::TokenUsage { .. } => "token_usage",
            Self::StatusUpdate { .. } => "status_update",
            Self::Heartbeat => "heartbeat",
            Self::IndirectInjectionWarning { .. } => "indirect_injection_warning",
            Self::WebshellRender { .. } => "webshell_render",
        }
    }
}
