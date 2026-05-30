//! Hermes MCP bridge — spawns `hermes mcp serve` as a per-request stdio MCP client.
//!
//! Provides two gateway actions:
//! - `mcp_hermes_send_message` — relay a message to a platform (Telegram, Discord, etc.)
//! - `mcp_hermes_poll_events` — poll for operator responses with a configurable timeout
//!
//! # Configuration
//!
//! | Env var | Default | Purpose |
//! |---|---|---|
//! | `HERMES_MCP_ENABLED` | `false` | Must be `true` or `1` to activate |
//! | `HERMES_BINARY` | `hermes` | Path or name of the Hermes binary |
//!
//! When `HERMES_MCP_ENABLED` is absent or `false`, both actions return a
//! descriptive disabled message instead of an error — the platform degrades
//! gracefully without Hermes present.
//!
//! # Protocol
//!
//! Each call spawns a fresh `hermes mcp serve` subprocess, performs the MCP
//! initialize handshake, issues one `tools/call`, reads the result, and kills
//! the subprocess. Stateless per-request design — persistent connection pooling
//! is deferred to Phase 4+.

use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::text_result;
use crate::config::GatewayConfig;
use crate::error::GatewayError;

/// Timeout applied to each Hermes MCP tool call (stdio round-trip).
const HERMES_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Lightweight stdio MCP client for [`hermes mcp serve`].
///
/// [`hermes mcp serve`]: https://docs.hermes.ai/mcp
pub struct HermesMcpClient {
    /// Resolved binary name — from `HERMES_BINARY` or `"hermes"`.
    binary: String,
}

impl HermesMcpClient {
    /// Construct from environment variables.
    ///
    /// Returns `None` when `HERMES_MCP_ENABLED` is absent, `false`, or `0`.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let enabled = std::env::var("HERMES_MCP_ENABLED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(false);
        if !enabled {
            return None;
        }
        Some(Self {
            binary: std::env::var("HERMES_BINARY").unwrap_or_else(|_| "hermes".to_owned()),
        })
    }

    /// Spawn `hermes mcp serve`, perform the MCP handshake, call `tool_name`
    /// with `arguments`, read the result, and kill the subprocess.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::Subprocess`] if the process cannot be spawned.
    /// Returns [`GatewayError::McpProtocol`] on handshake or timeout failure.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value, GatewayError> {
        let mut child = tokio::process::Command::new(&self.binary)
            .arg("mcp")
            .arg("serve")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| GatewayError::Subprocess(format!("hermes mcp serve spawn failed: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| GatewayError::Internal("hermes stdin unavailable".to_owned()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| GatewayError::Internal("hermes stdout unavailable".to_owned()))?;

        let mut writer = tokio::io::BufWriter::new(stdin);
        let mut reader = BufReader::new(stdout).lines();

        // ── MCP initialize handshake ──────────────────────────────────────────
        let init = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "lightarchitects-gateway", "version": "0.1.0"}
            }
        });
        send_line(&mut writer, &init).await?;

        // Discard initialize response — we don't inspect capabilities for now.
        reader.next_line().await.ok();

        // Send initialized notification (required by MCP spec before tool calls).
        let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        send_line(&mut writer, &initialized).await?;

        // ── Tool call ─────────────────────────────────────────────────────────
        let call = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {"name": tool_name, "arguments": arguments}
        });
        send_line(&mut writer, &call).await?;

        // ── Read result with timeout ──────────────────────────────────────────
        tokio::time::timeout(HERMES_CALL_TIMEOUT, async {
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    // Match response to our tool call by id=2.
                    if v.get("id").and_then(Value::as_u64) == Some(2) {
                        return Ok(v);
                    }
                }
            }
            Err(GatewayError::McpProtocol {
                agent: "hermes".to_owned(),
                reason: "subprocess closed without returning a tool-call response".to_owned(),
            })
        })
        .await
        .map_err(|_| GatewayError::McpProtocol {
            agent: "hermes".to_owned(),
            reason: format!(
                "tool call '{}' timed out after {}s",
                tool_name,
                HERMES_CALL_TIMEOUT.as_secs()
            ),
        })?
    }

    /// Relay a message to a Hermes-managed platform.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError`] on spawn or MCP protocol failure.
    pub async fn send_message(&self, platform: &str, content: &str) -> Result<Value, GatewayError> {
        self.call_tool(
            "send_message",
            json!({"platform": platform, "content": content}),
        )
        .await
    }

    /// Poll for operator events from Hermes.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError`] on spawn or MCP protocol failure.
    pub async fn poll_events(&self, timeout_secs: u64) -> Result<Value, GatewayError> {
        self.call_tool("poll_events", json!({"timeout_secs": timeout_secs}))
            .await
    }
}

/// Write a JSON value as a newline-terminated line to the subprocess stdin.
async fn send_line(
    writer: &mut tokio::io::BufWriter<tokio::process::ChildStdin>,
    value: &Value,
) -> Result<(), GatewayError> {
    let line = format!("{value}\n");
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| GatewayError::McpProtocol {
            agent: "hermes".to_owned(),
            reason: format!("stdin write failed: {e}"),
        })?;
    writer.flush().await.map_err(|e| GatewayError::McpProtocol {
        agent: "hermes".to_owned(),
        reason: format!("stdin flush failed: {e}"),
    })
}

// ── Gateway action handlers ───────────────────────────────────────────────────

/// Gateway action: `mcp_hermes_send_message`.
///
/// params: `{ platform: string, content: string }`
///
/// When `HERMES_MCP_ENABLED` is not set, returns a graceful disabled message
/// rather than an error so the caller can distinguish "not configured" from
/// "configured but failed".
///
/// # Errors
///
/// Returns [`GatewayError`] on spawn or protocol failure when Hermes is active.
pub async fn run_send_message(
    params: Value,
    _config: &GatewayConfig,
) -> Result<Value, GatewayError> {
    let Some(client) = HermesMcpClient::from_env() else {
        return Ok(text_result(
            "hermes_mcp disabled — set HERMES_MCP_ENABLED=true and HERMES_BINARY=<path> to activate",
        ));
    };
    let platform = params["platform"].as_str().unwrap_or("telegram");
    let content = params["content"]
        .as_str()
        .ok_or(GatewayError::MissingParam("content"))?;
    let result = client.send_message(platform, content).await?;
    Ok(text_result(result.to_string()))
}

/// Gateway action: `mcp_hermes_poll_events`.
///
/// params: `{ timeout_secs?: u64 }` (default 30)
///
/// # Errors
///
/// Returns [`GatewayError`] on spawn or protocol failure when Hermes is active.
pub async fn run_poll_events(
    params: Value,
    _config: &GatewayConfig,
) -> Result<Value, GatewayError> {
    let Some(client) = HermesMcpClient::from_env() else {
        return Ok(text_result(
            "hermes_mcp disabled — set HERMES_MCP_ENABLED=true and HERMES_BINARY=<path> to activate",
        ));
    };
    let timeout_secs = params["timeout_secs"].as_u64().unwrap_or(30);
    let result = client.poll_events(timeout_secs).await?;
    Ok(text_result(result.to_string()))
}
