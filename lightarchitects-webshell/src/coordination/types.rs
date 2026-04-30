//! Wire types for the Squad Comms coordination endpoints.
//!
//! These mirror the on-disk JSON shapes of the conductor queue and the
//! soul-chat session registry, but flatten them to UI-friendly forms with
//! string-typed enums (`pending`, `in_progress`, ...) so the React/Svelte
//! client doesn't have to know Rust enum tag conventions.

use serde::{Deserialize, Serialize};

/// Default poll interval for the chat SSE stream, in seconds.
///
/// Build `bridging-whistling-loom` quality gate: SSE polls **must not** run
/// faster than 1 Hz. We pick 2 s for a comfortable margin, matching the
/// "≤30 s task-list refresh" spec in the plan.
pub const CHAT_POLL_INTERVAL_SECS: u64 = 2;

/// Snapshot of the task queue + aggregate counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskQueueResponse {
    /// All tasks in the queue, regardless of status.
    pub tasks: Vec<TaskSummary>,
    /// Number of pending tasks.
    pub pending_count: usize,
    /// Number of in-progress tasks.
    pub in_progress_count: usize,
    /// Number of completed tasks.
    pub completed_count: usize,
    /// Number of failed tasks.
    pub failed_count: usize,
    /// Whether the conductor daemon process is running (best-effort).
    pub daemon_running: bool,
}

/// Single task summary — a flattened view of `conductor::queue::Task`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    /// Unique task identifier (e.g. `manual-20260429-170935`).
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Project path relative to `~/Projects/`.
    pub project: String,
    /// Truncated prompt body (first 240 chars). Full prompt is fetchable via
    /// `/api/coordination/tasks/:id/logs` once execution starts.
    pub prompt_excerpt: String,
    /// Lifecycle status: `pending`, `in_progress`, `completed`, `failed`,
    /// `timeout`, or `skipped`.
    pub status: String,
    /// Origin label (e.g. `manual`, `github`, `discovery`).
    pub source: String,
    /// Priority label: `high`, `medium`, or `low`.
    pub priority: String,
    /// ISO-8601 UTC timestamp the task was added, if recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<String>,
    /// ISO-8601 UTC timestamp execution started, if recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    /// ISO-8601 UTC timestamp execution finished, if recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<String>,
}

/// Request body for `POST /api/coordination/tasks/add`.
#[derive(Debug, Clone, Deserialize)]
pub struct AddTaskRequest {
    /// Human-readable title (required, max 200 chars).
    pub title: String,
    /// Project path (required).
    pub project: String,
    /// Prompt for the agent (required, max 4000 chars).
    pub prompt: String,
    /// Priority — `high`, `medium`, `low`. Defaults to `medium`.
    #[serde(default)]
    pub priority: Option<String>,
}

/// Response for `POST /api/coordination/tasks/add`.
#[derive(Debug, Clone, Serialize)]
pub struct AddTaskResponse {
    /// Newly minted task id (`manual-YYYYMMDD-HHMMSS`).
    pub id: String,
    /// Status of the newly added task — always `pending`.
    pub status: String,
}

/// Request body for `POST /api/coordination/tasks/claim/:id`.
#[derive(Debug, Clone, Deserialize)]
pub struct ClaimRequest {
    /// Identifier of the agent / tab claiming the task. Recorded as the new
    /// `source` annotation so the other tab can see the claim.
    pub claimant: String,
}

/// Response for `POST /api/coordination/tasks/claim/:id`.
#[derive(Debug, Clone, Serialize)]
pub struct ClaimResponse {
    /// Task id.
    pub id: String,
    /// Updated status — `in_progress` after a successful claim.
    pub status: String,
    /// ISO-8601 UTC timestamp the claim was recorded.
    pub started: String,
}

/// Response for `GET /api/coordination/tasks/:id/logs`.
#[derive(Debug, Clone, Serialize)]
pub struct TaskLogsResponse {
    /// Task id (echoed for correlation).
    pub id: String,
    /// Path to the task's log file on disk, if the conductor has produced one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    /// Tail of the log file (last 200 lines), or empty if unavailable.
    pub tail: Vec<String>,
}

/// Summary of a single chat session, derived from
/// `~/lightarchitects/soul/helix/chat/sessions/<id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionSummary {
    /// Session UUID.
    pub session_id: String,
    /// Lifecycle status string from the registry record (`running`, `stopped`).
    pub status: String,
    /// Participant list (sibling names).
    pub participants: Vec<String>,
    /// Current topic, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_topic: Option<String>,
    /// Number of messages on disk (best-effort: counts entries in the
    /// session's history file when present, else `None`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u64>,
}

/// Response for `GET /api/coordination/chat/sessions`.
#[derive(Debug, Clone, Serialize)]
pub struct ChatSessionsResponse {
    /// All session summaries discovered on disk.
    pub sessions: Vec<ChatSessionSummary>,
}

/// Request body for `POST /api/coordination/chat/inject`.
#[derive(Debug, Clone, Deserialize)]
pub struct InjectRequest {
    /// Target session id.
    pub session_id: String,
    /// Message body to inject (max 4000 chars).
    pub message: String,
}

/// Response for `POST /api/coordination/chat/inject`.
#[derive(Debug, Clone, Serialize)]
pub struct InjectResponse {
    /// Always `true` on success.
    pub injected: bool,
    /// Trace id (synthetic — the inject CLI does not return a message id
    /// today; we mint a short hex correlation id for client display).
    pub correlation_id: String,
}

/// One delta on the chat SSE stream.
///
/// Sent as the JSON body of each SSE `data:` line. The frontend dispatches
/// on `kind` to render the right UI element.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatStreamEvent {
    /// New message detected on the session's history file.
    Message {
        /// Session id this message belongs to.
        session_id: String,
        /// Speaker / sibling identifier.
        speaker: String,
        /// Plain-text message body.
        body: String,
        /// ISO-8601 UTC timestamp.
        timestamp: String,
    },
    /// Heartbeat — emitted every poll cycle even if no new messages, so the
    /// browser can distinguish "stream alive" from "stream stalled".
    Heartbeat {
        /// Session id (or empty string when streaming all sessions).
        session_id: String,
        /// ISO-8601 UTC timestamp of the heartbeat.
        timestamp: String,
    },
    /// Error encountered while polling — non-fatal; the stream continues.
    Warning {
        /// Human-readable description of what went wrong.
        message: String,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn task_summary_round_trips() {
        let t = TaskSummary {
            id: "manual-1".into(),
            title: "Test".into(),
            project: "lightarchitects-sdk".into(),
            prompt_excerpt: "do the thing".into(),
            status: "pending".into(),
            source: "manual".into(),
            priority: "medium".into(),
            added: Some("2026-04-29T17:00:00Z".into()),
            started: None,
            finished: None,
        };
        let json = serde_json::to_string(&t).expect("serialize");
        let back: TaskSummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.id, "manual-1");
        assert_eq!(back.priority, "medium");
        assert!(back.started.is_none());
    }

    #[test]
    fn add_task_request_accepts_minimal_body() {
        let json = r#"{"title":"x","project":"p","prompt":"do"}"#;
        let req: AddTaskRequest = serde_json::from_str(json).expect("parse");
        assert_eq!(req.title, "x");
        assert!(req.priority.is_none());
    }

    #[test]
    fn chat_stream_event_serialises_kind_tag() {
        let ev = ChatStreamEvent::Heartbeat {
            session_id: "abc".into(),
            timestamp: "2026-04-29T17:00:00Z".into(),
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        assert!(json.contains(r#""kind":"heartbeat""#), "{json}");
    }

    #[test]
    fn chat_stream_event_message_has_body() {
        let ev = ChatStreamEvent::Message {
            session_id: "s".into(),
            speaker: "eva".into(),
            body: "hello".into(),
            timestamp: "t".into(),
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        assert!(json.contains(r#""kind":"message""#));
        assert!(json.contains(r#""speaker":"eva""#));
        assert!(json.contains(r#""body":"hello""#));
    }

    #[test]
    fn task_summary_omits_none_timestamps() {
        let t = TaskSummary {
            id: "x".into(),
            title: "t".into(),
            project: "p".into(),
            prompt_excerpt: String::new(),
            status: "pending".into(),
            source: "manual".into(),
            priority: "low".into(),
            added: None,
            started: None,
            finished: None,
        };
        let json = serde_json::to_string(&t).expect("serialize");
        assert!(!json.contains("started"), "{json}");
        assert!(!json.contains("finished"), "{json}");
        assert!(!json.contains("added"), "{json}");
    }
}
