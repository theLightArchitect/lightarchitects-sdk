//! MCP server auto-discovery and tool registry.
//!
//! Connects to arbitrary MCP servers via stdio or HTTP transport, performs
//! the MCP initialize handshake, calls `tools/list` with pagination support,
//! and populates a [`ToolRegistry`] with discovered tool schemas.
//!
//! [`ToolRegistry`]: crate::discovery::ToolRegistry

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use lightarchitects_core::action::ToolInfo;
use lightarchitects_core::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use crate::config::{ServerConfig, TransportType};

/// Maximum bytes read from a single stdio line (8 MiB).
/// Prevents a rogue MCP server from consuming unbounded memory.
const MAX_LINE_BYTES: u64 = 8 * 1024 * 1024;

/// Errors that can occur during MCP server discovery.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// Failed to spawn the MCP server process.
    #[error("failed to spawn server '{name}': {source}")]
    SpawnFailed {
        /// Server name from config.
        name: String,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Failed to connect to HTTP MCP server.
    #[error("failed to connect to server '{name}' at {url}: {source}")]
    HttpConnectFailed {
        /// Server name from config.
        name: String,
        /// Server URL.
        url: String,
        /// Underlying reqwest error.
        source: reqwest::Error,
    },
    /// MCP initialize handshake failed.
    #[error("initialize handshake failed for '{name}': {reason}")]
    HandshakeFailed {
        /// Server name.
        name: String,
        /// Failure reason.
        reason: String,
    },
    /// `tools/list` call failed.
    #[error("tools/list failed for '{name}': {reason}")]
    ToolsListFailed {
        /// Server name.
        name: String,
        /// Failure reason.
        reason: String,
    },
    /// Transport I/O error.
    #[error("transport I/O error for '{name}': {source}")]
    TransportIo {
        /// Server name.
        name: String,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// JSON serialization/deserialization error.
    #[error("JSON error for '{name}': {source}")]
    Json {
        /// Server name.
        name: String,
        /// Underlying serde error.
        source: serde_json::Error,
    },
    /// Server process exited unexpectedly.
    #[error("server '{name}' exited unexpectedly")]
    ServerExited {
        /// Server name.
        name: String,
    },
    /// Schema cache I/O error.
    #[error("schema cache error: {0}")]
    CacheIo(#[from] std::io::Error),
    /// Schema cache deserialization error.
    #[error("schema cache parse error: {0}")]
    CacheParse(#[from] serde_json::Error),
}

/// Registry of discovered tools, keyed by server name.
///
/// Provides lookup by server name and by tool name (across all servers).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolRegistry {
    /// Tools grouped by the server that advertises them.
    servers: HashMap<String, Vec<ToolInfo>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register tools discovered from a named server.
    pub fn register(&mut self, server_name: String, tools: Vec<ToolInfo>) {
        self.servers.insert(server_name, tools);
    }

    /// Get all tools for a specific server.
    #[must_use]
    pub fn tools_for_server(&self, server_name: &str) -> Option<&[ToolInfo]> {
        self.servers.get(server_name).map(Vec::as_slice)
    }

    /// Find which server owns a tool by name.
    ///
    /// Returns `(server_name, tool_info)` or `None` if the tool is not found.
    #[must_use]
    pub fn find_tool(&self, tool_name: &str) -> Option<(&str, &ToolInfo)> {
        for (server, tools) in &self.servers {
            for tool in tools {
                if tool.name == tool_name {
                    return Some((server.as_str(), tool));
                }
            }
        }
        None
    }

    /// Get all tools across all servers as a flat list.
    #[must_use]
    pub fn all_tools(&self) -> Vec<(&str, &ToolInfo)> {
        let mut all = Vec::new();
        for (server, tools) in &self.servers {
            for tool in tools {
                all.push((server.as_str(), tool));
            }
        }
        all
    }

    /// Total number of tools across all servers.
    #[must_use]
    pub fn tool_count(&self) -> usize {
        self.servers.values().map(Vec::len).sum()
    }

    /// Number of registered servers.
    #[must_use]
    pub fn server_count(&self) -> usize {
        self.servers.len()
    }

    /// Server names.
    pub fn server_names(&self) -> impl Iterator<Item = &str> {
        self.servers.keys().map(String::as_str)
    }

    /// Save the registry to a JSON cache file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written or serialized.
    pub fn save_cache(&self, path: &Path) -> Result<(), DiscoveryError> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load the registry from a JSON cache file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or deserialized.
    pub fn load_cache(path: &Path) -> Result<Self, DiscoveryError> {
        let json = std::fs::read_to_string(path)?;
        let registry: Self = serde_json::from_str(&json)?;
        Ok(registry)
    }
}

// ── Stdio Arena Transport ───────────────────────────────────────────────────

/// Inner state for a stdio connection to an MCP server.
///
/// On drop, the child process is killed and waited to prevent zombie processes.
pub(crate) struct StdioConnection {
    /// Child process handle — killed on drop.
    child: Child,
    /// Write handle to stdin.
    stdin: ChildStdin,
    /// Buffered read handle to stdout.
    stdout: BufReader<ChildStdout>,
    /// Next JSON-RPC request ID.
    next_id: u64,
}

impl Drop for StdioConnection {
    fn drop(&mut self) {
        // Best-effort kill — ignore errors (process may have already exited).
        let _ = self.child.start_kill();
    }
}

/// Spawn an MCP server process and perform the initialize handshake.
///
/// # Errors
///
/// Returns [`DiscoveryError`] if the process cannot be spawned or the
/// handshake fails.
pub(crate) async fn connect_stdio(
    config: &ServerConfig,
) -> Result<Mutex<StdioConnection>, DiscoveryError> {
    let command_str = config.command.as_deref().unwrap_or_default();
    // Note: split_whitespace cannot handle binary paths containing spaces.
    // This is a known limitation of the string-based command format.
    // Users with space-containing paths should use a wrapper script.
    // Command::new() uses execve(2) directly — no shell interpolation occurs.
    let parts: Vec<&str> = command_str.split_whitespace().collect();
    if parts.is_empty() {
        return Err(DiscoveryError::SpawnFailed {
            name: config.name.clone(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty command"),
        });
    }

    let program = parts[0];
    let args = &parts[1..];

    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());

    for (key, value) in &config.env {
        cmd.env(key, value);
    }

    let mut child = cmd.spawn().map_err(|e| DiscoveryError::SpawnFailed {
        name: config.name.clone(),
        source: e,
    })?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| DiscoveryError::SpawnFailed {
            name: config.name.clone(),
            source: std::io::Error::new(std::io::ErrorKind::BrokenPipe, "no stdin"),
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DiscoveryError::SpawnFailed {
            name: config.name.clone(),
            source: std::io::Error::new(std::io::ErrorKind::BrokenPipe, "no stdout"),
        })?;

    let mut conn = StdioConnection {
        child,
        stdin,
        stdout: BufReader::new(stdout),
        next_id: 0,
    };

    // Perform MCP initialize handshake.
    let init_req = JsonRpcRequest::initialize(conn.next_id, "2024-11-05");
    conn.next_id = conn.next_id.wrapping_add(1);

    let resp = send_stdio(&mut conn, &init_req, &config.name, config.timeout_secs).await?;
    resp.into_result()
        .map_err(|e| DiscoveryError::HandshakeFailed {
            name: config.name.clone(),
            reason: format!("{e}"),
        })?;

    Ok(Mutex::new(conn))
}

/// Send a JSON-RPC request over stdio and read the response.
///
/// `timeout_secs` caps the total wait for the server's reply.
async fn send_stdio(
    conn: &mut StdioConnection,
    request: &JsonRpcRequest,
    server_name: &str,
    timeout_secs: u64,
) -> Result<JsonRpcResponse, DiscoveryError> {
    let mut json = serde_json::to_string(request).map_err(|e| DiscoveryError::Json {
        name: server_name.to_owned(),
        source: e,
    })?;
    json.push('\n');

    conn.stdin
        .write_all(json.as_bytes())
        .await
        .map_err(|e| DiscoveryError::TransportIo {
            name: server_name.to_owned(),
            source: e,
        })?;
    conn.stdin
        .flush()
        .await
        .map_err(|e| DiscoveryError::TransportIo {
            name: server_name.to_owned(),
            source: e,
        })?;

    let mut line = String::new();
    tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        (&mut conn.stdout).take(MAX_LINE_BYTES).read_line(&mut line),
    )
    .await
    .map_err(|_| DiscoveryError::TransportIo {
        name: server_name.to_owned(),
        source: std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("stdio read timed out after {timeout_secs}s"),
        ),
    })?
    .map_err(|e| DiscoveryError::TransportIo {
        name: server_name.to_owned(),
        source: e,
    })?;

    if line.is_empty() {
        return Err(DiscoveryError::ServerExited {
            name: server_name.to_owned(),
        });
    }

    serde_json::from_str(&line).map_err(|e| DiscoveryError::Json {
        name: server_name.to_owned(),
        source: e,
    })
}

// ── HTTP Arena Transport ────────────────────────────────────────────────────

/// HTTP connection to an MCP server.
pub(crate) struct HttpConnection {
    /// HTTP client.
    client: reqwest::Client,
    /// Server URL.
    url: String,
    /// Next JSON-RPC request ID.
    next_id: u64,
}

/// Connect to an HTTP MCP server and perform the initialize handshake.
///
/// # Errors
///
/// Returns [`DiscoveryError`] if the connection or handshake fails.
pub(crate) async fn connect_http(
    config: &ServerConfig,
) -> Result<Mutex<HttpConnection>, DiscoveryError> {
    let url = config.url.as_deref().unwrap_or_default().to_owned();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.timeout_secs))
        .build()
        .map_err(|e| DiscoveryError::HttpConnectFailed {
            name: config.name.clone(),
            url: url.clone(),
            source: e,
        })?;

    let mut conn = HttpConnection {
        client,
        url: url.clone(),
        next_id: 0,
    };

    // Perform MCP initialize handshake.
    let init_req = JsonRpcRequest::initialize(conn.next_id, "2024-11-05");
    conn.next_id = conn.next_id.wrapping_add(1);

    let resp = send_http(&mut conn, &init_req, &config.name).await?;
    resp.into_result()
        .map_err(|e| DiscoveryError::HandshakeFailed {
            name: config.name.clone(),
            reason: format!("{e}"),
        })?;

    Ok(Mutex::new(conn))
}

/// Send a JSON-RPC request over HTTP and read the response.
async fn send_http(
    conn: &mut HttpConnection,
    request: &JsonRpcRequest,
    server_name: &str,
) -> Result<JsonRpcResponse, DiscoveryError> {
    let resp = conn
        .client
        .post(&conn.url)
        .json(request)
        .send()
        .await
        .map_err(|e| DiscoveryError::HttpConnectFailed {
            name: server_name.to_owned(),
            url: conn.url.clone(),
            source: e,
        })?;

    let body =
        resp.json::<JsonRpcResponse>()
            .await
            .map_err(|e| DiscoveryError::HttpConnectFailed {
                name: server_name.to_owned(),
                url: conn.url.clone(),
                source: e,
            })?;

    Ok(body)
}

// ── Discovery Orchestrator ──────────────────────────────────────────────────

/// `tools/list` response with pagination cursor.
#[derive(Debug, Deserialize)]
struct ToolsListResult {
    /// Discovered tools.
    tools: Vec<ToolInfo>,
    /// Cursor for the next page, if any.
    #[serde(rename = "nextCursor")]
    next_cursor: Option<String>,
}

/// Discover tools from a single stdio MCP server.
///
/// Calls `tools/list` with pagination support.
///
/// # Errors
///
/// Returns [`DiscoveryError`] if tool listing fails.
pub async fn discover_stdio(config: &ServerConfig) -> Result<Vec<ToolInfo>, DiscoveryError> {
    let conn = connect_stdio(config).await?;
    let mut guard = conn.lock().await;
    let mut all_tools = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let id = guard.next_id;
        guard.next_id = guard.next_id.wrapping_add(1);

        let params = cursor.as_ref().map(|c| serde_json::json!({ "cursor": c }));

        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: "tools/list".to_owned(),
            params,
        };

        let resp = send_stdio(&mut guard, &req, &config.name, config.timeout_secs).await?;
        let result_value = resp
            .into_result()
            .map_err(|e| DiscoveryError::ToolsListFailed {
                name: config.name.clone(),
                reason: format!("{e}"),
            })?;

        let result: ToolsListResult =
            serde_json::from_value(result_value).map_err(|e| DiscoveryError::Json {
                name: config.name.clone(),
                source: e,
            })?;

        all_tools.extend(result.tools);

        match result.next_cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }

    Ok(all_tools)
}

/// Discover tools from a single HTTP MCP server.
///
/// Calls `tools/list` with pagination support.
///
/// # Errors
///
/// Returns [`DiscoveryError`] if tool listing fails.
pub async fn discover_http(config: &ServerConfig) -> Result<Vec<ToolInfo>, DiscoveryError> {
    let conn = connect_http(config).await?;
    let mut guard = conn.lock().await;
    let mut all_tools = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let id = guard.next_id;
        guard.next_id = guard.next_id.wrapping_add(1);

        let params = cursor.as_ref().map(|c| serde_json::json!({ "cursor": c }));

        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: "tools/list".to_owned(),
            params,
        };

        let resp = send_http(&mut guard, &req, &config.name).await?;
        let result_value = resp
            .into_result()
            .map_err(|e| DiscoveryError::ToolsListFailed {
                name: config.name.clone(),
                reason: format!("{e}"),
            })?;

        let result: ToolsListResult =
            serde_json::from_value(result_value).map_err(|e| DiscoveryError::Json {
                name: config.name.clone(),
                source: e,
            })?;

        all_tools.extend(result.tools);

        match result.next_cursor {
            Some(c) if !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }

    Ok(all_tools)
}

/// Discover tools from all configured MCP servers and build a [`ToolRegistry`].
///
/// Connects to each server sequentially, performs the MCP handshake, and
/// calls `tools/list` with pagination. Results are merged into a single
/// registry keyed by server name.
///
/// # Errors
///
/// Returns the first [`DiscoveryError`] encountered. Successfully discovered
/// servers before the error are not included in the result.
pub async fn discover_all(servers: &[ServerConfig]) -> Result<ToolRegistry, DiscoveryError> {
    let mut registry = ToolRegistry::new();

    for server in servers {
        let tools = match server.transport {
            TransportType::Stdio => discover_stdio(server).await?,
            TransportType::Http => discover_http(server).await?,
        };

        tracing::info!(
            server = %server.name,
            tool_count = tools.len(),
            "discovered tools"
        );

        registry.register(server.name.clone(), tools);
    }

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry() {
        let reg = ToolRegistry::new();
        assert_eq!(reg.tool_count(), 0);
        assert_eq!(reg.server_count(), 0);
        assert!(reg.find_tool("anything").is_none());
    }

    #[test]
    fn register_and_find() {
        let mut reg = ToolRegistry::new();
        let tools = vec![ToolInfo {
            name: "soulTools".to_owned(),
            description: Some("SOUL knowledge graph".to_owned()),
            input_schema: serde_json::json!({"type": "object"}),
        }];
        reg.register("soul".to_owned(), tools);

        assert_eq!(reg.tool_count(), 1);
        assert_eq!(reg.server_count(), 1);

        let (server, tool) = reg.find_tool("soulTools").expect("should find");
        assert_eq!(server, "soul");
        assert_eq!(tool.name, "soulTools");
    }

    #[test]
    fn find_tool_across_servers() {
        let mut reg = ToolRegistry::new();
        reg.register(
            "a".to_owned(),
            vec![ToolInfo {
                name: "tool_a".to_owned(),
                description: None,
                input_schema: serde_json::json!({}),
            }],
        );
        reg.register(
            "b".to_owned(),
            vec![ToolInfo {
                name: "tool_b".to_owned(),
                description: None,
                input_schema: serde_json::json!({}),
            }],
        );

        assert_eq!(reg.tool_count(), 2);
        assert_eq!(reg.find_tool("tool_b").unwrap().0, "b");
        assert!(reg.find_tool("nonexistent").is_none());
    }

    #[test]
    fn all_tools_flattens() {
        let mut reg = ToolRegistry::new();
        reg.register(
            "s1".to_owned(),
            vec![
                ToolInfo {
                    name: "t1".to_owned(),
                    description: None,
                    input_schema: serde_json::json!({}),
                },
                ToolInfo {
                    name: "t2".to_owned(),
                    description: None,
                    input_schema: serde_json::json!({}),
                },
            ],
        );
        reg.register(
            "s2".to_owned(),
            vec![ToolInfo {
                name: "t3".to_owned(),
                description: None,
                input_schema: serde_json::json!({}),
            }],
        );

        assert_eq!(reg.all_tools().len(), 3);
    }

    #[test]
    fn cache_roundtrip() {
        let mut reg = ToolRegistry::new();
        reg.register(
            "test".to_owned(),
            vec![ToolInfo {
                name: "my_tool".to_owned(),
                description: Some("A test tool".to_owned()),
                input_schema: serde_json::json!({"type": "object", "properties": {}}),
            }],
        );

        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("schema_cache.json");

        reg.save_cache(&path).expect("save");
        let loaded = ToolRegistry::load_cache(&path).expect("load");

        assert_eq!(loaded.tool_count(), 1);
        assert_eq!(loaded.find_tool("my_tool").unwrap().0, "test");
    }
}
