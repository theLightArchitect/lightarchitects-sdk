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

/// Interactive coding agent — NDJSON streaming + TTY REPL.
pub mod agent_stream;
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
/// Conversational build mode — pair programmer in a box.
pub mod conversational;
/// Core tool implementations.
pub mod core_tools;
/// Real-time helix enrichment worker — sage projection after SOUL writes.
pub mod enrichment;
/// Error types.
pub mod error;
/// Scope governance — trust and scope enforcement for agent orchestration.
pub mod governance;
/// In-process sibling handlers (feature-gated behind `inline-*` flags).
pub mod handlers;
/// Platform HTTP mode — private REST API backed by local Neo4j (`/v1/platform/*`).
#[allow(missing_docs, clippy::pedantic)]
pub mod http;
/// LightSquad task dispatch — single-shot LLM delegation via LiteLLM, offloading
/// Claude Code subagent work to cheaper providers (Ollama Cloud, etc.).
pub mod lightsquad_task;
/// Shared LLM client — Ollama, OpenAI-compatible, Anthropic.
pub mod llm;
/// LLM provider implementations for the agentic loop.
pub mod providers;
/// LASDLC C1-C8 effectiveness rubric — grades agent/task outputs.
pub mod rubric;
/// HMAC-SHA256 signing and verification for LASDLC hook payloads.
pub mod security;
/// MCP server loop and tool dispatch.
pub mod server;
/// Gateway-wide AYIN span context — task-local propagation + atomic disk write.
pub mod span_context;
/// Sibling subprocess spawner and MCP proxy.
///
/// Only compiled when the `spawner` feature is enabled (default).
/// When `inline-all` is used without `spawner`, this module is absent and
/// all sibling calls go through in-process handlers.
#[cfg(feature = "spawner")]
pub mod spawner;
/// Squad Comms MCP actions — HTTP delegation to the webshell coordination API.
pub mod squad_comms;
/// Vault-as-git — pre-push validation and public companion repo sync.
pub mod vault;
/// Build-time version metadata (CARGO_PKG_VERSION + git-sha + build-date).
pub mod version;
