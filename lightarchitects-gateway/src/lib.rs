//! `lightarchitects-gateway` — Light Architects unified gateway.
//!
//! Single binary with three operating modes:
//!
//! - **MCP mode** (default): stdio JSON-RPC server for Claude Code.
//! - **Arena mode** (`serve`): HTTP API + scheduler + autonomous heartbeat agents.
//! - **Conductor mode** (`conductor`): LVL8 autonomous task execution loop.
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

/// Arena — autonomous multi-agent research platform (HTTP + scheduler + heartbeats).
#[allow(unused, missing_docs, clippy::pedantic)]
pub mod arena;
/// Messaging channels — Discord webhooks, Telegram bot, Discord gateway.
#[allow(unused, missing_docs, clippy::pedantic)]
pub mod channels;
/// LVL8 Conductor — autonomous task execution loop.
pub mod conductor;
/// Gateway configuration: typed schema and loader.
pub mod config;
/// Core tool implementations.
pub mod core_tools;
/// Error types.
pub mod error;
/// Scope governance — trust and scope enforcement for agent orchestration.
pub mod governance;
/// MCP server loop and tool dispatch.
pub mod server;
/// Sibling subprocess spawner and MCP proxy.
pub mod spawner;
