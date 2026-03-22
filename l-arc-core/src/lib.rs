//! `l-arc-core` — wire protocol, stdio transport, retry, and error types.
//!
//! This crate is the foundation for all sibling-specific clients in the
//! `l-arc-*` workspace. It provides:
//!
//! - [`Transport`] — async trait over the MCP stdio wire protocol
//! - [`StdioTransport`] — production implementation with newline and
//!   `Content-Length` framing
//! - [`McpClient`] — generic retry-aware client
//! - [`SiblingId`] / [`McpFraming`] — per-sibling binary path and framing
//! - [`SdkError`] — unified error hierarchy
//! - [`Config`] / [`RetryConfig`] — client and retry configuration

/// MCP action types returned by `tools/list`.
pub mod action;
/// Generic retry-aware MCP client.
pub mod client;
/// Client and retry configuration.
pub mod config;
/// Protocol constants shared across `l-arc` crates.
pub mod constants;
/// SDK error hierarchy.
pub mod error;
/// JSON-RPC 2.0 request and response types.
pub mod jsonrpc;
/// Sibling identity: binary paths, framing, and subcommands.
pub mod sibling;
/// Async transport trait and stdio implementation.
pub mod transport;

// ── Convenience re-exports ────────────────────────────────────────────────────

pub use action::{ToolInfo, ToolsListResponse};
pub use client::McpClient;
pub use config::{Config, ConfigBuilder, RetryConfig};
pub use error::{ProtocolError, SdkError, ToolError, TransportError};
pub use jsonrpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use sibling::{McpFraming, SiblingId};
pub use transport::{StdioTransport, Transport};
