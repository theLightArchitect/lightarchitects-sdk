//! MCP Binary Process Pool — manages persistent stdio connections to sibling binaries.
//!
//! Each sibling runs as a child process with stdin/stdout piped for JSON-RPC communication.
//! The pool spawns binaries on startup, monitors health, and respawns on crash.
//! All I/O operations have a 30-second timeout to prevent indefinite hangs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::Mutex;

use super::compat::{JsonRpcRequestExt, JsonRpcResponseExt};
use lightarchitects::core::jsonrpc::{JsonRpcRequest, JsonRpcResponse};

/// Timeout for any single MCP call (including lock acquisition + I/O).
const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum response body size for Content-Length framed responses (10 MB).
const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

/// Minimum interval between respawn attempts per sibling (5 seconds).
const RESPAWN_COOLDOWN: Duration = Duration::from_secs(5);

/// Valid EVA tool names — rejects unknown tools at the gateway level.
const VALID_EVA_TOOLS: &[&str] = &[
    "speak",
    "visualize",
    "ideate",
    "memory",
    "build",
    "bible",
    "research",
    "secure",
    "teach",
];

/// MCP framing protocol — varies per sibling binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpFraming {
    /// Newline-delimited JSON (SOUL, CORSO, EVA, QUANTUM)
    Newline,
    /// Content-Length header framing (SERAPH)
    ContentLength,
}

/// A managed MCP binary process.
struct McpProcess {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout_reader: BufReader<ChildStdout>,
    framing: McpFraming,
}

/// Pool of MCP binary processes, one per sibling.
///
/// Immutable after `spawn_all()` — the `HashMap` never changes. Individual processes
/// are behind `Arc<Mutex<McpProcess>>` for respawn. No outer lock needed.
pub struct McpPool {
    processes: HashMap<String, Arc<Mutex<McpProcess>>>,
    binary_paths: HashMap<String, PathBuf>,
    /// Per-sibling respawn cooldown — prevents respawn storms.
    last_respawn: Mutex<HashMap<String, Instant>>,
}

impl McpPool {
    /// Create a new pool with sibling binary paths.
    #[must_use]
    pub fn new(paths: HashMap<String, PathBuf>) -> Self {
        Self {
            processes: HashMap::new(),
            binary_paths: paths,
            last_respawn: Mutex::new(HashMap::new()),
        }
    }

    /// Spawn all sibling processes. Skips binaries that don't exist.
    ///
    /// # Errors
    /// Returns error only if a critical spawn failure occurs.
    pub async fn spawn_all(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for (name, path) in &self.binary_paths {
            if !path.exists() {
                tracing::warn!(sibling = %name, path = %path.display(), "Binary not found, skipping");
                continue;
            }
            match Self::spawn_one(name, path).await {
                Ok(process) => {
                    self.processes
                        .insert(name.clone(), Arc::new(Mutex::new(process)));
                    tracing::info!(sibling = %name, "MCP binary spawned and initialized");
                }
                Err(e) => {
                    tracing::error!(sibling = %name, error = %e, "Failed to spawn MCP binary");
                }
            }
        }
        Ok(())
    }

    /// MCP subcommand required by specific siblings.
    ///
    /// Most siblings run as MCP servers by default (bare binary = stdio JSON-RPC).
    /// QUANTUM requires an explicit `mcp-server` subcommand.
    fn mcp_subcommand(name: &str) -> Option<&'static str> {
        match name {
            "quantum" => Some("mcp-server"),
            _ => None,
        }
    }

    /// Spawn a single sibling process with stderr logging and MCP init handshake.
    async fn spawn_one(
        name: &str,
        path: &Path,
    ) -> Result<McpProcess, Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(sibling = %name, path = %path.display(), "Spawning MCP binary");

        let mut cmd = Command::new(path);
        if let Some(subcmd) = Self::mcp_subcommand(name) {
            cmd.arg(subcmd);
            tracing::info!(sibling = %name, subcommand = subcmd, "Using MCP subcommand");
        }
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().ok_or("Failed to capture stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;

        // Spawn stderr logging task (capped to 64KB per line to prevent OOM)
        if let Some(stderr) = child.stderr.take() {
            let sibling_name = name.to_owned();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            if line.len() > 65_536 {
                                tracing::error!(sibling = %sibling_name, len = line.len(), "stderr line too large, truncating");
                                line.truncate(65_536);
                            }
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                tracing::warn!(sibling = %sibling_name, "{trimmed}");
                            }
                        }
                    }
                }
            });
        }

        // All siblings (including SERAPH) respond with plain newline-delimited JSON
        // via McpServerLoop::write_value — Content-Length framing is only used on
        // the SERAPH server's READ side (auto-detect). Use Newline for all spawned processes.
        let framing = McpFraming::Newline;

        let mut process = McpProcess {
            child,
            stdin,
            stdout_reader: BufReader::new(stdout),
            framing,
        };

        // Init handshake with timeout — prevents hanging on unresponsive binaries
        tokio::time::timeout(MCP_CALL_TIMEOUT, Self::send_initialize(&mut process, name))
            .await
            .map_err(|_| format!("MCP init handshake timed out for '{name}'"))??;
        Ok(process)
    }

    /// Send the MCP initialize handshake and read the response.
    async fn send_initialize(
        process: &mut McpProcess,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let init_request = JsonRpcRequest::new(
            0,
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "arena", "version": "0.1.0"}
            })),
        );
        let init_json = serde_json::to_string(&init_request).unwrap_or_default();
        Self::write_framed(process, &init_json).await?;
        let response = Self::read_framed(process).await?;
        let preview: String = response.chars().take(80).collect();
        tracing::info!(sibling = %name, framing = ?process.framing, "MCP initialized: {preview}");
        Ok(())
    }

    /// Write a JSON-RPC message with the correct framing protocol.
    async fn write_framed(
        process: &mut McpProcess,
        json: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match process.framing {
            McpFraming::Newline => {
                process.stdin.write_all(json.as_bytes()).await?;
                process.stdin.write_all(b"\n").await?;
            }
            McpFraming::ContentLength => {
                let header = format!("Content-Length: {}\r\n\r\n", json.len());
                process.stdin.write_all(header.as_bytes()).await?;
                process.stdin.write_all(json.as_bytes()).await?;
            }
        }
        process.stdin.flush().await?;
        Ok(())
    }

    /// Read a JSON-RPC response with the correct framing protocol.
    async fn read_framed(
        process: &mut McpProcess,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        match process.framing {
            McpFraming::Newline => {
                let line =
                    Self::read_line_limited(&mut process.stdout_reader, MAX_RESPONSE_BYTES).await?;
                let trimmed = line.trim().to_owned();
                if trimmed.is_empty() {
                    return Err("Empty response from MCP binary".into());
                }
                Ok(trimmed)
            }
            McpFraming::ContentLength => Self::read_content_length(process).await,
        }
    }

    /// Read a single line with a hard size cap, rejecting before full allocation.
    ///
    /// Unlike `read_line` (which reads the entire line into memory before any
    /// check), this reads byte-by-byte from the internal buffer, stopping as
    /// soon as either a newline is found or `max_bytes` is exceeded.  This
    /// prevents a malicious or malfunctioning sibling from causing unbounded
    /// memory allocation.
    async fn read_line_limited(
        reader: &mut BufReader<ChildStdout>,
        max_bytes: usize,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut buf = Vec::with_capacity(max_bytes.min(8192));
        loop {
            let available = reader.fill_buf().await?;
            if available.is_empty() {
                // EOF before newline — return what we have if non-empty.
                if buf.is_empty() {
                    return Err("EOF: no data from MCP binary".into());
                }
                break;
            }
            // Scan the buffered chunk for a newline.
            if let Some(newline_pos) = available.iter().position(|&b| b == b'\n') {
                let to_copy = newline_pos + 1; // include the newline
                if buf.len().saturating_add(to_copy) > max_bytes {
                    return Err(
                        format!("Response too large: >{max_bytes} bytes before newline").into(),
                    );
                }
                buf.extend_from_slice(&available[..to_copy]);
                reader.consume(to_copy);
                break;
            }
            // No newline in this chunk — consume it all if within budget.
            let chunk_len = available.len();
            if buf.len().saturating_add(chunk_len) > max_bytes {
                return Err(
                    format!("Response too large: >{max_bytes} bytes before newline").into(),
                );
            }
            buf.extend_from_slice(available);
            reader.consume(chunk_len);
        }
        String::from_utf8(buf).map_err(|e| format!("MCP response is not valid UTF-8: {e}").into())
    }

    /// Read a Content-Length framed response with size cap.
    async fn read_content_length(
        process: &mut McpProcess,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut header = String::new();
        process.stdout_reader.read_line(&mut header).await?;
        let mut empty = String::new();
        process.stdout_reader.read_line(&mut empty).await?;
        let content_len: usize = header
            .strip_prefix("Content-Length: ")
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        if content_len == 0 {
            return Err("No Content-Length in response".into());
        }
        if content_len > MAX_RESPONSE_BYTES {
            return Err(format!(
                "Response too large: {content_len} bytes (max {MAX_RESPONSE_BYTES})"
            )
            .into());
        }
        let mut body = vec![0u8; content_len];
        tokio::io::AsyncReadExt::read_exact(&mut process.stdout_reader, &mut body).await?;
        Ok(String::from_utf8_lossy(&body).into_owned())
    }

    /// Send a JSON-RPC request with timeout and auto-respawn on failure.
    ///
    /// # Errors
    /// Returns error if both the initial call and retry after respawn fail.
    #[tracing::instrument(skip(self, request), fields(sibling, method = %request.method))]
    pub async fn call(
        &self,
        sibling: &str,
        request: &JsonRpcRequest,
    ) -> Result<JsonRpcResponse, Box<dyn std::error::Error + Send + Sync>> {
        match self.try_call(sibling, request).await {
            Ok(response) => Ok(response),
            Err(e) => {
                tracing::warn!(sibling = %sibling, error = %e, "MCP call failed, attempting respawn");
                if let Err(re) = self.respawn(sibling).await {
                    tracing::error!(sibling = %sibling, error = %re, "Respawn failed");
                    return Err(e);
                }
                self.try_call(sibling, request).await
            }
        }
    }

    /// Attempt a single MCP call with 30-second timeout.
    async fn try_call(
        &self,
        sibling: &str,
        request: &JsonRpcRequest,
    ) -> Result<JsonRpcResponse, Box<dyn std::error::Error + Send + Sync>> {
        let process = self
            .processes
            .get(sibling)
            .ok_or_else(|| format!("Sibling '{sibling}' not available"))?;

        let result = tokio::time::timeout(MCP_CALL_TIMEOUT, async {
            let mut proc = process.lock().await;
            let request_json = serde_json::to_string(request)?;
            Self::write_framed(&mut proc, &request_json).await?;
            let response_str = Self::read_framed(&mut proc).await?;
            let response: JsonRpcResponse = serde_json::from_str(&response_str)?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(response)
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => {
                Err(format!("MCP call to '{sibling}' timed out after {MCP_CALL_TIMEOUT:?}").into())
            }
        }
    }

    /// List all sibling names registered in the pool.
    #[must_use]
    pub fn sibling_names(&self) -> Vec<String> {
        self.binary_paths.keys().cloned().collect()
    }

    /// Check whether a sibling process is still running.
    ///
    /// Returns `false` if the process has exited, was never spawned, or the
    /// sibling name is unknown. Uses non-blocking `try_wait` — does not block
    /// the calling task.
    pub async fn is_alive(&self, sibling: &str) -> bool {
        let Some(process) = self.processes.get(sibling) else {
            return false;
        };
        let mut proc = process.lock().await;
        // try_wait returns Ok(Some(status)) if exited, Ok(None) if still running
        matches!(proc.child.try_wait(), Ok(None))
    }

    /// Kill and respawn a crashed sibling (rate-limited to prevent storms).
    ///
    /// Public so the supervisor module can trigger respawns proactively.
    pub async fn respawn(
        &self,
        sibling: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Respawn cooldown check
        {
            let mut last = self.last_respawn.lock().await;
            if let Some(prev) = last.get(sibling) {
                if prev.elapsed() < RESPAWN_COOLDOWN {
                    return Err(format!("Respawn cooldown for '{sibling}'").into());
                }
            }
            last.insert(sibling.to_owned(), Instant::now());
        }

        let process = self
            .processes
            .get(sibling)
            .ok_or_else(|| format!("Sibling '{sibling}' not in pool"))?;
        let path = self
            .binary_paths
            .get(sibling)
            .ok_or_else(|| format!("No binary path for '{sibling}'"))?;

        let mut proc = process.lock().await;
        drop(proc.child.kill().await);
        drop(tokio::time::timeout(Duration::from_secs(5), proc.child.wait()).await);

        *proc = Self::spawn_one(sibling, path).await?;
        tracing::info!(sibling = %sibling, "MCP binary respawned successfully");
        Ok(())
    }

    /// Route a tool call to the correct sibling based on tool name.
    pub fn resolve_sibling(tool_name: &str) -> Option<&'static str> {
        match tool_name {
            "corsoTools" => Some("corso"),
            "soulTools" => Some("soul"),
            "qsTools" => Some("quantum"),
            "penTools" => Some("seraph"),
            t if VALID_EVA_TOOLS.contains(&t) => Some("eva"),
            _ => None,
        }
    }

    /// Check health of all siblings (no PIDs exposed).
    pub async fn health(&self) -> HashMap<String, SiblingHealth> {
        let mut status = HashMap::new();
        for (name, process) in &self.processes {
            let proc = process.lock().await;
            let connected = proc.child.id().is_some();
            status.insert(
                name.clone(),
                SiblingHealth {
                    status: if connected {
                        "connected"
                    } else {
                        "disconnected"
                    },
                },
            );
        }
        for name in self.binary_paths.keys() {
            status.entry(name.clone()).or_insert(SiblingHealth {
                status: "disconnected",
            });
        }
        status
    }
}

/// Health status of a sibling binary (PIDs intentionally omitted).
#[derive(Debug, serde::Serialize)]
pub struct SiblingHealth {
    pub status: &'static str,
}

/// Validate that an MCP action name is a safe identifier.
///
/// Actions are short `[a-z0-9_-]` identifiers. Rejecting unusual characters
/// prevents injection via crafted action names before they reach sibling binaries.
fn validate_action(action: &str) -> Result<(), String> {
    if action.is_empty() || action.len() > 64 {
        return Err(format!(
            "Action name length invalid (got {} chars, max 64)",
            action.len()
        ));
    }
    if !action
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(format!(
            "Action '{action}' contains invalid characters (must be [a-z0-9_-])"
        ));
    }
    Ok(())
}

/// Route a REST request to the appropriate MCP tool call.
pub fn rest_to_jsonrpc(
    sibling: &str,
    action: &str,
    params: Value,
    request_id: u64,
) -> Result<(String, JsonRpcRequest), String> {
    validate_action(action)?;
    let (tool_name, arguments) = match sibling {
        "corso" => (
            "corsoTools",
            serde_json::json!({"action": action, "params": params}),
        ),
        "soul" => (
            "soulTools",
            serde_json::json!({"action": action, "params": params}),
        ),
        "quantum" => (
            "qsTools",
            serde_json::json!({"action": action, "params": params}),
        ),
        "seraph" => (
            "penTools",
            serde_json::json!({"action": action, "params": params}),
        ),
        "eva" => {
            if !VALID_EVA_TOOLS.contains(&action) {
                return Err(format!("Unknown EVA tool: '{action}'"));
            }
            return Ok((
                "eva".into(),
                JsonRpcRequest::tools_call(request_id, action, params),
            ));
        }
        _ => return Err(format!("Unknown sibling: {sibling}")),
    };

    Ok((
        sibling.into(),
        JsonRpcRequest::tools_call(request_id, tool_name, arguments),
    ))
}
