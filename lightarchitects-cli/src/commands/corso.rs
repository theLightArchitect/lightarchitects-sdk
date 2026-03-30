//! `lightarchitects corso` subcommands.

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use lightarchitects::corso::CorsoClient;
use lightarchitects_core::SdkError;

use crate::output::{OutputMode, print_text};

/// CORSO operations-platform commands.
#[derive(Debug, Subcommand)]
pub enum CorsoCommand {
    /// Security audit a target file or directory.
    Guard {
        /// Path to audit.
        target: String,
    },
    /// Research and fetch knowledge for a query.
    Fetch {
        /// Research query.
        query: String,
    },
    /// Code analysis (sniff) a target.
    Sniff {
        /// Target specification or path.
        target: String,
    },
    /// Generate code from a specification.
    Generate {
        /// Code generation prompt.
        prompt: String,
    },
    /// Search documentation for a query.
    Docs {
        /// Documentation search query.
        query: String,
    },
}

/// Execute a CORSO subcommand.
///
/// # Errors
///
/// Propagates any [`SdkError`] from the CORSO client.
pub async fn execute(binary: PathBuf, cmd: CorsoCommand, mode: OutputMode) -> Result<(), SdkError> {
    let client = CorsoClient::builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(120))
        .build()
        .await?;

    let output = match cmd {
        CorsoCommand::Guard { target } => client.guard(&target).await?.output,
        CorsoCommand::Fetch { query } => client.fetch(&query).await?.output,
        CorsoCommand::Sniff { target } => client.sniff(&target).await?.output,
        CorsoCommand::Generate { prompt } => client.generate_code(&prompt).await?.output,
        CorsoCommand::Docs { query } => client.search_documentation(&query).await?.output,
    };

    print_text(mode, &output);
    Ok(())
}
