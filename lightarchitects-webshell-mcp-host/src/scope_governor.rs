//! Layer 4 trust boundary: validates tool calls against per-server scope policy.
//!
//! Mitigated: CWE-285 (tool allowlist), CWE-22 (path traversal), CWE-918 (SSRF).

use std::path::Path;

use serde_json::Value;
use url::Url;

use crate::{McpHostError, config::ScopeConfig};

/// Validates a single tool call against the server's declared scope.
pub struct ScopeGovernor<'a> {
    server: String,
    scope: &'a ScopeConfig,
}

impl<'a> ScopeGovernor<'a> {
    /// Create a new governor for the named server.
    pub fn new(server: impl Into<String>, scope: &'a ScopeConfig) -> Self {
        Self {
            server: server.into(),
            scope,
        }
    }

    /// Run all scope checks for a tool call.
    pub fn check_call(&self, tool: &str, input: &Value) -> Result<(), McpHostError> {
        self.check_tool_allowed(tool)?;
        self.scan_value(input)
    }

    fn check_tool_allowed(&self, tool: &str) -> Result<(), McpHostError> {
        if let Some(allowed) = &self.scope.allowed_tools {
            if !allowed.iter().any(|t| t == tool) {
                return Err(self.scope_err(format!("tool '{tool}' not in allowed_tools")));
            }
        }
        Ok(())
    }

    fn scan_value(&self, v: &Value) -> Result<(), McpHostError> {
        match v {
            Value::String(s) => {
                if looks_like_path(s) {
                    self.check_path(s)?;
                }
                if looks_like_url(s) {
                    self.check_url(s)?;
                }
            }
            Value::Object(m) => {
                for v in m.values() {
                    self.scan_value(v)?;
                }
            }
            Value::Array(a) => {
                for v in a {
                    self.scan_value(v)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn check_path(&self, s: &str) -> Result<(), McpHostError> {
        // Reject traversal sequences before prefix check (CWE-22)
        if s.contains("..") {
            return Err(self.scope_err(format!("path '{s}' contains traversal sequence")));
        }
        if self.scope.allowed_paths.is_empty() {
            return Err(self.scope_err(format!("path '{s}' rejected: no allowed_paths configured")));
        }
        let candidate = expand_tilde(s);
        let inside = self
            .scope
            .allowed_paths
            .iter()
            .any(|ap| Path::new(&candidate).starts_with(Path::new(&expand_tilde(ap))));
        if !inside {
            return Err(self.scope_err(format!("path '{s}' outside allowed_paths")));
        }
        Ok(())
    }

    fn check_url(&self, s: &str) -> Result<(), McpHostError> {
        if self.scope.allowed_net_hosts.is_empty() {
            return Err(self
                .scope_err("URL argument rejected: no allowed_net_hosts configured".to_string()));
        }
        let host = Url::parse(s)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        if !self.scope.allowed_net_hosts.iter().any(|h| h == &host) {
            return Err(self.scope_err(format!("net host '{host}' not in allowed_net_hosts")));
        }
        Ok(())
    }

    fn scope_err(&self, reason: String) -> McpHostError {
        McpHostError::Scope {
            name: self.server.clone(),
            reason,
        }
    }
}

fn looks_like_path(s: &str) -> bool {
    s.starts_with('/') || s.starts_with("~/") || s.starts_with("./") || s.starts_with("../")
}

fn looks_like_url(s: &str) -> bool {
    s.contains("://")
}

fn expand_tilde(s: &str) -> String {
    match s.strip_prefix("~/") {
        Some(rest) => {
            std::env::var("HOME").map_or_else(|_| s.to_owned(), |h| format!("{h}/{rest}"))
        }
        None => s.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LifecycleMode, ScopeConfig};
    use serde_json::json;

    fn scope(
        allowed_tools: Option<Vec<&str>>,
        allowed_paths: Vec<&str>,
        allowed_net_hosts: Vec<&str>,
    ) -> ScopeConfig {
        ScopeConfig {
            allowed_tools: allowed_tools.map(|v| v.into_iter().map(str::to_owned).collect()),
            allowed_paths: allowed_paths.into_iter().map(str::to_owned).collect(),
            allowed_net_hosts: allowed_net_hosts.into_iter().map(str::to_owned).collect(),
            allowed_env_keys: vec![],
            max_concurrent_calls: 3,
            call_timeout_ms: 30_000,
            lifecycle_mode: LifecycleMode::Persistent,
        }
    }

    // H1-c: tool allowlist blocks unlisted tool
    #[test]
    fn h1c_tool_not_in_allowlist_is_rejected() {
        let s = scope(Some(vec!["safe_tool"]), vec![], vec![]);
        let gov = ScopeGovernor::new("srv", &s);
        assert!(gov.check_call("dangerous_tool", &json!({})).is_err());
    }

    #[test]
    fn allowlisted_tool_passes() {
        let s = scope(Some(vec!["safe_tool"]), vec![], vec![]);
        let gov = ScopeGovernor::new("srv", &s);
        assert!(gov.check_call("safe_tool", &json!({})).is_ok());
    }

    #[test]
    fn no_allowlist_permits_any_tool() {
        let s = scope(None, vec![], vec![]);
        let gov = ScopeGovernor::new("srv", &s);
        assert!(gov.check_call("any_tool", &json!({})).is_ok());
    }

    // H1-d: path traversal in arguments is rejected
    #[test]
    fn h1d_path_traversal_is_rejected() {
        let s = scope(None, vec!["/tmp/workspace"], vec![]);
        let gov = ScopeGovernor::new("srv", &s);
        let input = json!({ "path": "/tmp/workspace/../../../etc/passwd" });
        assert!(gov.check_call("read_file", &input).is_err());
    }

    #[test]
    fn path_within_allowed_passes() {
        let s = scope(None, vec!["/tmp/workspace"], vec![]);
        let gov = ScopeGovernor::new("srv", &s);
        let input = json!({ "path": "/tmp/workspace/file.txt" });
        assert!(gov.check_call("read_file", &input).is_ok());
    }

    #[test]
    fn path_outside_allowed_is_rejected() {
        let s = scope(None, vec!["/tmp/workspace"], vec![]);
        let gov = ScopeGovernor::new("srv", &s);
        let input = json!({ "path": "/etc/passwd" });
        assert!(gov.check_call("read_file", &input).is_err());
    }

    // H1-b: net host SSRF boundary
    #[test]
    fn h1b_disallowed_net_host_is_rejected() {
        let s = scope(None, vec![], vec!["allowed.example.com"]);
        let gov = ScopeGovernor::new("srv", &s);
        let input = json!({ "url": "https://evil.internal/steal?secret=1" });
        assert!(gov.check_call("fetch", &input).is_err());
    }

    #[test]
    fn allowed_net_host_passes() {
        let s = scope(None, vec![], vec!["allowed.example.com"]);
        let gov = ScopeGovernor::new("srv", &s);
        let input = json!({ "url": "https://allowed.example.com/api" });
        assert!(gov.check_call("fetch", &input).is_ok());
    }

    #[test]
    fn no_net_hosts_blocks_any_url() {
        let s = scope(None, vec![], vec![]);
        let gov = ScopeGovernor::new("srv", &s);
        let input = json!({ "url": "https://anywhere.com" });
        assert!(gov.check_call("fetch", &input).is_err());
    }
}
