//! Protocol constants shared across all `l-arc` crates.

/// JSON-RPC protocol version used in all requests.
pub const JSONRPC_VERSION: &str = "2.0";

/// MCP protocol version negotiated during `initialize`.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Default per-call timeout in seconds.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum response size accepted from an MCP binary, in bytes (10 MiB).
pub const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;
