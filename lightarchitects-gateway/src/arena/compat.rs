//! Compatibility layer for Arena modules migrated from `la_sdk_core`.
//!
//! The Arena was originally built against `la_sdk_core` (IronClaw-era SDK).
//! This module provides thin wrappers and re-exports that map `la_sdk_core`
//! APIs to `lightarchitects-core` equivalents, avoiding mass rewrites of
//! battle-tested Arena code.

use serde::{Deserialize, Serialize};

// ── JSON-RPC extensions ──────────────────────────────────────────────────

/// Extension trait adding `new()` to the SDK's `JsonRpcRequest`.
///
/// The SDK's `JsonRpcRequest` has `tools_call()` but not a general `new()`.
/// The Arena's MCP pool needs both.
pub trait JsonRpcRequestExt {
    /// Create a raw JSON-RPC request (method + params).
    fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self;
}

impl JsonRpcRequestExt for lightarchitects_core::jsonrpc::JsonRpcRequest {
    fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

/// Extension trait adding `is_error()` to the SDK's `JsonRpcResponse`.
pub trait JsonRpcResponseExt {
    /// Check if the response contains an error.
    fn is_error(&self) -> bool;
}

impl JsonRpcResponseExt for lightarchitects_core::jsonrpc::JsonRpcResponse {
    fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

// ── API error types (Arena HTTP layer) ───────────────────────────────────

/// Machine-readable error codes matching the OpenAPI `ApiError.code` enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Invalid request parameters.
    InvalidRequest,
    /// Authentication failed.
    Unauthorized,
    /// Forbidden (valid auth, insufficient scope).
    Forbidden,
    /// Resource not found.
    NotFound,
    /// Rate limit exceeded.
    RateLimited,
    /// Scope denied by governance.
    ScopeDenied,
    /// Sibling MCP server unavailable.
    SiblingUnavailable,
    /// Action execution failed.
    ActionFailed,
    /// Internal server error.
    InternalError,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::InvalidRequest => "invalid_request",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::RateLimited => "rate_limited",
            Self::ScopeDenied => "scope_denied",
            Self::SiblingUnavailable => "sibling_unavailable",
            Self::ActionFailed => "action_failed",
            Self::InternalError => "internal_error",
        };
        write!(f, "{s}")
    }
}

/// HTTP API error with code, message, and optional details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    /// Machine-readable error code.
    pub code: ErrorCode,
    /// Human-readable error message.
    pub message: String,
    /// HTTP status code hint (e.g., 401, 429, 500).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// Additional structured details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}
