//! Subprocess builder for MCP server processes.
//!
//! Applies env isolation and process-group containment before handing
//! the command to `rmcp`'s `TokioChildProcess` transport.

use tokio::process::Command;

use crate::{McpHostError, config::ServerEntry};

/// Build a configured `Command` for the given server entry.
///
/// Applies:
/// - env isolation: `env_clear()` + `allowed_env_keys` whitelist (Layer 1, CWE-209)
/// - process group: `process_group(0)` for `kill(-pgid)` teardown (Layer 3)
///
/// stdio is left unset — `TokioChildProcess` configures piped I/O on spawn.
pub fn build_command(entry: &ServerEntry) -> Result<Command, McpHostError> {
    let mut cmd = Command::new(&entry.command);
    cmd.args(&entry.args);
    apply_env(&mut cmd, entry);
    cmd.process_group(0);
    Ok(cmd)
}

/// Populate env: clear all, then whitelist parent keys, then server overrides.
fn apply_env(cmd: &mut Command, entry: &ServerEntry) {
    cmd.env_clear();
    for key in &entry.scope.allowed_env_keys {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    for (k, v) in &entry.env {
        cmd.env(k, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LifecycleMode, ScopeConfig};
    use std::collections::HashMap;

    fn entry_for(command: &str) -> ServerEntry {
        ServerEntry {
            name: "test".into(),
            command: command.into(),
            args: vec![],
            env: HashMap::new(),
            scope: ScopeConfig {
                allowed_paths: vec![],
                allowed_net_hosts: vec![],
                allowed_env_keys: vec!["PATH".into()],
                max_concurrent_calls: 3,
                call_timeout_ms: 30_000,
                lifecycle_mode: LifecycleMode::Persistent,
                allowed_tools: None,
            },
        }
    }

    #[test]
    fn build_command_succeeds_for_valid_binary() {
        let entry = entry_for("echo");
        assert!(build_command(&entry).is_ok());
    }

    #[test]
    fn server_override_env_is_set() {
        let mut entry = entry_for("echo");
        entry.env.insert("MY_VAR".into(), "value".into());
        let cmd = build_command(&entry).expect("build");
        // Command::get_envs is available — verify our key appears
        let envs: Vec<_> = cmd.as_std().get_envs().collect();
        let found = envs
            .iter()
            .any(|(k, _)| *k == std::ffi::OsStr::new("MY_VAR"));
        assert!(found, "MY_VAR should be present in command env");
    }
}
