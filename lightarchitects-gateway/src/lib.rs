//! `lightarchitects-gateway` — Light Architects unified gateway.
//!
//! Single binary with three operating modes:
//!
//! - **MCP mode** (default): stdio JSON-RPC server for Claude Code.
//! - **Arena mode** (`serve`): HTTP API + scheduler + autonomous heartbeat agents.
//! - **Conductor mode** (`conductor`): autonomous task execution loop.
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
/// CLI subcommands (soul, corso, eva, quantum, seraph, status, config, builds, setup, webshell).
pub mod cli;
/// Conductor — autonomous task execution loop.
pub mod conductor;
/// Gateway configuration: typed schema and loader.
pub mod config;
/// Core tool implementations.
pub mod core_tools;
/// Error types.
pub mod error;
/// Scope governance — trust and scope enforcement for agent orchestration.
pub mod governance;
/// In-process sibling handlers (feature-gated behind `inline-*` flags).
pub mod handlers;
/// MCP server loop and tool dispatch.
pub mod server;
/// Sibling subprocess spawner and MCP proxy.
///
/// Only compiled when the `spawner` feature is enabled (default).
/// When `inline-all` is used without `spawner`, this module is absent and
/// all sibling calls go through in-process handlers.
#[cfg(feature = "spawner")]
pub mod spawner;
/// Squad Comms MCP actions — HTTP delegation to the webshell coordination API.
pub mod squad_comms;
/// Vault-as-git module — pre-push validation and public companion sync.
pub mod vault;
/// Build-time version metadata (CARGO_PKG_VERSION + git-sha + build-date).
pub mod version;
