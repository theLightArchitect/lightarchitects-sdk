//! JSON-RPC 2.0 request and response types for the stdio MCP transport.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constants::JSONRPC_VERSION;
use crate::error::ProtocolError;

/// A JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// Always `"2.0"`. Stored as `&'static str` (not `String`) to avoid
    /// allocation on the request side; [`JsonRpcResponse::jsonrpc`] is `String`
    /// because it is deserialized from wire data and must own its memory.
    pub jsonrpc: &'static str,
    /// Caller-assigned identifier correlated to the response.
    pub id: u64,
    /// Method name (e.g., `"tools/call"`, `"tools/list"`, `"initialize"`).
    pub method: String,
    /// Method-specific parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    /// Create a `tools/call` request for the named tool.
    #[must_use]
    pub fn tools_call(id: u64, tool: &str, arguments: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION,
            id,
            method: "tools/call".to_owned(),
            params: Some(serde_json::json!({
                "name": tool,
                "arguments": arguments
            })),
        }
    }

    /// Create a `tools/list` request.
    #[must_use]
    pub fn tools_list(id: u64) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION,
            id,
            method: "tools/list".to_owned(),
            params: None,
        }
    }

    /// Create an MCP `initialize` handshake request.
    #[must_use]
    pub fn initialize(id: u64, protocol_version: &str) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION,
            id,
            method: "initialize".to_owned(),
            params: Some(serde_json::json!({
                "protocolVersion": protocol_version,
                "capabilities": {},
                "clientInfo": {
                    "name": "l-arc",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            })),
        }
    }
}

/// A JSON-RPC 2.0 response (successful result or error).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Correlates to the originating request `id`.
    pub id: Option<u64>,
    /// Successful result payload (mutually exclusive with `error`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error payload (mutually exclusive with `result`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Unwrap the `result` field or return a [`ProtocolError`].
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::RpcError`] when the response contains a
    /// JSON-RPC error object.
    pub fn into_result(self) -> Result<Value, ProtocolError> {
        if let Some(err) = self.error {
            return Err(ProtocolError::RpcError {
                code: err.code,
                message: err.message,
            });
        }
        Ok(self.result.unwrap_or(Value::Null))
    }
}

/// JSON-RPC 2.0 error object embedded in a [`JsonRpcResponse`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Numeric error code (see JSON-RPC 2.0 spec).
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_serializes_without_params() {
        let req = JsonRpcRequest::tools_list(42);
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains(r#""method":"tools/list""#));
        assert!(json.contains(r#""id":42"#));
        // `params` is None → must be omitted (skip_serializing_if)
        assert!(!json.contains("params"));
    }

    #[test]
    fn tools_call_includes_name_and_arguments() {
        let req =
            JsonRpcRequest::tools_call(1, "soulTools", serde_json::json!({"action": "helix"}));
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains(r#""method":"tools/call""#));
        assert!(json.contains(r#""name":"soulTools""#));
        assert!(json.contains(r#""action":"helix""#));
    }

    #[test]
    fn response_into_result_ok() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: Some(1),
            result: Some(serde_json::json!({"ok": true})),
            error: None,
        };
        assert!(resp.into_result().is_ok());
    }

    #[test]
    fn response_into_result_rpc_error() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_owned(),
            id: Some(1),
            result: None,
            error: Some(JsonRpcError {
                code: -32_600,
                message: "invalid request".to_owned(),
            }),
        };
        let err = resp.into_result().expect_err("should be err");
        assert!(matches!(err, ProtocolError::RpcError { code: -32_600, .. }));
    }

    #[test]
    fn initialize_includes_protocol_version() {
        let req = JsonRpcRequest::initialize(0, "2024-11-05");
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains("2024-11-05"));
        assert!(json.contains(r#""method":"initialize""#));
    }
}
