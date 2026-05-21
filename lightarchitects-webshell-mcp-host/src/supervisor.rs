//! Per-server lifecycle supervisor (7-state machine).
//!
//! States: Spawning → Handshaking → Ready ↔ Degraded → Restarting → CircuitOpen
//!                                                                  → ConfigError
//!                                     Ready → Stopped

use rmcp::service::{RoleClient, RunningService};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::{
    McpHostError,
    catalog::{ToolCatalog, ToolInfo},
    config::ServerEntry,
    spawner, transport,
};

/// Supervisor lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorState {
    /// Subprocess is being built and spawned.
    Spawning,
    /// MCP `initialize` handshake in progress.
    Handshaking,
    /// Healthy: accepts `tools/call` requests.
    Ready,
    /// Health check failed; retry pending.
    Degraded,
    /// Deliberate restart underway.
    Restarting,
    /// Restart limit exceeded; backing off permanently.
    CircuitOpen,
    /// Permanent failure (bad config or binary not found).
    ConfigError,
    /// Gracefully stopped.
    Stopped,
}

impl std::fmt::Display for SupervisorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Spawning => "Spawning",
            Self::Handshaking => "Handshaking",
            Self::Ready => "Ready",
            Self::Degraded => "Degraded",
            Self::Restarting => "Restarting",
            Self::CircuitOpen => "CircuitOpen",
            Self::ConfigError => "ConfigError",
            Self::Stopped => "Stopped",
        };
        f.write_str(label)
    }
}

/// Manages a single MCP server process lifecycle.
pub struct Supervisor {
    entry: ServerEntry,
    state: SupervisorState,
    running: Option<RunningService<RoleClient, ()>>,
    ct: CancellationToken,
    restart_count: u32,
}

impl Supervisor {
    const MAX_RESTARTS: u32 = 5;

    /// Create a new supervisor (does not spawn yet — call `start()`).
    pub fn new(entry: ServerEntry, ct: CancellationToken) -> Self {
        Self {
            entry,
            state: SupervisorState::Spawning,
            running: None,
            ct,
            restart_count: 0,
        }
    }

    /// Current state (cheap copy of the enum).
    pub fn state(&self) -> &SupervisorState {
        &self.state
    }

    /// Server name from the config entry.
    pub fn name(&self) -> &str {
        &self.entry.name
    }

    /// `true` when the server is `Ready` and can accept calls.
    pub fn is_ready(&self) -> bool {
        self.state == SupervisorState::Ready
    }

    /// Spawn the process, perform the MCP handshake, and populate the catalog.
    ///
    /// On success the supervisor transitions to `Ready`.
    /// On failure it transitions to `ConfigError` (permanent) or returns the error.
    pub async fn start(&mut self, catalog: &ToolCatalog) -> Result<(), McpHostError> {
        self.state = SupervisorState::Spawning;
        let cmd = match spawner::build_command(&self.entry) {
            Ok(c) => c,
            Err(e) => {
                self.state = SupervisorState::ConfigError;
                error!(server = %self.entry.name, err = %e, "spawn build failed");
                return Err(e);
            }
        };

        self.state = SupervisorState::Handshaking;
        let running = match transport::connect(cmd, &self.entry.name, self.ct.child_token()).await {
            Ok(r) => r,
            Err(e) => {
                self.state = SupervisorState::Degraded;
                error!(server = %self.entry.name, err = %e, "handshake failed");
                return Err(e);
            }
        };

        let tools = fetch_tools(&running, &self.entry.name).await?;
        catalog.set(&self.entry.name, tools);

        self.running = Some(running);
        self.state = SupervisorState::Ready;
        self.restart_count = 0;
        info!(server = %self.entry.name, "ready");
        Ok(())
    }

    /// Access the live rmcp connection for making `tools/call` requests.
    pub fn running(&self) -> Option<&RunningService<RoleClient, ()>> {
        self.running.as_ref()
    }

    /// Graceful stop: drop the connection and transition to `Stopped`.
    pub async fn stop(&mut self, catalog: &ToolCatalog) {
        self.running = None;
        catalog.remove(&self.entry.name);
        self.state = SupervisorState::Stopped;
        info!(server = %self.entry.name, "stopped");
    }

    /// Restart after a transient failure.
    ///
    /// Opens the circuit breaker after `MAX_RESTARTS` consecutive failures.
    pub async fn restart(&mut self, catalog: &ToolCatalog) -> Result<(), McpHostError> {
        self.restart_count += 1;
        if self.restart_count > Self::MAX_RESTARTS {
            warn!(
                server = %self.entry.name,
                restarts = self.restart_count,
                "circuit open — restart limit exceeded",
            );
            self.state = SupervisorState::CircuitOpen;
            return Err(McpHostError::NotReady {
                name: self.entry.name.clone(),
            });
        }
        self.state = SupervisorState::Restarting;
        self.running = None;
        catalog.remove(&self.entry.name);
        self.start(catalog).await
    }
}

/// Fetch the tool catalog from a live rmcp connection.
async fn fetch_tools(
    running: &RunningService<RoleClient, ()>,
    server_name: &str,
) -> Result<Vec<ToolInfo>, McpHostError> {
    let result = running
        .peer()
        .list_tools(None)
        .await
        .map_err(|e| McpHostError::ToolsList {
            name: server_name.to_owned(),
            reason: e.to_string(),
        })?;

    let tools = result
        .tools
        .into_iter()
        .map(|t| ToolInfo {
            name: t.name.to_string(),
            description: t.description.clone().unwrap_or_default().to_string(),
            input_schema: serde_json::to_value(&t.input_schema).unwrap_or_default(),
        })
        .collect();

    Ok(tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LifecycleMode, ScopeConfig};
    use std::collections::HashMap;

    fn entry_for(name: &str, command: &str) -> ServerEntry {
        ServerEntry {
            name: name.into(),
            command: command.into(),
            args: vec![],
            env: HashMap::new(),
            scope: ScopeConfig {
                allowed_paths: vec![],
                allowed_net_hosts: vec![],
                allowed_env_keys: vec![],
                max_concurrent_calls: 3,
                call_timeout_ms: 30_000,
                lifecycle_mode: LifecycleMode::Persistent,
                allowed_tools: None,
            },
        }
    }

    #[test]
    fn new_supervisor_starts_in_spawning_state() {
        let ct = CancellationToken::new();
        let sup = Supervisor::new(entry_for("test", "echo"), ct);
        assert_eq!(*sup.state(), SupervisorState::Spawning);
    }

    #[test]
    fn new_supervisor_is_not_ready() {
        let ct = CancellationToken::new();
        let sup = Supervisor::new(entry_for("test", "echo"), ct);
        assert!(!sup.is_ready());
    }

    #[test]
    fn supervisor_name_matches_entry() {
        let ct = CancellationToken::new();
        let sup = Supervisor::new(entry_for("myserver", "echo"), ct);
        assert_eq!(sup.name(), "myserver");
    }

    #[test]
    fn state_display_labels_are_correct() {
        assert_eq!(SupervisorState::Ready.to_string(), "Ready");
        assert_eq!(SupervisorState::CircuitOpen.to_string(), "CircuitOpen");
        assert_eq!(SupervisorState::ConfigError.to_string(), "ConfigError");
    }
}
