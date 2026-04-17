//! Claude Code executor — spawns `claude` CLI sessions for task execution.
//!
//! Each task gets its own Claude Code session with a structured prompt that
//! includes the orchestrator context, task details, and constraints.
//! Each execution writes a `{task-id}.trace.json` alongside the log.

use std::path::Path;
use std::time::Instant;

use serde::Serialize;
use tokio::process::Command;

use super::config::BudgetConfig;
use super::queue::Task;

/// Result of executing a task via Claude Code.
#[derive(Debug)]
pub struct ExecutionResult {
    /// Whether the task completed successfully.
    pub success: bool,
    /// Exit code from the Claude Code process.
    pub exit_code: i32,
    /// Wall-clock duration in seconds.
    pub elapsed_secs: u64,
    /// Whether the task was killed due to timeout.
    pub timed_out: bool,
    /// Path to the output log file.
    pub log_path: String,
}

/// Execution trace written as JSON for observability.
#[derive(Debug, Serialize)]
struct ExecutionTrace {
    task_id: String,
    project: String,
    started_at: String,
    finished_at: String,
    elapsed_secs: u64,
    success: bool,
    timed_out: bool,
    exit_code: i32,
}

/// Build the full prompt for Claude Code, including orchestrator context and guardrails.
fn build_prompt(task: &Task, guardrails: &str) -> String {
    let guardrails_section = if guardrails.is_empty() {
        String::new()
    } else {
        format!("\nGUARDRAILS (learned from prior failures — read these FIRST):\n{guardrails}\n")
    };

    format!(
        r"You are executing an autonomous conductor task. Use the Light Architects orchestrator (lightarchitects:orchestrator) to classify and route this task to the appropriate domain agent(s). If the orchestrator is unavailable, fall back to direct tool use.

TASK: {title}
PROJECT: {project}
{guardrails_section}
{prompt}

WORKFLOW:
1. Create a git branch: conductor/{id}
2. Execute the task using appropriate meta-skills (/BUILD, /RESEARCH, /SECURE, etc.)
3. Run quality gates: cargo fmt, cargo clippy --all-targets -- -D warnings, cargo test
4. If quality gates pass, commit with a descriptive message
5. Create a PR with gh pr create (do NOT merge — Kevin reviews)
6. Run /REFLECT — what was learned? Propose CLAUDE.md updates if warranted
7. Run /ENRICH — save significant outputs to SOUL helix if significance >= 7.0

CONSTRAINTS:
- If stuck after 3 attempts on the same error, STOP and report the blocker
- All work on the conductor/{id} branch — never modify main directly
- Follow Builders Cookbook standards (no unwrap, no panic, complexity <= 10, 60-line functions)
- Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com> in every commit",
        title = task.title,
        project = task.project,
        prompt = task.prompt,
        id = task.id,
        guardrails_section = guardrails_section,
    )
}

/// Execute a task by spawning a Claude Code CLI session.
///
/// # Errors
///
/// Returns an error if the Claude Code process cannot be spawned.
pub async fn execute_task(
    task: &Task,
    projects_root: &Path,
    budgets: &BudgetConfig,
    log_dir: &Path,
    guardrails: &str,
) -> Result<ExecutionResult, ExecutorError> {
    let prompt = build_prompt(task, guardrails);
    let cwd = projects_root.join(&task.project);

    if !cwd.exists() {
        return Err(ExecutorError::ProjectNotFound(task.project.clone()));
    }

    // Ensure log directory exists.
    std::fs::create_dir_all(log_dir).map_err(ExecutorError::Io)?;

    let started_at = chrono::Utc::now();
    let timestamp = started_at.format("%Y%m%d-%H%M%S");
    let log_path = log_dir
        .join(format!("{}-{}.log", task.id, timestamp))
        .display()
        .to_string();

    let log_file = std::fs::File::create(&log_path).map_err(ExecutorError::Io)?;

    tracing::info!(
        task_id = %task.id,
        project = %task.project,
        "spawning Claude Code session"
    );

    let start = Instant::now();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(budgets.max_wall_time_secs),
        Command::new("claude")
            .arg("--print")
            .arg("--dangerously-skip-permissions")
            .arg("-p")
            .arg(&prompt)
            .current_dir(&cwd)
            .stdout(log_file.try_clone().map_err(ExecutorError::Io)?)
            .stderr(log_file)
            .spawn()
            .map_err(ExecutorError::Spawn)?
            .wait(),
    )
    .await;

    let elapsed_secs = start.elapsed().as_secs();
    let finished_at = chrono::Utc::now();

    let exec_result = match result {
        Ok(Ok(status)) => ExecutionResult {
            success: status.success(),
            exit_code: status.code().unwrap_or(-1),
            elapsed_secs,
            timed_out: false,
            log_path: log_path.clone(),
        },
        Ok(Err(e)) => return Err(ExecutorError::Wait(e)),
        Err(_) => {
            tracing::warn!(
                task_id = %task.id,
                elapsed = elapsed_secs,
                limit = budgets.max_wall_time_secs,
                "task timed out"
            );
            ExecutionResult {
                success: false,
                exit_code: 124,
                elapsed_secs,
                timed_out: true,
                log_path: log_path.clone(),
            }
        }
    };

    // Write trace JSON alongside the log.
    let trace = ExecutionTrace {
        task_id: task.id.clone(),
        project: task.project.clone(),
        started_at: started_at.to_rfc3339(),
        finished_at: finished_at.to_rfc3339(),
        elapsed_secs: exec_result.elapsed_secs,
        success: exec_result.success,
        timed_out: exec_result.timed_out,
        exit_code: exec_result.exit_code,
    };
    let trace_path = log_dir.join(format!("{}-{}.trace.json", task.id, timestamp));
    if let Ok(json) = serde_json::to_string_pretty(&trace) {
        if let Err(e) = std::fs::write(&trace_path, &json) {
            tracing::warn!(error = %e, "failed to write trace");
        }
    }

    Ok(exec_result)
}

/// Executor errors.
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    /// The project directory does not exist.
    #[error("project not found: {0}")]
    ProjectNotFound(String),
    /// IO error.
    #[error("IO error: {0}")]
    Io(std::io::Error),
    /// Failed to spawn the Claude Code process.
    #[error("failed to spawn claude: {0}")]
    Spawn(std::io::Error),
    /// Failed to wait for the Claude Code process.
    #[error("failed to wait for claude: {0}")]
    Wait(std::io::Error),
}
