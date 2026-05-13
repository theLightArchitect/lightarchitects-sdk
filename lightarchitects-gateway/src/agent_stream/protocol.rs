//! Wire protocol for agent streaming — mirrors `lightarchitects-webshell/src/agent/protocol.rs`.
//!
//! Kept in-sync manually; both sides must agree on variant names and fields
//! so NDJSON parsing is zero-config.

use serde::{Deserialize, Serialize};

// ── Events (agent → browser / stdout) ────────────────────────────────────────

/// One event emitted by the agent runner during a turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum AgentEvent {
    /// Streaming text chunk from the LLM.
    Text { chunk: String },

    /// Thinking / reasoning content from the LLM.
    Thinking { content: String },

    /// A tool call has been parsed and is about to execute.
    ToolStart {
        name: String,
        id: String,
        input: serde_json::Value,
    },

    /// A tool call finished executing.
    ToolComplete {
        id: String,
        success: bool,
        duration_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
    },

    /// The agent turn completed successfully.
    Complete { reason: TerminationReason },

    /// A non-fatal error occurred during the turn.
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        recoverable: Option<bool>,
    },

    /// Token usage for the turn.
    TokenUsage { input: u64, output: u64 },

    /// Status bar update.
    StatusUpdate { text: String },

    /// Heartbeat / keepalive.
    Heartbeat,
}

impl AgentEvent {
    /// SSE `event:` name for this variant.
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
        }
    }
}

/// Why the agent loop terminated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[allow(missing_docs)]
pub enum TerminationReason {
    /// Normal completion.
    Complete,
    /// Iteration cap reached.
    MaxIterations,
    /// Token budget exhausted.
    TokenBudgetExhausted,
    /// User cancelled.
    UserCancelled,
    /// Wall-clock timeout exceeded.
    Timeout,
    /// Unrecoverable error during execution.
    Error { message: String },
}

// ── Control messages (browser / stdin → agent) ──────────────────────────────

/// Control message sent by the browser over the agent WebSocket.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum ControlMessage {
    /// Start a new turn with the given user message.
    SendMessage { text: String },
    /// Approve a pending permission request.
    ApprovePermission { request_id: String },
    /// Deny a pending permission request.
    DenyPermission {
        request_id: String,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Interrupt / cancel the current in-flight turn.
    Interrupt,
    /// Steer the agent mid-turn.
    Steer { text: String },
    /// Override the system prompt for subsequent turns (must be sent before `SendMessage`).
    /// `text` is capped at 8 KiB and must not contain NUL bytes; the runner validates both.
    SetSystemPrompt { text: String },
    /// Execute queued plan actions.
    ExecutePlan,
    /// Ping / keepalive.
    Ping,
}
