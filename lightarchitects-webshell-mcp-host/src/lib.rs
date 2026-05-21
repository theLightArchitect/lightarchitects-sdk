//! `lightarchitects-webshell-mcp-host`
//!
//! Generic MCP host library: spawn declared stdio MCP servers, perform
//! `initialize` + `tools/list` handshake, cache the tool catalog, and expose
//! an async API consumed by the webshell HTTP surface (Phase 5).

#![warn(missing_docs)]

pub mod catalog;
pub mod config;
pub mod error;
pub mod schema_validator;
pub mod scope_governor;
pub mod spawner;
pub mod supervisor;
pub mod transport;

pub use catalog::{ToolCatalog, ToolInfo};
pub use config::{LifecycleMode, McpHostConfig, ScopeConfig, ServerEntry};
pub use error::McpHostError;

use std::{collections::HashMap, sync::Arc};

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::error;

use supervisor::Supervisor;

/// Status snapshot for one managed MCP server.
#[derive(Debug, serde::Serialize)]
pub struct ServerStatus {
    /// Logical server name from the config.
    pub name: String,
    /// Current lifecycle state label.
    pub state: String,
    /// Number of tools in the catalog (0 when not yet ready).
    pub tool_count: usize,
}

/// Manages all declared MCP servers: spawn, catalog, and call routing.
pub struct HostManager {
    supervisors: HashMap<String, Arc<Mutex<Supervisor>>>,
    catalog: ToolCatalog,
    /// Root cancellation token — cancel to stop all servers.
    ct: CancellationToken,
}

impl HostManager {
    /// Create a `HostManager` from a parsed config and start all servers.
    ///
    /// Servers that fail to start are logged but do not prevent other servers
    /// from starting (fail-open per server, fail-closed per call).
    pub async fn from_config(config: McpHostConfig) -> Self {
        let ct = CancellationToken::new();
        let catalog = ToolCatalog::new();
        let mut supervisors = HashMap::new();

        for entry in config.servers {
            let name = entry.name.clone();
            let mut sup = Supervisor::new(entry, ct.child_token());
            if let Err(e) = sup.start(&catalog).await {
                error!(server = %name, err = %e, "server failed to start");
            }
            supervisors.insert(name, Arc::new(Mutex::new(sup)));
        }

        Self {
            supervisors,
            catalog,
            ct,
        }
    }

    /// Status snapshot of all managed servers.
    pub async fn list_servers(&self) -> Vec<ServerStatus> {
        let mut out = Vec::with_capacity(self.supervisors.len());
        for (name, sup) in &self.supervisors {
            let sup = sup.lock().await;
            let tool_count = self.catalog.get(name).map_or(0, |t| t.len());
            out.push(ServerStatus {
                name: name.clone(),
                state: sup.state().to_string(),
                tool_count,
            });
        }
        out
    }

    /// All cached tools across all ready servers.
    pub fn list_tools(&self) -> Vec<(String, ToolInfo)> {
        self.catalog.all()
    }

    /// Look up a tool by server name and tool name.
    pub fn find_tool(&self, server: &str, tool: &str) -> Option<ToolInfo> {
        self.catalog
            .get(server)?
            .into_iter()
            .find(|t| t.name == tool)
    }

    /// Layer 4 pre-call gate: scope + schema checks without invoking the tool.
    ///
    /// Returns `Ok(())` if the call is permitted and the input is well-formed.
    /// Phase 5 will call this before forwarding to the live rmcp connection.
    pub async fn check_call_policy(
        &self,
        server_name: &str,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Result<(), McpHostError> {
        let sup_arc = self
            .supervisors
            .get(server_name)
            .ok_or_else(|| McpHostError::NotFound {
                name: server_name.to_owned(),
            })?;

        let guard = sup_arc.lock().await;
        if !guard.is_ready() {
            return Err(McpHostError::NotReady {
                name: server_name.to_owned(),
            });
        }

        scope_governor::ScopeGovernor::new(server_name, guard.scope())
            .check_call(tool_name, input)?;

        if let Some(tool) = self.find_tool(server_name, tool_name) {
            schema_validator::validate_input(&tool.input_schema, input, server_name, tool_name)?;
        }

        Ok(())
    }

    /// Invoke a tool on a live MCP server after running scope + schema checks.
    ///
    /// Holds the supervisor lock for the duration of the call. Concurrent calls
    /// to the same server are serialized — acceptable for Phase 5; a per-server
    /// semaphore can replace this in a later phase.
    pub async fn invoke_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, McpHostError> {
        self.check_call_policy(server_name, tool_name, &input)
            .await?;

        let sup_arc = self
            .supervisors
            .get(server_name)
            .ok_or_else(|| McpHostError::NotFound {
                name: server_name.to_owned(),
            })?;

        let guard = sup_arc.lock().await;
        let running = guard.running().ok_or_else(|| McpHostError::NotReady {
            name: server_name.to_owned(),
        })?;

        let base = rmcp::model::CallToolRequestParams::new(tool_name.to_owned());
        let params = input
            .as_object()
            .cloned()
            .map_or(base.clone(), |args| base.with_arguments(args));

        let result =
            running
                .peer()
                .call_tool(params)
                .await
                .map_err(|e| McpHostError::ToolsCall {
                    name: server_name.to_owned(),
                    tool: tool_name.to_owned(),
                    reason: e.to_string(),
                })?;

        Ok(serde_json::to_value(&result.content)?)
    }

    /// Cancel the root token, stopping all managed server connections.
    pub fn shutdown(&self) {
        self.ct.cancel();
    }
}
