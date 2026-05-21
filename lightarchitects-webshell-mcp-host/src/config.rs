//! Config schema for `webshell-mcp.json`.

use crate::McpHostError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root config — one entry per MCP server to manage.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpHostConfig {
    /// Ordered list of MCP server entries.
    pub servers: Vec<ServerEntry>,
}

/// One MCP server definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerEntry {
    /// Logical name used as the server identifier in API responses.
    pub name: String,
    /// Binary to execute (e.g. `"npx"`, `"soul"`, absolute path).
    pub command: String,
    /// Arguments passed to the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra env vars injected alongside the allowed_env_keys whitelist.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Scope constraints enforced at spawn-time and invocation-time.
    pub scope: ScopeConfig,
}

/// Per-server scope constraints (5-layer trust model Layer 1).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScopeConfig {
    /// Filesystem paths the subprocess may write to.
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    /// Hostnames the subprocess may reach over TCP.
    #[serde(default)]
    pub allowed_net_hosts: Vec<String>,
    /// Env keys inherited from the parent process.
    #[serde(default)]
    pub allowed_env_keys: Vec<String>,
    /// Maximum simultaneous in-flight tool calls.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_calls: usize,
    /// Per-call timeout in milliseconds.
    #[serde(default = "default_call_timeout_ms")]
    pub call_timeout_ms: u64,
    /// Whether the server stays alive between calls or is re-spawned on demand.
    #[serde(default = "default_lifecycle")]
    pub lifecycle_mode: LifecycleMode,
    /// If set, only these tool names may be invoked on this server.
    pub allowed_tools: Option<Vec<String>>,
}

/// Server process lifecycle policy.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleMode {
    /// Server stays alive across calls.
    Persistent,
    /// Server is spawned per call and terminated after.
    OnDemand,
}

fn default_max_concurrent() -> usize {
    3
}
fn default_call_timeout_ms() -> u64 {
    30_000
}
fn default_lifecycle() -> LifecycleMode {
    LifecycleMode::Persistent
}

impl McpHostConfig {
    /// Parse a `webshell-mcp.json` document.
    pub fn from_json(json: &str) -> Result<Self, McpHostError> {
        serde_json::from_str(json).map_err(|e| McpHostError::Config(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let json = r#"{
            "servers": [{
                "name": "test",
                "command": "echo",
                "args": ["hello"],
                "scope": {
                    "allowed_paths": ["/tmp/test"],
                    "allowed_net_hosts": []
                }
            }]
        }"#;
        let cfg = McpHostConfig::from_json(json).expect("parse");
        assert_eq!(cfg.servers.len(), 1);
        assert_eq!(cfg.servers[0].name, "test");
        assert_eq!(cfg.servers[0].scope.max_concurrent_calls, 3);
        assert_eq!(
            cfg.servers[0].scope.lifecycle_mode,
            LifecycleMode::Persistent
        );
    }

    #[test]
    fn parse_config_with_optional_fields() {
        let json = r#"{
            "servers": [{
                "name": "drawio",
                "command": "npx",
                "args": ["-y", "@drawio/mcp"],
                "scope": {
                    "allowed_paths": ["/tmp/diagrams"],
                    "allowed_net_hosts": ["drawio.com"],
                    "allowed_env_keys": ["HOME", "PATH"],
                    "max_concurrent_calls": 2,
                    "call_timeout_ms": 15000,
                    "lifecycle_mode": "persistent",
                    "allowed_tools": ["open_drawio_xml", "open_drawio_mermaid"]
                }
            }]
        }"#;
        let cfg = McpHostConfig::from_json(json).expect("parse");
        let scope = &cfg.servers[0].scope;
        assert_eq!(scope.max_concurrent_calls, 2);
        assert_eq!(
            scope.allowed_tools,
            Some(vec!["open_drawio_xml".into(), "open_drawio_mermaid".into()])
        );
    }

    #[test]
    fn parse_config_rejects_invalid_json() {
        assert!(McpHostConfig::from_json("not json").is_err());
    }
}
