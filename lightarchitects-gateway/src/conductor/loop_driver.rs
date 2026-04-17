//! Conductor — the state machine that drives the autonomous loop.
//!
//! Mirrors Arena's conductor pattern: DISCOVER -> PICK -> EXECUTE -> VERIFY -> LOOP.
//! Uses Claude Code CLI instead of Ollama for task execution.
//!
//! Includes gutter detection (same error 3x = stuck), guardrails (learned
//! failures persist across context rotations), heartbeat, and metrics export.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use super::config::Config;
use super::executor;
use super::guardrails;
use super::queue::{TaskQueue, TaskStatus};

/// Per-task error signature history for gutter detection.
type GutterMap = HashMap<String, Vec<u64>>;

/// Cumulative metrics exported to `conductor.metrics.json`.
#[derive(Debug, Default, Serialize)]
struct Metrics {
    completed: u64,
    failed: u64,
    guttered: u64,
    timed_out: u64,
    total_duration_secs: u64,
    avg_duration_secs: f64,
    last_updated: String,
}

/// Run the conductor loop.
///
/// Loops indefinitely (or once with `--once`), picking tasks from the queue,
/// injecting guardrails, executing via Claude Code, detecting gutters, and
/// appending signs on failure. Writes heartbeat every interval and exports
/// metrics after each task.
///
/// # Errors
///
/// Returns an error if the queue file cannot be loaded or a fatal IO error
/// occurs.
pub async fn run(config: &Config, once: bool) -> Result<(), ConductorError> {
    let queue_path = &config.paths.queue;
    let guardrails_path = config.base_dir.join("guardrails.md");
    let mut gutter_map: GutterMap = HashMap::new();
    let mut metrics = Metrics::default();

    // Graceful shutdown on SIGTERM / SIGINT.
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        shutdown_clone.store(true, Ordering::Relaxed);
        tracing::info!("received shutdown signal");
    });

    tracing::info!(
        wip = config.conductor.wip_limit,
        poll = config.conductor.poll_interval_secs,
        wall_time = config.budgets.max_wall_time_secs,
        retries = config.budgets.max_retries,
        "conductor starting"
    );

    // Spawn heartbeat writer.
    let heartbeat_path = config.heartbeat_path();
    let heartbeat_interval = config.conductor.heartbeat_interval_secs;
    let heartbeat_shutdown = Arc::clone(&shutdown);
    tokio::spawn(async move {
        loop {
            if heartbeat_shutdown.load(Ordering::Relaxed) {
                break;
            }
            write_heartbeat(&heartbeat_path);
            tokio::time::sleep(std::time::Duration::from_secs(heartbeat_interval)).await;
        }
    });

    loop {
        if shutdown.load(Ordering::Relaxed) {
            tracing::info!("shutting down gracefully");
            let _ = std::fs::remove_file(config.pid_path());
            break;
        }

        let ran_task = run_one_iteration(
            config,
            queue_path,
            &guardrails_path,
            &mut gutter_map,
            &mut metrics,
        )
        .await?;

        if once {
            if ran_task {
                tracing::info!("--once mode — exiting after single task");
            } else {
                tracing::info!("no pending tasks — exiting (--once mode)");
            }
            return Ok(());
        }

        if ran_task {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        } else {
            tracing::info!(poll = config.conductor.poll_interval_secs, "sleeping");
            tokio::time::sleep(std::time::Duration::from_secs(
                config.conductor.poll_interval_secs,
            ))
            .await;
        }
    }

    Ok(())
}

/// Execute one iteration of the conductor loop.
///
/// Returns `true` if a task was picked and executed, `false` if the queue was
/// empty (after optional discovery).
async fn run_one_iteration(
    config: &Config,
    queue_path: &Path,
    guardrails_path: &Path,
    gutter_map: &mut GutterMap,
    metrics: &mut Metrics,
) -> Result<bool, ConductorError> {
    // -- DISCOVER ---
    let mut queue = TaskQueue::load(queue_path).map_err(ConductorError::Queue)?;

    if queue.count_by_status(TaskStatus::Pending) == 0 && config.conductor.auto_discover {
        tracing::info!("queue empty — running discovery");
        run_discovery(&config.paths.discovery).await;
        queue = TaskQueue::load(queue_path).map_err(ConductorError::Queue)?;
    }

    // -- PICK ---
    let Some(task) = queue.next_pending().cloned() else {
        return Ok(false);
    };

    // Scope gate.
    if !config.security.is_project_allowed(&task.project) {
        tracing::error!(
            task_id = %task.id,
            project = %task.project,
            "project not in allowed_projects — skipping"
        );
        queue.set_status(&task.id, TaskStatus::Failed);
        queue.save(queue_path).map_err(ConductorError::Queue)?;
        return Ok(true);
    }

    tracing::info!(
        task_id = %task.id,
        title = %task.title,
        project = %task.project,
        priority = ?task.priority,
        "picked task"
    );

    // -- GUARDRAILS ---
    let guardrails_content = guardrails::read_guardrails(guardrails_path).unwrap_or_default();

    queue.set_status(&task.id, TaskStatus::InProgress);
    queue.save(queue_path).map_err(ConductorError::Queue)?;

    // -- EXECUTE ---
    let result = executor::execute_task(
        &task,
        &config.paths.projects_root,
        &config.budgets,
        &config.paths.logs,
        &guardrails_content,
    )
    .await;

    // Reload queue (another process may have modified it).
    let mut queue = TaskQueue::load(queue_path).map_err(ConductorError::Queue)?;

    match result {
        Ok(exec_result) => {
            update_metrics(metrics, &exec_result);
            handle_result(
                &exec_result,
                &task,
                &mut queue,
                gutter_map,
                guardrails_path,
                config,
                metrics,
            );
            if let Some(t) = queue.tasks.iter_mut().find(|t| t.id == task.id) {
                t.output_log = Some(exec_result.log_path.clone());
            }
        }
        Err(e) => {
            tracing::error!(task_id = %task.id, error = %e, "executor error");
            queue.set_status(&task.id, TaskStatus::Failed);
            metrics.failed = metrics.failed.saturating_add(1);
        }
    }

    queue.save(queue_path).map_err(ConductorError::Queue)?;
    export_metrics(config, metrics);
    Ok(true)
}

/// Handle an execution result: update status, detect gutter, append guardrails.
fn handle_result(
    exec_result: &executor::ExecutionResult,
    task: &super::queue::Task,
    queue: &mut TaskQueue,
    gutter_map: &mut GutterMap,
    guardrails_path: &Path,
    config: &Config,
    metrics: &mut Metrics,
) {
    if exec_result.success {
        tracing::info!(task_id = %task.id, elapsed = exec_result.elapsed_secs, "task completed");
        queue.set_status(&task.id, TaskStatus::Completed);
        gutter_map.remove(&task.id);
        return;
    }

    if exec_result.timed_out {
        tracing::warn!(task_id = %task.id, elapsed = exec_result.elapsed_secs, "task timed out");
        queue.set_status(&task.id, TaskStatus::Timeout);
        append_failure_sign(
            guardrails_path,
            task,
            "Timed out — task exceeded wall clock limit",
        );
        return;
    }

    // -- Gutter detection ---
    let sig = guardrails::error_signature(Path::new(&exec_result.log_path), 10);
    let sigs = gutter_map.entry(task.id.clone()).or_default();
    sigs.push(sig);

    if guardrails::is_guttered(sigs) {
        tracing::error!(task_id = %task.id, "GUTTERED — same error 3x, skipping");
        queue.set_status(&task.id, TaskStatus::Failed);
        metrics.guttered = metrics.guttered.saturating_add(1);
        let summary = guardrails::failure_summary(Path::new(&exec_result.log_path), 5);
        append_failure_sign(
            guardrails_path,
            task,
            &format!("Guttered (same error 3x): {summary}"),
        );
        return;
    }

    // -- Normal retry logic ---
    let retries = queue.increment_retries(&task.id);
    if retries >= config.budgets.max_retries {
        tracing::error!(task_id = %task.id, retries, "max retries exceeded");
        queue.set_status(&task.id, TaskStatus::Failed);
        let summary = guardrails::failure_summary(Path::new(&exec_result.log_path), 5);
        append_failure_sign(guardrails_path, task, &summary);
    } else {
        tracing::warn!(task_id = %task.id, retries, max = config.budgets.max_retries, "will retry");
        queue.set_status(&task.id, TaskStatus::Pending);
    }
}

/// Append a failure sign to the guardrails file.
fn append_failure_sign(path: &Path, task: &super::queue::Task, failure: &str) {
    let sign = guardrails::Sign {
        task_id: task.id.clone(),
        failure: failure.to_owned(),
        instruction: format!("Avoid the approach that caused: {failure}"),
        added: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = guardrails::append_sign(path, &sign) {
        tracing::warn!(error = %e, "failed to append guardrail sign");
    }
}

/// Write a heartbeat timestamp to disk.
fn write_heartbeat(path: &Path) {
    let now = chrono::Utc::now().to_rfc3339();
    if let Err(e) = std::fs::write(path, &now) {
        tracing::warn!(error = %e, "failed to write heartbeat");
    }
}

/// Update cumulative metrics from an execution result.
fn update_metrics(metrics: &mut Metrics, result: &executor::ExecutionResult) {
    metrics.total_duration_secs = metrics
        .total_duration_secs
        .saturating_add(result.elapsed_secs);

    if result.success {
        metrics.completed = metrics.completed.saturating_add(1);
    } else if result.timed_out {
        metrics.timed_out = metrics.timed_out.saturating_add(1);
    } else {
        metrics.failed = metrics.failed.saturating_add(1);
    }

    let total = metrics
        .completed
        .saturating_add(metrics.failed)
        .saturating_add(metrics.timed_out)
        .saturating_add(metrics.guttered);

    if total > 0 {
        #[allow(clippy::cast_precision_loss)]
        {
            metrics.avg_duration_secs = metrics.total_duration_secs as f64 / total as f64;
        }
    }
}

/// Export metrics to the JSON file.
fn export_metrics(config: &Config, metrics: &Metrics) {
    let mut metrics_out = Metrics {
        completed: metrics.completed,
        failed: metrics.failed,
        guttered: metrics.guttered,
        timed_out: metrics.timed_out,
        total_duration_secs: metrics.total_duration_secs,
        avg_duration_secs: metrics.avg_duration_secs,
        last_updated: chrono::Utc::now().to_rfc3339(),
    };
    // Ensure last_updated is set even if redundant.
    let _ = &mut metrics_out;

    match serde_json::to_string_pretty(&metrics_out) {
        Ok(json) => {
            let path = config.metrics_path();
            if let Err(e) = std::fs::write(&path, &json) {
                tracing::warn!(error = %e, "failed to write metrics");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to serialize metrics");
        }
    }
}

/// Run all discovery scripts in the discovery directory.
async fn run_discovery(discovery_dir: &Path) {
    let entries = match std::fs::read_dir(discovery_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(path = %discovery_dir.display(), error = %e, "cannot read discovery dir");
            return;
        }
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "sh") {
            tracing::info!(script = %path.display(), "running discovery script");
            match tokio::process::Command::new("bash")
                .arg(&path)
                .status()
                .await
            {
                Ok(s) if s.success() => {
                    tracing::info!(script = %path.display(), "discovery complete");
                }
                Ok(s) => {
                    tracing::warn!(script = %path.display(), code = s.code(), "discovery failed");
                }
                Err(e) => {
                    tracing::warn!(script = %path.display(), error = %e, "failed to run");
                }
            }
        }
    }
}

/// Conductor errors.
#[derive(Debug, thiserror::Error)]
pub enum ConductorError {
    /// Queue operation failed.
    #[error("queue error: {0}")]
    Queue(super::queue::QueueError),
}
