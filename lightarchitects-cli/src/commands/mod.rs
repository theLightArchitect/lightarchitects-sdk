//! Top-level command dispatch for the `lightarchitects` CLI.

pub mod builds;
pub mod config;
pub mod corso;
pub mod eva;
pub mod quantum;
pub mod seraph;
pub mod setup;
pub mod soul;
pub mod status;
pub mod webshell;

use clap::Subcommand;
use lightarchitects_core::SdkError;

use crate::config::CliConfig;
use crate::output::OutputMode;

/// All top-level `lightarchitects` subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// SOUL knowledge-graph operations.
    Soul {
        /// Which SOUL operation to run.
        #[command(subcommand)]
        cmd: soul::SoulCommand,
    },
    /// CORSO operations-platform commands.
    Corso {
        /// Which CORSO operation to run.
        #[command(subcommand)]
        cmd: corso::CorsoCommand,
    },
    /// EVA consciousness-system commands.
    Eva {
        /// Which EVA operation to run.
        #[command(subcommand)]
        cmd: eva::EvaCommand,
    },
    /// QUANTUM investigation-toolkit commands.
    Quantum {
        /// Which QUANTUM operation to run.
        #[command(subcommand)]
        cmd: quantum::QuantumCommand,
    },
    /// SERAPH pentest-orchestration commands.
    ///
    /// All commands require an active engagement scope in `~/lightarchitects/seraph/scope.toml`.
    Seraph {
        /// Which SERAPH operation to run.
        #[command(subcommand)]
        cmd: seraph::SeraphCommand,
    },
    /// Show which MCP binaries are present on disk.
    Status,
    /// Show the resolved binary-path configuration.
    Config,
    /// Interactive configuration wizard.
    ///
    /// Run `setup` alone for the full wizard, or specify a component:
    ///
    /// ```text
    /// lightarchitects setup           # Full wizard (keys + voice)
    /// lightarchitects setup keys      # API keys only
    /// lightarchitects setup keys --key MISTRAL_API_KEY  # Single key
    /// lightarchitects setup voice     # Voice provider + sibling voices
    /// ```
    Setup {
        /// Component to configure (omit for full wizard).
        #[command(subcommand)]
        component: Option<setup::SetupComponent>,
    },
    /// Build portfolio — show project tiers and status from the SOUL vault.
    Builds {
        /// Which builds operation to run.
        #[command(subcommand)]
        cmd: builds::BuildsCommand,
    },
    /// Webshell — local web GUI for the active coding agent.
    Webshell {
        /// Which webshell operation to run.
        #[command(subcommand)]
        cmd: webshell::WebshellCommand,
    },
}

/// Route a parsed command to its executor.
///
/// # Errors
///
/// Propagates any [`SdkError`] returned by the MCP client.
pub async fn dispatch(cmd: Commands, cfg: &CliConfig, mode: OutputMode) -> Result<(), SdkError> {
    match cmd {
        Commands::Soul { cmd } => soul::execute(cfg.soul.clone(), cmd, mode).await,
        Commands::Corso { cmd } => corso::execute(cfg.corso.clone(), cmd, mode).await,
        Commands::Eva { cmd } => eva::execute(cfg.eva.clone(), cmd, mode).await,
        Commands::Quantum { cmd } => quantum::execute(cfg.quantum.clone(), cmd, mode).await,
        Commands::Seraph { cmd } => seraph::execute(cfg.seraph.clone(), cmd, mode).await,
        Commands::Status => {
            status::execute(cfg, mode);
            Ok(())
        }
        Commands::Config => {
            config::execute(cfg, mode);
            Ok(())
        }
        Commands::Setup { component } => setup::execute(component, mode),
        Commands::Builds { cmd } => builds::execute(cmd, mode),
        Commands::Webshell { cmd } => webshell::execute(cfg.webshell.clone(), cmd).await,
    }
}
