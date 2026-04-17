//! Training `CanonicalTurn` projection — Week 2 scope.
//!
//! Replaces the 3-source cursor merge in the existing lÆx0 training exporter.
//! A session's log entries fold into a `Vec<CanonicalTurn>` by grouping
//! children of each `TurnStart` parent_seq.

use serde::{Deserialize, Serialize};

/// One complete exchange — user input, assistant response, tool calls —
/// ready for training export.
///
/// Shape follows the existing `src/training/canonical.rs` `CanonicalTurn`
/// in lÆx0 so the downstream exporters (ChatML, ShareGPT, DeepSeek-R1, …)
/// require no changes once this projection replaces the cursor merge.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CanonicalTurnProjection {
    /// Session UUID.
    pub session_id: String,
    /// Turn index within the session (0-indexed).
    pub turn_id: u32,
    /// Unix milliseconds of the turn start.
    pub ts_ms: i64,
    /// User input.
    pub user: Option<String>,
    /// Assistant response (main content).
    pub assistant: Option<String>,
    /// Extended thinking if captured.
    pub thinking: Option<String>,
    /// Tool calls emitted this turn.
    pub tool_calls: Vec<ToolCallProjection>,
    /// Model identifier for this turn.
    pub model: Option<String>,
    /// Provider name.
    pub provider: Option<String>,
    /// Prompt tokens.
    pub input_tokens: Option<u64>,
    /// Completion tokens.
    pub output_tokens: Option<u64>,
    /// End-to-end turn duration.
    pub duration_ms: Option<u64>,
    /// Weight from the matching `Reflection` entry, if any.
    pub weight: Option<f64>,
    /// Whether `SecurityEvent` entries exist for this turn.
    pub has_security_events: bool,
}

/// One tool call within a [`CanonicalTurnProjection`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolCallProjection {
    /// Tool call ID.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Full input JSON (not preview-truncated).
    pub input: serde_json::Value,
    /// Tool output text.
    pub output: Option<String>,
    /// Whether the tool succeeded.
    pub success: Option<bool>,
    /// Wall-clock tool duration.
    pub duration_ms: Option<u64>,
    /// Cognitive phase at time of call.
    pub cognitive_phase: Option<String>,
}
