//! `lightarchitects webshell` — local web GUI for the active coding agent.
//!
//! Three subcommands:
//! - `start`   — launch the webshell server (spawns the binary)
//! - `control`  — send a control command to a running webshell
//! - `status`   — check if the webshell server is running

use std::path::PathBuf;
use std::process::Command;

use crate::config::GatewayConfig;
use crate::error::GatewayError;

/// Webshell subcommands (parsed from args, not clap).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebshellCommand {
    /// Launch the webshell server.
    Start {
        /// Port to listen on.
        port: u16,
        /// Host command to invoke (e.g. "claude").
        host_cmd: String,
        /// Working directory for the spawned process.
        cwd: Option<PathBuf>,
    },
    /// Check if the webshell server is running.
    Status {
        /// Port to check.
        port: u16,
    },
    /// Send a control command to a running webshell.
    Control {
        /// Control command string.
        cmd: String,
    },
}

/// Execute a webshell subcommand.
///
/// # Errors
///
/// Returns [`GatewayError`] on spawn failure, HTTP error, or missing binary.
pub async fn execute(config: &GatewayConfig, args: &[String]) -> Result<(), GatewayError> {
    match args.first().map(String::as_str) {
        Some("start") => {
            let port = args
                .iter()
                .position(|a| a == "--port")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(8733);
            let host_cmd = args
                .iter()
                .position(|a| a == "--host-cmd")
                .and_then(|i| args.get(i + 1))
                .cloned()
                .unwrap_or_else(|| "claude".to_owned());
            let cwd = args
                .iter()
                .position(|a| a == "--cwd")
                .and_then(|i| args.get(i + 1))
                .map(std::path::PathBuf::from);

            start_server(config, port, &host_cmd, cwd.as_deref())
        }
        Some("status") => {
            let port = args
                .iter()
                .position(|a| a == "--port")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(8733);
            check_status(port).await
        }
        Some("control") => {
            let cmd = args
                .get(1)
                .cloned()
                .ok_or(GatewayError::MissingParam("control command"))?;
            send_control(&cmd).await
        }
        Some(other) => {
            eprintln!("Unknown webshell subcommand: {other}");
            eprintln!("Available: start, control, status");
            Err(GatewayError::UnknownTool(other.to_owned()))
        }
        None => start_server(config, 8733, "claude", None),
    }
}

fn start_server(
    config: &GatewayConfig,
    port: u16,
    host_cmd: &str,
    cwd: Option<&std::path::Path>,
) -> Result<(), GatewayError> {
    let binary = config.agents.get("webshell").map_or_else(
        || {
            let home = std::env::var_os("HOME").unwrap_or_default();
            let home_path = PathBuf::from(&home);
            home_path
                .join("lightarchitects")
                .join("webshell")
                .join("bin")
                .join("lightarchitects-webshell")
        },
        super::super::config::AgentConfig::binary_path,
    );

    let mut child = Command::new(&binary);
    child.arg("--port").arg(port.to_string());
    child.arg("--host-cmd").arg(host_cmd);
    if let Some(cwd_path) = cwd {
        child.arg("--cwd").arg(cwd_path);
    }

    let status = child.status().map_err(|e| GatewayError::SpawnFailed {
        agent: "webshell".to_owned(),
        reason: format!("failed to spawn webshell: {e}"),
    })?;

    if status.success() {
        Ok(())
    } else {
        Err(GatewayError::SpawnFailed {
            agent: "webshell".to_owned(),
            reason: format!("webshell exited with status: {status}"),
        })
    }
}

async fn send_control(cmd: &str) -> Result<(), GatewayError> {
    let port = std::env::var("LIGHTARCHITECTS_WEBSHELL_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8733);

    let token = resolve_token();
    let payload = serde_json::json!({"command": cmd});

    let url = format!("http://127.0.0.1:{port}/api/control");
    let client = reqwest::Client::new();
    let mut request = client.post(&url).json(&payload);

    if let Some(ref token_str) = token {
        request = request.bearer_auth(token_str);
    }

    let response = request
        .send()
        .await
        .map_err(|e| GatewayError::Internal(format!("control request failed: {e}")))?;

    if response.status().is_success() {
        println!("OK");
        Ok(())
    } else {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| String::from("(no body)"));
        Err(GatewayError::Internal(format!(
            "control API returned {status}: {body}"
        )))
    }
}

async fn check_status(port: u16) -> Result<(), GatewayError> {
    let url = format!("http://127.0.0.1:{port}/api/health");
    let client = reqwest::Client::new();

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| GatewayError::Internal(format!("status check failed: {e}")))?;

    if response.status().is_success() {
        let body = response
            .text()
            .await
            .map_err(|e| GatewayError::Internal(format!("failed to read response: {e}")))?;
        println!("running (port {port}) — {body}");
        Ok(())
    } else {
        Err(GatewayError::Internal(format!(
            "webshell returned status {}",
            response.status()
        )))
    }
}

/// Resolve the webshell auth token: env var → keyring → file.
fn resolve_token() -> Option<String> {
    // 1. Environment variable
    if let Ok(token) = std::env::var("LIGHTARCHITECTS_WEBSHELL_TOKEN") {
        if !token.is_empty() {
            return Some(token);
        }
    }

    // 2. OS keyring
    if let Ok(entry) = keyring::Entry::new("lightarchitects", "webshell-token") {
        if let Ok(token) = entry.get_password() {
            if !token.is_empty() {
                return Some(token);
            }
        }
    }

    // 3. File
    if let Some(path) = lightarchitects::core::paths::root() {
        let token_path = path.join("webshell").join(".token");
        if let Ok(token) = std::fs::read_to_string(&token_path) {
            let trimmed = token.trim().to_owned();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }

    None
}
