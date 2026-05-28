//! JSON-schema type definitions for the `lightsquad_plan` copilot tool.
//!
//! This module owns the serde structs that the LLM populates when it calls
//! `lightsquad_plan`, plus the [`lightsquad_plan_tool_definition`] function
//! that returns the JSON Schema required by [`crate::agent::tool_executor::ToolDefinition`].

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::agent::tool_executor::ToolDefinition;

#[allow(clippy::expect_used)]
static CODENAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9-]{3,40}$").expect("static regex"));
#[allow(clippy::expect_used)]
static TASK_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9_-]{2,40}$").expect("static regex"));

// ── Capacity limits (anti-runaway guard) ─────────────────────────────────────

/// Maximum number of waves per plan.
pub const MAX_WAVES: usize = 6;
/// Maximum number of tasks per wave.
pub const MAX_TASKS_PER_WAVE: usize = 16;

// ── Input structs (deserialized from LLM tool_use) ────────────────────────────

/// Top-level plan passed to the `lightsquad_plan` tool.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlanInput {
    /// Build codename: `^[a-z0-9-]{3,40}$`.
    pub codename: String,
    /// One-sentence description of intent (included in decision log).
    pub intent: String,
    /// Git feature branch to accumulate merged task results.
    pub feat_branch: String,
    /// Sequential waves; tasks within a wave run in parallel.
    pub waves: Vec<WaveInput>,
}

/// A single wave of parallel tasks.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WaveInput {
    /// Human-readable wave name (used in decision log entries).
    pub name: String,
    /// Tasks in this wave — run in parallel, subject to DAG `depends_on`.
    pub tasks: Vec<TaskInput>,
}

/// A single task within a wave.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskInput {
    /// Task id: `^[a-z0-9_-]{2,40}$`.
    pub id: String,
    /// Full implementation prompt for the autonomous worker.
    pub prompt: String,
    /// `true` = read-only exploration; eligible for the 16-slot safe pool.
    #[serde(default)]
    pub concurrency_safe: bool,
    /// Repository-relative file paths this task may write (empty = no restriction).
    #[serde(default)]
    pub file_ownership: Vec<String>,
    /// IDs of tasks that must complete before this one starts.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

// ── Validation ────────────────────────────────────────────────────────────────

/// Validation errors returned from [`validate_plan`].
#[derive(Debug, thiserror::Error)]
pub enum PlanValidationError {
    /// Codename did not match `^[a-z0-9-]{3,40}$`.
    #[error("codename '{0}' does not match ^[a-z0-9-]{{3,40}}$")]
    BadCodename(String),
    /// Plan has no waves.
    #[error("plan contains no waves")]
    EmptyWaves,
    /// Plan exceeds [`MAX_WAVES`].
    #[error("plan has {0} waves; maximum is {MAX_WAVES}")]
    TooManyWaves(usize),
    /// A wave exceeds [`MAX_TASKS_PER_WAVE`].
    #[error("wave '{wave}' has {count} tasks; maximum is {MAX_TASKS_PER_WAVE}")]
    TooManyTasks {
        /// Wave name.
        wave: String,
        /// Observed task count.
        count: usize,
    },
    /// Task id did not match `^[a-z0-9_-]{2,40}$`.
    #[error("task id '{0}' does not match ^[a-z0-9_-]{{2,40}}$")]
    BadTaskId(String),
    /// Two tasks share the same id.
    #[error("duplicate task id '{0}'")]
    DuplicateTaskId(String),
}

/// Validate the plan, returning the first error found.
///
/// # Errors
///
/// Returns [`PlanValidationError`] on the first violated constraint.
pub fn validate_plan(plan: &PlanInput) -> Result<(), PlanValidationError> {
    if !CODENAME_RE.is_match(&plan.codename) {
        return Err(PlanValidationError::BadCodename(plan.codename.clone()));
    }

    if plan.waves.is_empty() {
        return Err(PlanValidationError::EmptyWaves);
    }
    if plan.waves.len() > MAX_WAVES {
        return Err(PlanValidationError::TooManyWaves(plan.waves.len()));
    }

    let mut seen_ids = std::collections::HashSet::new();

    for wave in &plan.waves {
        if wave.tasks.len() > MAX_TASKS_PER_WAVE {
            return Err(PlanValidationError::TooManyTasks {
                wave: wave.name.clone(),
                count: wave.tasks.len(),
            });
        }
        for task in &wave.tasks {
            if !TASK_ID_RE.is_match(&task.id) {
                return Err(PlanValidationError::BadTaskId(task.id.clone()));
            }
            if !seen_ids.insert(task.id.clone()) {
                return Err(PlanValidationError::DuplicateTaskId(task.id.clone()));
            }
        }
    }
    Ok(())
}

// ── Tool definition ───────────────────────────────────────────────────────────

/// Returns the [`ToolDefinition`] for the `lightsquad_plan` copilot tool.
///
/// The JSON Schema enforces capacity limits and pattern constraints so the LLM
/// can self-correct before the executor rejects the call.
pub fn lightsquad_plan_tool_definition() -> ToolDefinition {
    let schema: Value = json!({
        "type": "object",
        "required": ["codename", "intent", "feat_branch", "waves"],
        "additionalProperties": false,
        "properties": {
            "codename": {
                "type": "string",
                "description": "Kebab-case build codename (3–40 chars, a-z0-9-).",
                "pattern": "^[a-z0-9-]{3,40}$"
            },
            "intent": {
                "type": "string",
                "description": "One-sentence description of what this build achieves."
            },
            "feat_branch": {
                "type": "string",
                "description": "Git feature branch that receives merged task results (e.g. feat/my-build)."
            },
            "waves": {
                "type": "array",
                "description": "Sequential waves; tasks within each wave run in parallel (max 6 waves).",
                "minItems": 1,
                "maxItems": MAX_WAVES,
                "items": {
                    "type": "object",
                    "required": ["name", "tasks"],
                    "additionalProperties": false,
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Human-readable wave name."
                        },
                        "tasks": {
                            "type": "array",
                            "description": "Parallel tasks in this wave (max 16).",
                            "minItems": 1,
                            "maxItems": MAX_TASKS_PER_WAVE,
                            "items": {
                                "type": "object",
                                "required": ["id", "prompt"],
                                "additionalProperties": false,
                                "properties": {
                                    "id": {
                                        "type": "string",
                                        "description": "Task id (2–40 chars, a-z0-9_-).",
                                        "pattern": "^[a-z0-9_-]{2,40}$"
                                    },
                                    "prompt": {
                                        "type": "string",
                                        "description": "Full implementation prompt for the autonomous worker."
                                    },
                                    "concurrency_safe": {
                                        "type": "boolean",
                                        "description": "true for read-only exploration tasks (16-slot pool). false (default) for any code-writing task.",
                                        "default": false
                                    },
                                    "file_ownership": {
                                        "type": "array",
                                        "description": "Repo-relative paths this task may write. Empty = no restriction.",
                                        "items": { "type": "string" },
                                        "default": []
                                    },
                                    "depends_on": {
                                        "type": "array",
                                        "description": "IDs of tasks that must complete before this one starts.",
                                        "items": { "type": "string" },
                                        "default": []
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    ToolDefinition {
        name: "lightsquad_plan".to_string(),
        description: "Launch an autonomous LightSquad build: spawns parallel workers that write \
            code, run tests, and commit results across multiple files. \
            INVOKE ONLY when the operator explicitly requests implementation work — e.g. \
            'build X', 'implement Y', 'add feature Z', 'create a new module for ...', \
            or when a /BUILD / /SQUAD skill is invoked. \
            DO NOT invoke for: questions ('how do I...', 'what is...', 'explain...'), \
            explanations of code or concepts, debugging advice, code review, \
            single-file edits, or any request that does not explicitly ask you to write \
            and commit code into a git branch. \
            When in doubt, answer the question directly in the streaming response instead. \
            The operator must approve the plan via HITL before execution begins. \
            Returns a BuildSummary with succeeded/failed task counts when complete."
            .to_string(),
        input_schema: schema,
    }
}
