//! LVL8 Conductor — autonomous task execution loop embedded in the gateway.
//!
//! Provides the `conductor` subcommand group for the `lightarchitects` binary:
//!
//! ```text
//! lightarchitects conductor start              Start the conductor daemon
//! lightarchitects conductor stop               Stop the running daemon
//! lightarchitects conductor status             Show queue and heartbeat status
//! lightarchitects conductor add <title> ...    Add a task to the queue
//! lightarchitects conductor logs [task-id]     Tail task logs
//! lightarchitects conductor run                Run the loop (internal, used by start)
//! lightarchitects conductor --once             Execute one task and exit
//! lightarchitects conductor --discover         Run discovery scripts only
//! lightarchitects conductor --dry-run          Show next task without executing
//! ```

pub mod config;
mod executor;
pub mod guardrails;
mod loop_driver;
pub mod queue;

use crate::error::GatewayError;

/// Dispatch a conductor subcommand.
///
/// # Errors
///
/// Returns `GatewayError` if the subcommand fails.
pub async fn dispatch(args: &[String]) -> Result<(), GatewayError> {
    let conductor_config = config::Config::resolve()
        .map_err(|e| GatewayError::Conductor(format!("conductor config: {e}")))?;

    match args.first().map(String::as_str) {
        Some("start") => cmd_start(&conductor_config),
        Some("stop") => cmd_stop(&conductor_config),
        Some("status" | "--status") => {
            cmd_status(&conductor_config);
            Ok(())
        }
        Some("add") => cmd_add(&conductor_config, &args[1..]),
        Some("logs") => {
            cmd_logs(&conductor_config, args.get(1).map(String::as_str));
            Ok(())
        }
        Some("run") => {
            loop_driver::run(&conductor_config, false)
                .await
                .map_err(|e| GatewayError::Conductor(format!("conductor: {e}")))?;
            Ok(())
        }
        Some("--once") => {
            loop_driver::run(&conductor_config, true)
                .await
                .map_err(|e| GatewayError::Conductor(format!("conductor: {e}")))?;
            Ok(())
        }
        Some("--discover") => {
            discover_only(&conductor_config).await;
            Ok(())
        }
        Some("--dry-run") => {
            cmd_dry_run(&conductor_config);
            Ok(())
        }
        _ => {
            print_conductor_usage();
            Ok(())
        }
    }
}

// ── Subcommand implementations ──────────────────────────────────────────────

fn cmd_start(config: &config::Config) -> Result<(), GatewayError> {
    let pid_path = config.pid_path();

    if let Some(pid) = read_pid(&pid_path) {
        if is_process_alive(pid) {
            eprintln!("Conductor is already running (PID {pid})");
            return Err(GatewayError::Conductor("already running".into()));
        }
        let _ = std::fs::remove_file(&pid_path);
    }

    let exe = std::env::current_exe()
        .map_err(|e| GatewayError::Conductor(format!("cannot determine exe: {e}")))?;

    let log_dir = &config.paths.logs;
    let _ = std::fs::create_dir_all(log_dir);
    let log_path = log_dir.join("conductor-daemon.log");

    let log_file = std::fs::File::create(&log_path)
        .map_err(|e| GatewayError::Conductor(format!("cannot create log: {e}")))?;
    let stderr_file = log_file
        .try_clone()
        .map_err(|e| GatewayError::Conductor(format!("cannot clone log: {e}")))?;

    let child = std::process::Command::new(exe)
        .args(["conductor", "run"])
        .stdout(log_file)
        .stderr(stderr_file)
        .spawn();

    match child {
        Ok(child) => {
            let pid = child.id();
            let _ = std::fs::write(&pid_path, pid.to_string());
            println!("Conductor started (PID {pid})");
            println!("  Log:  {}", log_path.display());
            println!("  Stop: lightarchitects conductor stop");
            Ok(())
        }
        Err(e) => Err(GatewayError::Conductor(format!("failed to start: {e}"))),
    }
}

fn cmd_stop(config: &config::Config) -> Result<(), GatewayError> {
    let pid_path = config.pid_path();
    let Some(pid) = read_pid(&pid_path) else {
        eprintln!("Conductor is not running (no PID file)");
        return Err(GatewayError::Conductor("not running".into()));
    };

    if !is_process_alive(pid) {
        eprintln!("Conductor is not running (stale PID {pid})");
        let _ = std::fs::remove_file(&pid_path);
        return Ok(());
    }

    let result = std::process::Command::new("kill")
        .arg(pid.to_string())
        .status();

    match result {
        Ok(s) if s.success() => {
            println!("Conductor stopped (PID {pid})");
            let _ = std::fs::remove_file(&pid_path);
            Ok(())
        }
        _ => Err(GatewayError::Conductor(format!("failed to stop PID {pid}"))),
    }
}

fn cmd_add(config: &config::Config, args: &[String]) -> Result<(), GatewayError> {
    // Parse: add <title> --project <path> --prompt <instructions> [--priority high|medium|low]
    let mut title = None;
    let mut project = None;
    let mut prompt = None;
    let mut priority = "medium".to_owned();

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--project" => project = iter.next().cloned(),
            "--prompt" => prompt = iter.next().cloned(),
            "--priority" => {
                if let Some(p) = iter.next() {
                    priority.clone_from(p);
                }
            }
            _ if title.is_none() => title = Some(arg.clone()),
            _ => {}
        }
    }

    let title = title.ok_or(GatewayError::MissingParam("title"))?;
    let project = project.ok_or(GatewayError::MissingParam("--project"))?;
    let prompt = prompt.ok_or(GatewayError::MissingParam("--prompt"))?;

    if !config.security.is_project_allowed(&project) {
        return Err(GatewayError::Conductor(format!(
            "project '{project}' not in allowed_projects"
        )));
    }

    let title = sanitize_input(&title, 200);
    let prompt = sanitize_input(&prompt, 2000);

    let queue_path = &config.paths.queue;
    let mut q = queue::TaskQueue::load(queue_path)
        .map_err(|e| GatewayError::Conductor(format!("queue: {e}")))?;

    let priority = match priority.to_lowercase().as_str() {
        "high" => queue::Priority::High,
        "low" => queue::Priority::Low,
        _ => queue::Priority::Medium,
    };

    let id = format!("manual-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));

    q.tasks.push(queue::Task {
        id: id.clone(),
        title,
        project,
        prompt,
        status: queue::TaskStatus::Pending,
        source: "manual".into(),
        priority,
        added: Some(chrono::Utc::now()),
        started: None,
        finished: None,
        retries: 0,
        output_log: None,
    });

    q.save(queue_path)
        .map_err(|e| GatewayError::Conductor(format!("queue save: {e}")))?;

    println!("Task added: {id}");
    Ok(())
}

fn cmd_status(config: &config::Config) {
    let q = match queue::TaskQueue::load(&config.paths.queue) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("Failed to load queue: {e}");
            return;
        }
    };

    println!("Conductor Status");
    println!(
        "  Pending:     {}",
        q.count_by_status(queue::TaskStatus::Pending)
    );
    println!(
        "  In Progress: {}",
        q.count_by_status(queue::TaskStatus::InProgress)
    );
    println!(
        "  Completed:   {}",
        q.count_by_status(queue::TaskStatus::Completed)
    );
    println!(
        "  Failed:      {}",
        q.count_by_status(queue::TaskStatus::Failed)
    );

    let heartbeat_path = config.heartbeat_path();
    if let Ok(meta) = std::fs::metadata(&heartbeat_path) {
        if let Ok(modified) = meta.modified() {
            let age = modified.elapsed().unwrap_or_default().as_secs();
            if age < 120 {
                println!("  Heartbeat:   {age}s ago (healthy)");
            } else {
                println!("  Heartbeat:   {age}s ago (STALE)");
            }
        }
    } else {
        println!("  Heartbeat:   none (not running)");
    }

    let pid_path = config.pid_path();
    if let Some(pid) = read_pid(&pid_path) {
        if is_process_alive(pid) {
            println!("  Daemon:      running (PID {pid})");
        } else {
            println!("  Daemon:      not running (stale PID)");
        }
    } else {
        println!("  Daemon:      not running");
    }
}

fn cmd_logs(config: &config::Config, task_id: Option<&str>) {
    let log_dir = &config.paths.logs;

    if let Some(id) = task_id {
        if let Ok(q) = queue::TaskQueue::load(&config.paths.queue) {
            if let Some(task) = q.tasks.iter().find(|t| t.id == id) {
                if let Some(ref log_path) = task.output_log {
                    tail_file(log_path);
                    return;
                }
            }
        }
        eprintln!("No log found for task '{id}'");
        return;
    }

    let entries = match std::fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Cannot read log dir: {e}");
            return;
        }
    };

    let mut logs: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "log"))
        .collect();
    logs.sort_by_key(|e| {
        e.metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    if let Some(latest) = logs.last() {
        tail_file(&latest.path().display().to_string());
    } else {
        eprintln!("No log files in {}", log_dir.display());
    }
}

fn cmd_dry_run(config: &config::Config) {
    let q = match queue::TaskQueue::load(&config.paths.queue) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("Error: {e}");
            return;
        }
    };

    let Some(task) = q.next_pending() else {
        println!("No pending tasks.");
        return;
    };

    println!("Next task (dry run):");
    println!("  ID:      {}", task.id);
    println!("  Title:   {}", task.title);
    println!("  Project: {}", task.project);
    println!(
        "  Prompt:  {}...",
        &task.prompt[..task.prompt.len().min(100)]
    );
}

async fn discover_only(config: &config::Config) {
    let entries = match std::fs::read_dir(&config.paths.discovery) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!(error = %e, "cannot read discovery dir");
            return;
        }
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "sh") {
            tracing::info!(script = %path.display(), "running");
            let _ = tokio::process::Command::new("bash")
                .arg(&path)
                .status()
                .await;
        }
    }
}

fn print_conductor_usage() {
    println!("lightarchitects conductor — autonomous task loop");
    println!();
    println!("Commands:");
    println!("  start                  Start the conductor daemon");
    println!("  stop                   Stop the running daemon");
    println!("  status                 Show queue and heartbeat status");
    println!("  add <title> --project <path> --prompt <text>  Add a task");
    println!("  logs [task-id]         Tail task logs");
    println!("  --once                 Execute one task and exit");
    println!("  --discover             Run discovery scripts only");
    println!("  --dry-run              Show next task without executing");
}

// ── Utility ─────────────────────────────────────────────────────────────────

fn read_pid(path: &std::path::PathBuf) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn is_process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn tail_file(path: &str) {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(50);
            for line in &lines[start..] {
                println!("{line}");
            }
        }
        Err(e) => eprintln!("Cannot read {path}: {e}"),
    }
}

fn sanitize_input(input: &str, max_len: usize) -> String {
    let sanitized: String = input.chars().take(max_len).collect();
    let lower = sanitized.to_lowercase();
    if lower.contains("ignore") && lower.contains("previous")
        || lower.contains("disregard")
        || lower.contains("```")
    {
        return "[REJECTED: suspicious input]".to_owned();
    }
    sanitized
}
