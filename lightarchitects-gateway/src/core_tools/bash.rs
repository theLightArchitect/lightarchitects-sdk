//! `lightarchitects_bash` — execute a shell command and return its output.

use serde_json::{Value, json};
use tokio::process::Command;

use crate::error::GatewayError;

/// Default command timeout in milliseconds (120 seconds).
const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// Execute `lightarchitects_bash`.
///
/// # Parameters (JSON object)
/// - `command` (string, required): shell command to run.
/// - `timeout_ms` (integer, optional, default 120 000): abort after this many ms.
/// - `cwd` (string, optional): working directory for the command.
///
/// Non-zero exit codes are **not** errors — the output is returned with the exit
/// code embedded so callers can inspect it.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] when `command` is absent, and
/// [`GatewayError::Subprocess`] when the process cannot be spawned or waited on.
pub async fn run(params: Value) -> Result<Value, GatewayError> {
    let command = params["command"]
        .as_str()
        .ok_or(GatewayError::MissingParam("command"))?;
    let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(DEFAULT_TIMEOUT_MS);
    let cwd = params["cwd"].as_str();

    let mut sh = Command::new("sh");
    sh.arg("-c").arg(command);
    sh.stdout(std::process::Stdio::piped());
    sh.stderr(std::process::Stdio::piped());
    if let Some(dir) = cwd {
        sh.current_dir(dir);
    }

    let child = sh
        .spawn()
        .map_err(|e| GatewayError::Subprocess(format!("spawn failed: {e}")))?;

    let timeout = std::time::Duration::from_millis(timeout_ms);
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| GatewayError::Subprocess(format!("timed out after {timeout_ms}ms")))?
        .map_err(|e| GatewayError::Subprocess(format!("wait failed: {e}")))?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    let combined = if stderr.is_empty() {
        stdout.clone()
    } else if stdout.is_empty() {
        stderr.clone()
    } else {
        format!("{stdout}\n{stderr}")
    };

    let text = serde_json::to_string(&json!({
        "exit_code": exit_code,
        "output": combined
    }))?;

    Ok(json!({
        "content": [{"type": "text", "text": text}]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn echo_succeeds() {
        let result = run(json!({"command": "echo hello"})).await.expect("run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("hello"));
        assert!(text.contains("\"exit_code\":0"));
    }

    #[tokio::test]
    async fn nonzero_exit_is_not_an_error() {
        let result = run(json!({"command": "exit 42"})).await.expect("run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"exit_code\":42"));
    }

    #[tokio::test]
    async fn missing_command_is_error() {
        let result = run(json!({})).await;
        assert!(result.is_err());
    }
}
