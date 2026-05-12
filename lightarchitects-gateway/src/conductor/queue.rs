//! Task queue — JSON-backed task state management.
//!
//! The queue is a simple JSON file (`tasks/queue.json`) that tracks pending,
//! in-progress, completed, and failed tasks. State transitions are atomic
//! (read-modify-write with tmp+rename).

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The task queue file.
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskQueue {
    /// Schema version.
    pub version: String,
    /// All tasks (any status).
    pub tasks: Vec<Task>,
}

/// A single task in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier (e.g. `gh-soul-42`, `sec-RUSTSEC-2026-001`).
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Project path relative to `~/Projects/` (e.g. `SOUL/SOUL-DEV`).
    pub project: String,
    /// Detailed prompt for Claude Code.
    pub prompt: String,
    /// Current status.
    #[serde(default = "default_pending")]
    pub status: TaskStatus,
    /// Where this task came from.
    #[serde(default)]
    pub source: String,
    /// Priority level.
    #[serde(default)]
    pub priority: Priority,
    /// When the task was added.
    #[serde(default)]
    pub added: Option<DateTime<Utc>>,
    /// When execution started.
    #[serde(default)]
    pub started: Option<DateTime<Utc>>,
    /// When execution finished.
    #[serde(default)]
    pub finished: Option<DateTime<Utc>>,
    /// Number of retry attempts so far.
    #[serde(default)]
    pub retries: u32,
    /// Path to the output log file (set after execution).
    #[serde(default)]
    pub output_log: Option<String>,
    /// Assertion gate ID blocking this task (set when status is
    /// [`TaskStatus::AwaitingOperatorResolution`]).
    #[serde(default)]
    pub awaiting_assertion_id: Option<String>,
    /// Deadline by which the operator must resolve the blocked gate. After this
    /// time the conductor transitions the task to [`TaskStatus::Failed`].
    #[serde(default)]
    pub resolution_deadline: Option<DateTime<Utc>>,
    /// Build codename this task belongs to (e.g. `squad-comms-session-per-build`).
    /// Scopes the task to a specific build for multi-agent coordination.
    #[serde(default)]
    pub build_codename: Option<String>,
    /// Agent or worker that has claimed this task.
    #[serde(default)]
    pub assignee: Option<String>,
    /// UUID of the soul-chat session for this build. Set by `session_start` and
    /// propagated to all tasks in the same build so workers can join the session.
    #[serde(default)]
    pub build_session_id: Option<String>,
}

/// Task lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Waiting to be picked up.
    Pending,
    /// Currently executing.
    InProgress,
    /// Completed successfully.
    Completed,
    /// Failed after max retries.
    Failed,
    /// Exceeded wall time limit.
    Timeout,
    /// Manually skipped.
    Skipped,
    /// Blocked waiting for an operator to resolve an assertion gate.
    ///
    /// The assertion ID and resolution deadline are stored in the parent
    /// [`Task`] fields `awaiting_assertion_id` and `resolution_deadline`.
    /// Conductor polls `resolution_deadline` and transitions to `Failed`
    /// on expiry, or resumes execution when the assertion is resolved.
    AwaitingOperatorResolution,
}

/// Task priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    /// Security fixes, blockers.
    High,
    /// Normal feature work.
    Medium,
    /// Quality improvements, cleanup.
    Low,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Medium
    }
}

impl TaskQueue {
    /// Load the queue from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, QueueError> {
        let content = std::fs::read_to_string(path).map_err(QueueError::Io)?;
        serde_json::from_str(&content).map_err(QueueError::Parse)
    }

    /// Save the queue to disk atomically (tmp + rename).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), QueueError> {
        let content = serde_json::to_string_pretty(self).map_err(QueueError::Serialize)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &content).map_err(QueueError::Io)?;
        std::fs::rename(&tmp, path).map_err(QueueError::Io)?;
        Ok(())
    }

    /// Return the next pending task, sorted by priority (high first).
    #[must_use]
    pub fn next_pending(&self) -> Option<&Task> {
        self.tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .min_by_key(|t| match t.priority {
                Priority::High => 0,
                Priority::Medium => 1,
                Priority::Low => 2,
            })
    }

    /// Count tasks by status.
    #[must_use]
    pub fn count_by_status(&self, status: TaskStatus) -> usize {
        self.tasks.iter().filter(|t| t.status == status).count()
    }

    /// Update a task's status by id. Returns false if the task was not found.
    pub fn set_status(&mut self, id: &str, status: TaskStatus) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.status = status;
            match status {
                TaskStatus::InProgress => task.started = Some(Utc::now()),
                TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Timeout => {
                    task.finished = Some(Utc::now());
                }
                _ => {}
            }
            true
        } else {
            false
        }
    }

    /// Increment the retry count for a task. Returns the new count.
    pub fn increment_retries(&mut self, id: &str) -> u32 {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.retries = task.retries.saturating_add(1);
            task.retries
        } else {
            0
        }
    }
}

/// Queue errors.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    /// IO error reading/writing the queue file.
    #[error("queue IO error: {0}")]
    Io(std::io::Error),
    /// Failed to parse the queue JSON.
    #[error("queue parse error: {0}")]
    Parse(serde_json::Error),
    /// Failed to serialize the queue.
    #[error("queue serialize error: {0}")]
    Serialize(serde_json::Error),
}

fn default_pending() -> TaskStatus {
    TaskStatus::Pending
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_queue() -> TaskQueue {
        TaskQueue {
            version: "1.0".into(),
            tasks: vec![
                Task {
                    id: "t-high".into(),
                    title: "High priority".into(),
                    project: "SOUL/SOUL-DEV".into(),
                    prompt: "Do the thing".into(),
                    status: TaskStatus::Pending,
                    source: "manual".into(),
                    priority: Priority::High,
                    added: None,
                    started: None,
                    finished: None,
                    retries: 0,
                    output_log: None,
                    awaiting_assertion_id: None,
                    resolution_deadline: None,
                    build_codename: None,
                    assignee: None,
                    build_session_id: None,
                },
                Task {
                    id: "t-med".into(),
                    title: "Medium priority".into(),
                    project: "conductor".into(),
                    prompt: "Do the other thing".into(),
                    status: TaskStatus::Pending,
                    source: "github".into(),
                    priority: Priority::Medium,
                    added: None,
                    started: None,
                    finished: None,
                    retries: 0,
                    output_log: None,
                    awaiting_assertion_id: None,
                    resolution_deadline: None,
                    build_codename: None,
                    assignee: None,
                    build_session_id: None,
                },
                Task {
                    id: "t-done".into(),
                    title: "Already done".into(),
                    project: "conductor".into(),
                    prompt: "Was done".into(),
                    status: TaskStatus::Completed,
                    source: "manual".into(),
                    priority: Priority::Low,
                    added: None,
                    started: None,
                    finished: None,
                    retries: 0,
                    output_log: None,
                    awaiting_assertion_id: None,
                    resolution_deadline: None,
                    build_codename: None,
                    assignee: None,
                    build_session_id: None,
                },
            ],
        }
    }

    #[test]
    fn next_pending_returns_highest_priority() {
        let queue = make_queue();
        let next = queue.next_pending().unwrap();
        assert_eq!(next.id, "t-high");
    }

    #[test]
    fn next_pending_skips_completed() {
        let mut queue = make_queue();
        queue.set_status("t-high", TaskStatus::Completed);
        let next = queue.next_pending().unwrap();
        assert_eq!(next.id, "t-med");
    }

    #[test]
    fn next_pending_returns_none_when_empty() {
        let queue = TaskQueue {
            version: "1.0".into(),
            tasks: vec![],
        };
        assert!(queue.next_pending().is_none());
    }

    #[test]
    fn count_by_status_counts_correctly() {
        let queue = make_queue();
        assert_eq!(queue.count_by_status(TaskStatus::Pending), 2);
        assert_eq!(queue.count_by_status(TaskStatus::Completed), 1);
        assert_eq!(queue.count_by_status(TaskStatus::Failed), 0);
    }

    #[test]
    fn set_status_updates_task() {
        let mut queue = make_queue();
        assert!(queue.set_status("t-high", TaskStatus::InProgress));
        let task = queue.tasks.iter().find(|t| t.id == "t-high").unwrap();
        assert_eq!(task.status, TaskStatus::InProgress);
        assert!(task.started.is_some());
    }

    #[test]
    fn set_status_returns_false_for_missing() {
        let mut queue = make_queue();
        assert!(!queue.set_status("nonexistent", TaskStatus::Completed));
    }

    #[test]
    fn set_status_records_finished_timestamp() {
        let mut queue = make_queue();
        queue.set_status("t-high", TaskStatus::Failed);
        let task = queue.tasks.iter().find(|t| t.id == "t-high").unwrap();
        assert!(task.finished.is_some());
    }

    #[test]
    fn increment_retries_increments() {
        let mut queue = make_queue();
        assert_eq!(queue.increment_retries("t-high"), 1);
        assert_eq!(queue.increment_retries("t-high"), 2);
        assert_eq!(queue.increment_retries("t-high"), 3);
    }

    #[test]
    fn increment_retries_returns_zero_for_missing() {
        let mut queue = make_queue();
        assert_eq!(queue.increment_retries("nonexistent"), 0);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let queue = make_queue();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        queue.save(tmp.path()).unwrap();

        let loaded = TaskQueue::load(tmp.path()).unwrap();
        assert_eq!(loaded.tasks.len(), 3);
        assert_eq!(loaded.tasks[0].id, "t-high");
        assert_eq!(loaded.tasks[1].priority, Priority::Medium);
        assert_eq!(loaded.tasks[2].status, TaskStatus::Completed);
    }

    #[test]
    fn save_is_atomic_via_tmp_rename() {
        let queue = make_queue();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("queue.json");

        queue.save(&path).unwrap();

        // .tmp file should not exist (renamed to .json)
        let tmp_path = path.with_extension("tmp");
        assert!(!tmp_path.exists());
        assert!(path.exists());
    }

    #[test]
    fn load_fails_gracefully_on_missing_file() {
        let result = TaskQueue::load(Path::new("/nonexistent/queue.json"));
        assert!(result.is_err());
    }

    #[test]
    fn load_fails_gracefully_on_invalid_json() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "not valid json").unwrap();
        let result = TaskQueue::load(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn priority_sorting_respects_all_levels() {
        let mut queue = TaskQueue {
            version: "1.0".into(),
            tasks: vec![
                Task {
                    id: "low".into(),
                    title: "Low".into(),
                    project: "X".into(),
                    prompt: String::new(),
                    status: TaskStatus::Pending,
                    source: String::new(),
                    priority: Priority::Low,
                    added: None,
                    started: None,
                    finished: None,
                    retries: 0,
                    output_log: None,
                    awaiting_assertion_id: None,
                    resolution_deadline: None,
                    build_codename: None,
                    assignee: None,
                    build_session_id: None,
                },
                Task {
                    id: "high".into(),
                    title: "High".into(),
                    project: "X".into(),
                    prompt: String::new(),
                    status: TaskStatus::Pending,
                    source: String::new(),
                    priority: Priority::High,
                    added: None,
                    started: None,
                    finished: None,
                    retries: 0,
                    output_log: None,
                    awaiting_assertion_id: None,
                    resolution_deadline: None,
                    build_codename: None,
                    assignee: None,
                    build_session_id: None,
                },
            ],
        };
        // High should come first despite being added second
        assert_eq!(queue.next_pending().unwrap().id, "high");

        // After completing high, low is next
        queue.set_status("high", TaskStatus::Completed);
        assert_eq!(queue.next_pending().unwrap().id, "low");
    }
}
