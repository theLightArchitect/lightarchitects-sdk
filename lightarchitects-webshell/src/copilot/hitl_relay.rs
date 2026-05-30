//! HITL relay — forwards approval requests to the operator via Hermes MCP.
//!
//! When `HermesMcpConfig.enabled` is true, the approval gate sends a formatted
//! message to the operator (default platform: Telegram) and waits up to
//! [`RELAY_POLL_TIMEOUT`] seconds for a response. Operator replies of
//! `"approve"` / `"yes"` / `"ok"` map to [`ApprovalDecision::Approved`]; any
//! other non-empty reply maps to [`ApprovalDecision::Denied`]. No reply within
//! the timeout maps to [`ApprovalDecision::Timeout`].
//!
//! When Hermes is not configured (`enabled: false`), [`relay_hitl_approval`]
//! returns `ApprovalDecision::Timeout` immediately so callers can fall back to
//! the standard UI approval flow.

use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::config::HermesMcpConfig;

/// Default poll timeout for operator HITL responses.
pub const RELAY_POLL_TIMEOUT: Duration = Duration::from_secs(30);

/// MCP handshake + tool-call timeout for a single Hermes stdio round-trip.
const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(10);

// ── Decision type ─────────────────────────────────────────────────────────────

/// The operator's HITL decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// Operator explicitly approved the action.
    Approved,
    /// Operator explicitly denied the action.
    Denied,
    /// No operator response within [`RELAY_POLL_TIMEOUT`].
    Timeout,
}

// ── Hermes MCP client (webshell-local) ────────────────────────────────────────

/// Stdio MCP client for `hermes mcp serve`.
///
/// Each call spawns a fresh subprocess (stateless per-request).
/// Persistent connection pooling is deferred to Phase 4+.
pub struct HermesMcpClient {
    /// Path or name of the Hermes binary.
    binary: String,
    /// Per-call MCP round-trip timeout.
    timeout: Duration,
}

impl HermesMcpClient {
    /// Construct from [`HermesMcpConfig`].
    ///
    /// Returns `None` when `config.enabled` is false.
    #[must_use]
    pub fn from_config(config: &HermesMcpConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        Some(Self {
            binary: std::env::var("HERMES_BINARY").unwrap_or_else(|_| "hermes".to_owned()),
            timeout: MCP_CALL_TIMEOUT,
        })
    }

    /// Override timeout — used in tests to avoid 10s waits.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Spawn `hermes mcp serve`, handshake, call `tool_name`, and return result.
    ///
    /// # Errors
    ///
    /// Returns a descriptive string on spawn failure, write error, or timeout.
    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value, String> {
        let mut child = tokio::process::Command::new(&self.binary)
            .arg("mcp")
            .arg("serve")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("hermes mcp serve spawn failed: {e}"))?;

        let stdin = child.stdin.take().ok_or("hermes stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("hermes stdout unavailable")?;

        let mut writer = tokio::io::BufWriter::new(stdin);
        let mut reader = BufReader::new(stdout).lines();

        // MCP initialize handshake
        let init = json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "lightarchitects-webshell", "version": "0.1.0"}
            }
        });
        write_line(&mut writer, &init).await?;
        reader.next_line().await.ok(); // discard initialize response

        let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        write_line(&mut writer, &initialized).await?;

        // Tool call
        let call = json!({
            "jsonrpc": "2.0", "id": 2,
            "method": "tools/call",
            "params": {"name": tool_name, "arguments": arguments}
        });
        write_line(&mut writer, &call).await?;

        // Read result with timeout
        tokio::time::timeout(self.timeout, async {
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    if v.get("id").and_then(Value::as_u64) == Some(2) {
                        return Ok(v);
                    }
                }
            }
            Err("hermes: no tool-call response received".to_owned())
        })
        .await
        .map_err(|_| format!("hermes: tool call '{tool_name}' timed out"))?
    }

    /// Send an operator message via Hermes.
    ///
    /// # Errors
    ///
    /// Returns a descriptive string on MCP protocol failure.
    pub async fn send_message(&self, platform: &str, content: &str) -> Result<Value, String> {
        self.call_tool(
            "send_message",
            json!({"platform": platform, "content": content}),
        )
        .await
    }

    /// Poll for operator events.
    ///
    /// # Errors
    ///
    /// Returns a descriptive string on MCP protocol failure or timeout.
    pub async fn poll_events(&self, timeout_secs: u64) -> Result<Value, String> {
        self.call_tool("poll_events", json!({"timeout_secs": timeout_secs}))
            .await
    }
}

async fn write_line(
    writer: &mut tokio::io::BufWriter<tokio::process::ChildStdin>,
    value: &Value,
) -> Result<(), String> {
    let line = format!("{value}\n");
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("hermes stdin write: {e}"))?;
    writer
        .flush()
        .await
        .map_err(|e| format!("hermes stdin flush: {e}"))
}

// ── Public relay API ──────────────────────────────────────────────────────────

/// Format a HITL approval request message suitable for sending to an operator.
#[must_use]
pub fn format_approval_request(approval_text: &str, build_id: &str) -> String {
    format!(
        "[LA HITL] Build `{build_id}` requires approval:\n\n{approval_text}\n\nReply `approve` to allow or `deny` to block."
    )
}

/// Parse an operator response text into an [`ApprovalDecision`].
#[must_use]
pub fn parse_operator_response(response: &str) -> ApprovalDecision {
    match response.trim().to_lowercase().as_str() {
        "approve" | "yes" | "ok" | "allow" | "y" => ApprovalDecision::Approved,
        _ if response.trim().is_empty() => ApprovalDecision::Timeout,
        _ => ApprovalDecision::Denied,
    }
}

/// Relay a HITL approval request via Hermes and await the operator's decision.
///
/// Returns [`ApprovalDecision::Timeout`] immediately when `client` is `None`
/// (i.e., Hermes is not configured) so callers can fall back to the UI flow.
///
/// # Errors
///
/// Returns a descriptive string when the Hermes MCP protocol fails (spawn,
/// write, or protocol error). A timeout during polling is NOT an error — it
/// maps to [`ApprovalDecision::Timeout`].
pub async fn relay_hitl_approval(
    approval_text: &str,
    build_id: &str,
    client: &HermesMcpClient,
) -> Result<ApprovalDecision, String> {
    let message = format_approval_request(approval_text, build_id);

    // Send approval request to operator
    client.send_message("telegram", &message).await?;

    // Poll for operator response; timeout → Timeout decision (not an error)
    match client.poll_events(RELAY_POLL_TIMEOUT.as_secs()).await {
        Ok(response) => {
            let text = response["result"]
                .as_str()
                .or_else(|| response["content"][0]["text"].as_str())
                .unwrap_or("");
            Ok(parse_operator_response(text))
        }
        Err(e) if e.contains("timed out") => Ok(ApprovalDecision::Timeout),
        Err(e) => Err(e),
    }
}
