//! Tool catalog — thread-safe cache of per-server `tools/list` responses.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use serde::{Deserialize, Serialize};

/// One tool entry as returned by `tools/list`, stored in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    /// Tool name (matches `tools/call` `name` field).
    pub name: String,
    /// Human-readable description for the UI.
    pub description: String,
    /// JSON Schema 2020-12 describing the tool's input arguments.
    pub input_schema: serde_json::Value,
}

/// Thread-safe cache: server name → list of its tools.
#[derive(Debug, Default, Clone)]
pub struct ToolCatalog {
    inner: Arc<RwLock<HashMap<String, Vec<ToolInfo>>>>,
}

impl ToolCatalog {
    /// Create an empty catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the tool list for `server_name`.
    pub fn set(&self, server_name: &str, tools: Vec<ToolInfo>) {
        if let Ok(mut map) = self.inner.write() {
            map.insert(server_name.to_owned(), tools);
        }
    }

    /// Retrieve all tools for `server_name`, or `None` if not yet populated.
    pub fn get(&self, server_name: &str) -> Option<Vec<ToolInfo>> {
        self.inner
            .read()
            .ok()
            .and_then(|m| m.get(server_name).cloned())
    }

    /// All tools across all servers, paired with their server name.
    pub fn all(&self) -> Vec<(String, ToolInfo)> {
        self.inner.read().ok().map_or_else(Vec::new, |m| {
            m.iter()
                .flat_map(|(srv, tools)| tools.iter().map(|t| (srv.clone(), t.clone())))
                .collect()
        })
    }

    /// Remove all entries for a server (called on supervisor stop/restart).
    pub fn remove(&self, server_name: &str) {
        if let Ok(mut map) = self.inner.write() {
            map.remove(server_name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_tool(name: &str) -> ToolInfo {
        ToolInfo {
            name: name.into(),
            description: format!("{name} tool"),
            input_schema: json!({"type": "object", "properties": {}}),
        }
    }

    #[test]
    fn set_and_get_roundtrip() {
        let catalog = ToolCatalog::new();
        catalog.set("drawio", vec![make_tool("open_drawio_xml")]);
        let tools = catalog.get("drawio").expect("should exist");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "open_drawio_xml");
    }

    #[test]
    fn get_missing_server_returns_none() {
        let catalog = ToolCatalog::new();
        assert!(catalog.get("nonexistent").is_none());
    }

    #[test]
    fn all_returns_all_servers() {
        let catalog = ToolCatalog::new();
        catalog.set("srv1", vec![make_tool("a"), make_tool("b")]);
        catalog.set("srv2", vec![make_tool("c")]);
        assert_eq!(catalog.all().len(), 3);
    }

    #[test]
    fn remove_clears_server() {
        let catalog = ToolCatalog::new();
        catalog.set("drawio", vec![make_tool("tool")]);
        catalog.remove("drawio");
        assert!(catalog.get("drawio").is_none());
    }
}
