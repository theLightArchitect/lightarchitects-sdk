//! `lightarchitects-gateway` — Light Architects MCP Gateway.
//!
//! This crate is the `lightarchitects` binary: an MCP server that provides
//! eight core tools (`lightarchitects_read`, `_write`, `_edit`, `_bash`,
//! `_search`, `_glob`, `_discover`, `_ask_user`) and proxies requests to
//! enabled sibling MCP servers.
//!
//! # Quick start
//!
//! ```no_run
//! # use lightarchitects_gateway::config::GatewayConfig;
//! # async fn example() -> Result<(), lightarchitects_gateway::error::GatewayError> {
//! let config = GatewayConfig::load()?;
//! lightarchitects_gateway::server::run(&config).await
//! # }
//! ```

/// LVL8 Conductor — autonomous task execution loop.
pub mod conductor;
/// Gateway configuration: typed schema and loader.
pub mod config;
/// Core tool implementations.
pub mod core_tools;
/// Error types.
pub mod error;
/// Scope governance — trust and scope enforcement for sibling orchestration.
pub mod governance;
/// MCP server loop and tool dispatch.
pub mod server;
/// Sibling subprocess spawner and MCP proxy.
pub mod spawner;
