//! Core state types for the lightsquad orchestration engine.
//!
//! Typed enums follow Ironclaw §15 verbatim: no `String` for status values.
//! [`SharedState`] is accessed through an `Arc<RwLock<_>>` because reads
//! dominate during wave execution. [`Coordinator`] distributes `Arc`-cloned
//! handles so all downstream components share the same state and mutex.
//!
//! # Re-exports from `la_lightsquad`
//!
//! The four status enums ([`TaskStatus`], [`WaveStatus`], [`BuildStatus`],
//! [`AgentStatus`]) are re-exported from the public `la_lightsquad` crate.
//! This ensures wire-format compatibility between the SDK and any external
//! consumer that depends on `la_lightsquad` directly.

use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Notify, RwLock};

// ── Status enums (re-exported from la_lightsquad) ───────────────────────────

pub use la_lightsquad::{AgentStatus, BuildStatus, TaskStatus, WaveStatus};

// ── Task definition ───────────────────────────────────────────────────────────

/// Context bundle injected into a worker agent, split into three tiers.
///
/// Matches `manifest_schema_version: "1.1.0"` from gitforest (the
/// `context_tiers` array, not the superseded `context_budget` struct).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTier {
    /// Tier label: `"T1"` (global), `"T2"` (task), or `"T3"` (file set).
    pub tier: String,
    /// Human-readable label (e.g. `"Global CLAUDE.md"`).
    pub label: String,
    /// File paths to include in this tier's context injection.
    pub files: Vec<String>,
    /// Estimated token cost of this tier (used for budget enforcement).
    pub token_estimate: u32,
}

/// A single unit of work dispatched to a worker slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier within the build (e.g. `"task-001"`).
    pub id: String,
    /// Git branch the worker operates on (e.g. `"task/build/task-001"`).
    pub branch: String,
    /// IDs of tasks that must reach [`TaskStatus::Complete`] before this one starts.
    pub depends_on: Vec<String>,
    /// Domain role of the agent executing this task.
    ///
    /// Used to populate [`super::supervisor::HitlEscalation::escalating_role`] and
    /// carried into SSE envelopes so the operator can filter by role.
    /// Defaults to [`super::agent_role::AgentRole::Engineer`].
    #[serde(default)]
    pub role: super::agent_role::AgentRole,
    /// Worktree-relative file paths this task is exclusively allowed to write.
    ///
    /// When empty, ownership enforcement is opt-out (legacy / interactive
    /// mode). When non-empty, the wave dispatcher rejects the wave if any
    /// two tasks claim the same file (`WaveError::OwnershipConflict`) and the
    /// worker fails if it writes files outside this set (post-task PoT-1
    /// gate per agents-playbook §15.3.13).
    #[serde(default)]
    pub file_ownership: Vec<String>,
    /// Declares this task as a read-only investigation (no filesystem writes,
    /// no git mutations, no shared external state). The wave dispatcher uses a
    /// separate, larger slot budget (`SLOT_CAPACITY_READ`, default 16) for
    /// these tasks so context-gathering doesn't burn the
    /// `SLOT_CAPACITY` (7) budget reserved for write tasks.
    ///
    /// Mirrors Claude Code's per-tool `isConcurrencySafe()` predicate, lifted
    /// to the wave-task layer. Use for: codebase exploration, dependency
    /// research, prior-art lookup via context7, doc retrieval.
    #[serde(default)]
    pub concurrency_safe: bool,
    /// Context bundles for the worker's system-prompt injection.
    pub context_tiers: Vec<ContextTier>,
    /// Prompt describing the task for the worker subprocess.
    pub prompt: String,
    /// Per-task container policy override — may only *tighten* the system-wide
    /// policy enforced by `ContainerExecutor`.  `None` = use system defaults.
    /// Ignored by `InProcessExecutor`.
    #[serde(default)]
    pub policy_override: Option<TaskPolicyOverride>,
}

/// Per-task policy tightening hook for container-executor builds.
///
/// All fields are optional (`None` = use system default).  A non-`None` value
/// that is *less restrictive* than the effective system policy causes
/// [`WorkerError::Policy`] in `ContainerExecutor::dispatch_one`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskPolicyOverride {
    /// Isolation mode — must be ≥ system `iso_mode` strictness when set.
    #[serde(default)]
    pub iso_mode: Option<crate::container_spawn::IsoMode>,
    /// Network policy — must be ≥ system `network` strictness when set.
    #[serde(default)]
    pub network: Option<crate::container_spawn::NetworkPolicy>,
    /// Memory cap in MiB — must be ≤ system cap when set.
    #[serde(default)]
    pub memory_mb: Option<u64>,
    /// CPU allocation — must be ≤ system cap when set.
    #[serde(default)]
    pub cpus: Option<f64>,
}

// ── Shared state ──────────────────────────────────────────────────────────────

/// Snapshot of all task, build, and agent statuses for a running build.
///
/// Accessed through [`Coordinator::state`]. Writers take the write lock only
/// during status transitions; all other access is read-only.
#[derive(Debug, Default)]
pub struct SharedState {
    /// Task statuses keyed by [`Task::id`].
    pub tasks: HashMap<String, TaskStatus>,
    /// Build statuses keyed by build codename.
    pub builds: HashMap<String, BuildStatus>,
    /// Agent subprocess statuses keyed by [`Task::id`].
    pub agent_statuses: HashMap<String, AgentStatus>,
}

// ── Coordinator ───────────────────────────────────────────────────────────────

/// Central coordinator handle for a running build.
///
/// Clone-cheap: all fields are `Arc`-wrapped. The wave dispatcher, merge
/// agent, and worker slots each hold a clone of this handle.
///
/// `ops_mutex` serialises all ref-mutating git operations (branch cuts,
/// merges, worktree add/remove) across the entire build. Both [`MergeAgent`]
/// and [`WorktreeManager`] clone this `Arc` from the coordinator so a single
/// underlying mutex governs all concurrent git writers.
///
/// [`MergeAgent`]: super::merge_agent::MergeAgent
/// [`WorktreeManager`]: super::worktree_manager::WorktreeManager
#[derive(Clone)]
pub struct Coordinator {
    /// Shared task/build/agent state — `RwLock` because reads dominate.
    pub state: Arc<RwLock<SharedState>>,
    /// Wake-up signal for the dispatcher when a dependency completes.
    pub notify: Arc<Notify>,
    /// Serialises ALL ref-mutating git operations across the build.
    pub ops_mutex: Arc<Mutex<()>>,
}

impl Coordinator {
    /// Create a fresh coordinator for a new build.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(SharedState::default())),
            notify: Arc::new(Notify::new()),
            ops_mutex: Arc::new(Mutex::new(())),
        }
    }
}

impl Default for Coordinator {
    fn default() -> Self {
        Self::new()
    }
}

// ── Scheduling predicate ──────────────────────────────────────────────────────

/// Returns `true` when all of `task.depends_on` have [`TaskStatus::Complete`].
///
/// O(1) per dependency via `HashMap::get`. Called by the wave dispatcher
/// before each task dispatch tick.
pub fn can_run(task: &Task, state: &SharedState) -> bool {
    task.depends_on
        .iter()
        .all(|dep| state.tasks.get(dep.as_str()) == Some(&TaskStatus::Complete))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn task(id: &str, deps: &[&str]) -> Task {
        Task {
            id: id.to_owned(),
            branch: format!("task/{id}"),
            depends_on: deps.iter().map(|s| (*s).to_owned()).collect(),
            role: crate::lightsquad::agent_role::AgentRole::Engineer,
            file_ownership: vec![],
            concurrency_safe: false,
            context_tiers: vec![],
            prompt: "test".to_owned(),
            policy_override: None,
        }
    }

    #[test]
    fn can_run_no_deps_always_true() {
        assert!(can_run(&task("t1", &[]), &SharedState::default()));
    }

    #[test]
    fn can_run_pending_dep_is_false() {
        let t = task("t2", &["t1"]);
        let mut s = SharedState::default();
        s.tasks.insert("t1".to_owned(), TaskStatus::Pending);
        assert!(!can_run(&t, &s));
    }

    #[test]
    fn can_run_in_progress_dep_is_false() {
        let t = task("t2", &["t1"]);
        let mut s = SharedState::default();
        s.tasks.insert("t1".to_owned(), TaskStatus::InProgress);
        assert!(!can_run(&t, &s));
    }

    #[test]
    fn can_run_complete_dep_is_true() {
        let t = task("t2", &["t1"]);
        let mut s = SharedState::default();
        s.tasks.insert("t1".to_owned(), TaskStatus::Complete);
        assert!(can_run(&t, &s));
    }

    #[test]
    fn can_run_missing_dep_is_false() {
        let t = task("t2", &["t1"]);
        assert!(!can_run(&t, &SharedState::default()));
    }

    #[test]
    fn can_run_all_complete_is_true() {
        let t = task("t3", &["t1", "t2"]);
        let mut s = SharedState::default();
        s.tasks.insert("t1".to_owned(), TaskStatus::Complete);
        s.tasks.insert("t2".to_owned(), TaskStatus::Complete);
        assert!(can_run(&t, &s));
    }

    #[test]
    fn can_run_partial_complete_is_false() {
        let t = task("t3", &["t1", "t2"]);
        let mut s = SharedState::default();
        s.tasks.insert("t1".to_owned(), TaskStatus::Complete);
        s.tasks.insert("t2".to_owned(), TaskStatus::InProgress);
        assert!(!can_run(&t, &s));
    }

    #[tokio::test]
    async fn coordinator_new_has_empty_state() {
        let coord = Coordinator::new();
        let state = coord.state.read().await;
        assert!(state.tasks.is_empty());
        assert!(state.builds.is_empty());
        assert!(state.agent_statuses.is_empty());
    }

    #[test]
    fn task_status_serialises_snake_case() {
        let json = serde_json::to_string(&TaskStatus::InProgress).unwrap();
        assert_eq!(json, r#""in_progress""#);
    }

    #[test]
    fn build_status_serialises_snake_case() {
        let json = serde_json::to_string(&BuildStatus::Gating).unwrap();
        assert_eq!(json, r#""gating""#);
    }
}
