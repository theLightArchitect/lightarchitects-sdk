//! CLI subcommands for the `lightarchitects` gateway binary.
//!
//! Merged from `lightarchitects-cli` — provides typed access to all MCP siblings
//! plus utility commands (status, config, builds, setup, webshell).
//!
//! # Commands
//!
//! - **Sibling**: `soul`, `corso`, `eva`, `quantum`, `seraph`
//! - **Utility**: `status`, `config`, `builds`, `setup`, `webshell`
//!
//! # Output format
//!
//! Every command accepts `--output-format <json|text>` (default: `text`).
//! In JSON mode, sensitive fields are redacted before printing.

pub mod auth;
pub mod builds;
pub mod config_cmd;
pub mod init;
pub mod launcher;
pub mod output;
pub mod setup;
pub mod status;
pub mod webshell;

// Sibling CLI commands (SDK clients)
pub mod corso;
pub mod eva;
pub mod quantum;
pub mod seraph;
pub mod soul;
pub mod vault;

pub use output::OutputMode;
