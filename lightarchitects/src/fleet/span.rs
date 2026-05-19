//! Fleet span types — `FleetSpan`, `FleetStatus`, `ExitPath`.
//!
//! Gate 1 OQ decisions (locked):
//! - `parent_agent_id`: inferred by `FleetTracker` from active-agent context stack.
//! - `worktree_path`: always `None` in V1 (`isolation: "worktree"` is a string tag,
//!   not a resolved path — OQ2 resolved).
//! - `turns`: always `0` while running; always `0` at completion in V1
//!   (not reliably countable from parent JSONL — SCR1-F2 resolved as OQ4).
//! - `elapsed_ms`: timer-driven (`FleetBroadcaster` ticker), updated every 500 ms.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Maximum allowed description length (bytes, UTF-8 truncation by char boundary).
const MAX_DESCRIPTION_LEN: usize = 200;

/// Snapshot of a single agent execution visible to the fleet dashboard.
///
/// Instances are created via [`FleetSpan::new`] and mutated exclusively by
/// [`crate::fleet::tracker::FleetTracker`].  All fields are `pub` for
/// serialisation; callers must NOT mutate them directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetSpan {
    /// Stable unique identifier for this agent invocation (`tool_use_id` from JSONL).
    pub agent_id: String,
    /// `subagent_type` from the Agent tool call (e.g. `"engineer"`, `"quality"`).
    pub agent_type: String,
    /// Human-readable task description — sanitized at construction.
    pub description: String,
    /// Parent agent that spawned this one, inferred from the active-stack.
    pub parent_agent_id: Option<String>,
    /// Resolved worktree path — always `None` in V1 (OQ2).
    pub worktree_path: Option<String>,
    /// Whether the agent was launched with `run_in_background: true`.
    pub run_in_background: bool,
    /// Current lifecycle state.
    pub status: FleetStatus,
    /// Turn count — always `0` in V1 (SCR1-F2 / OQ4).
    pub turns: u64,
    /// Elapsed wall-clock milliseconds; updated externally every 500 ms.
    pub elapsed_ms: u64,
    /// How the agent exited — `None` while still running.
    pub exit_path: Option<ExitPath>,
    /// When the agent was spawned (UTC).
    pub spawned_at: DateTime<Utc>,
    /// When the agent completed — `None` while still running.
    pub completed_at: Option<DateTime<Utc>>,
}

impl FleetSpan {
    /// Create a new `FleetSpan` in the [`FleetStatus::Running`] state.
    ///
    /// `description` is sanitized: truncated to ≤ 200 chars (UTF-8 boundary-safe),
    /// and all `\n`, `\r`, `\0` bytes stripped before truncation.
    #[must_use]
    pub fn new(
        agent_id: String,
        agent_type: String,
        description: String,
        parent_agent_id: Option<String>,
        run_in_background: bool,
    ) -> Self {
        Self {
            agent_id,
            agent_type,
            description: sanitize_description(description),
            parent_agent_id,
            worktree_path: None, // V1: always None (OQ2)
            run_in_background,
            status: FleetStatus::Running,
            turns: 0, // V1: always 0 (OQ4 / SCR1-F2)
            elapsed_ms: 0,
            exit_path: None,
            spawned_at: Utc::now(),
            completed_at: None,
        }
    }
}

/// Sanitize a free-text description for dashboard display.
///
/// Strips control characters (`\n`, `\r`, `\0`) then truncates to
/// [`MAX_DESCRIPTION_LEN`] bytes at a valid UTF-8 character boundary.
fn sanitize_description(raw: String) -> String {
    let stripped: String = raw
        .chars()
        .filter(|c| !matches!(c, '\n' | '\r' | '\0'))
        .collect();

    // Truncate at a UTF-8 char boundary, not a byte offset.
    if stripped.len() <= MAX_DESCRIPTION_LEN {
        return stripped;
    }
    let mut end = MAX_DESCRIPTION_LEN;
    while !stripped.is_char_boundary(end) {
        end -= 1;
    }
    stripped[..end].to_owned()
}

/// Lifecycle state of a fleet agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetStatus {
    /// Accepted but not yet started (reserved for future queueing support).
    Queued,
    /// Currently executing.
    Running,
    /// Finished successfully.
    Completed,
    /// Finished with an error.
    Failed,
    /// Watchdog detected stall (no progress within threshold).
    Stalled,
}

/// How an agent exited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitPath {
    /// Normal completion.
    Completed,
    /// Terminated due to error.
    Error,
    /// Terminated by watchdog stall detection.
    WatchdogStall,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_control_chars() {
        let desc = "line1\nline2\r\0end".to_owned();
        let span = FleetSpan::new("id".into(), "engineer".into(), desc, None, false);
        assert!(!span.description.contains('\n'));
        assert!(!span.description.contains('\r'));
        assert!(!span.description.contains('\0'));
        assert_eq!(span.description, "line1line2end");
    }

    #[test]
    fn sanitize_truncates_at_char_boundary() {
        // Create a string > 200 chars using ASCII (easy boundary check).
        let long = "a".repeat(300);
        let span = FleetSpan::new("id".into(), "t".into(), long, None, false);
        assert!(span.description.len() <= MAX_DESCRIPTION_LEN);
    }

    #[test]
    fn sanitize_truncates_multibyte_at_boundary() {
        // Each '£' is 2 bytes in UTF-8; 100 × '£' = 200 bytes — fits exactly.
        let exactly_200: String = "£".repeat(100);
        assert_eq!(exactly_200.len(), 200);
        let span = FleetSpan::new("id".into(), "t".into(), exactly_200.clone(), None, false);
        assert_eq!(span.description, exactly_200);

        // 101 × '£' = 202 bytes — must truncate to 100 chars (200 bytes).
        let over: String = "£".repeat(101);
        let span2 = FleetSpan::new("id".into(), "t".into(), over, None, false);
        assert!(span2.description.len() <= MAX_DESCRIPTION_LEN);
        assert!(span2.description.is_empty() || span2.description.chars().all(|c| c == '£'));
    }

    #[test]
    fn new_span_has_expected_defaults() {
        let span = FleetSpan::new("a1".into(), "eng".into(), "task".into(), None, true);
        assert_eq!(span.status, FleetStatus::Running);
        assert_eq!(span.turns, 0);
        assert_eq!(span.elapsed_ms, 0);
        assert!(span.exit_path.is_none());
        assert!(span.completed_at.is_none());
        assert!(span.worktree_path.is_none());
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn fleet_status_serde_roundtrip() {
        let json = serde_json::to_string(&FleetStatus::Completed).expect("serialize");
        assert_eq!(json, r#""completed""#);
        let back: FleetStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, FleetStatus::Completed);
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn exit_path_serde_roundtrip() {
        let json = serde_json::to_string(&ExitPath::WatchdogStall).expect("serialize");
        assert_eq!(json, r#""watchdog_stall""#);
        let back: ExitPath = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ExitPath::WatchdogStall);
    }
}
