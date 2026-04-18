//! `lightarchitects-core` — wire protocol, stdio transport, retry, and error types.
//!
//! This crate is the foundation for all sibling-specific clients in the
//! `lightarchitects-*` workspace. It provides:
//!
//! - [`Transport`] — async trait over the MCP stdio wire protocol
//! - [`StdioTransport`] — production implementation with newline and
//!   `Content-Length` framing
//! - [`McpClient`] — generic retry-aware client
//! - [`SiblingId`] / [`McpFraming`] — per-sibling binary path and framing
//! - [`SdkError`] — unified error hierarchy
//! - [`Config`] / [`RetryConfig`] — client and retry configuration
//! - [`McpHandler`] / [`McpServerLoop`] — server-side stdio transport primitive

/// MCP action types returned by `tools/list`.
pub mod action;
/// Connection-time authentication provider and type-erased checker.
pub mod auth;
/// Generic retry-aware MCP client.
pub mod client;
/// Client and retry configuration.
pub mod config;
/// Protocol constants shared across `lightarchitects` crates.
pub mod constants;
/// SDK error hierarchy.
pub mod error;
/// HTTP transport — connects the SDK to the Light Architects cloud gateway.
#[cfg(feature = "http-client")]
pub mod http_transport;
/// JSON-RPC 2.0 request and response types.
pub mod jsonrpc;
/// Canonical Light Architects filesystem path resolution.
pub mod paths;
/// Server-side stdio transport primitive for MCP servers.
pub mod server;
/// Sibling identity: binary paths, framing, and subcommands.
pub mod sibling;
/// Async transport trait and stdio implementation.
pub mod transport;

// ── Convenience re-exports ────────────────────────────────────────────────────

pub use action::{ToolInfo, ToolsListResponse};
pub use auth::{AuthChecker, AuthProvider, AuthStatus};
pub use client::McpClient;
pub use config::{Config, ConfigBuilder, RetryConfig};
pub use error::{ProtocolError, SdkError, ToolError, TransportError};
#[cfg(feature = "http-client")]
pub use http_transport::{DEFAULT_BASE_URL, HttpTransport, HttpTransportBuilder};
pub use jsonrpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use server::{McpHandler, McpServerLoop, ServerError};
pub use sibling::{McpFraming, SiblingId};
#[cfg(any(test, feature = "test-utils"))]
pub use transport::MockTransport;
pub use transport::{StdioTransport, Transport};

// ── In-process handler trait, registry, and dispatch mode ─────────────────────
pub mod handler;
