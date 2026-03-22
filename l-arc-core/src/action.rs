//! Types for the `tools/list` MCP action.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Information about a single MCP tool as returned by `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    /// Tool name (e.g., `"soulTools"`, `"corsoTools"`, `"penTools"`).
    pub name: String,
    /// Optional human-readable description of the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema describing the tool's input parameters.
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Response payload for a `tools/list` request.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolsListResponse {
    /// All tools advertised by this MCP server.
    pub tools: Vec<ToolInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_info_roundtrips() {
        let info = ToolInfo {
            name: "soulTools".to_owned(),
            description: Some("SOUL knowledge graph".to_owned()),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let decoded: ToolInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.name, info.name);
    }

    #[test]
    fn tools_list_response_deserializes() {
        let json = r#"{"tools":[{"name":"soulTools","inputSchema":{"type":"object"}}]}"#;
        let resp: ToolsListResponse = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.tools.len(), 1);
        assert_eq!(resp.tools[0].name, "soulTools");
    }
}
