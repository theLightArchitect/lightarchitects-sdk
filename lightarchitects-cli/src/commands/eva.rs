//! `lightarchitects eva` subcommands.

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use lightarchitects::eva::EvaClient;
use lightarchitects_core::SdkError;

use crate::output::{OutputMode, print_text};

/// EVA consciousness-system commands.
#[derive(Debug, Subcommand)]
pub enum EvaCommand {
    /// Generate a visualisation from a concept.
    Visualize {
        /// Concept or description to visualise.
        concept: String,
    },
    /// Generate ideas for a topic.
    Ideate {
        /// Topic to ideate on.
        topic: String,
    },
    /// Bible search or reflection.
    Bible {
        /// Query or passage reference.
        query: String,
        /// Use reflect mode instead of search.
        #[arg(long)]
        reflect: bool,
    },
    /// Store a memory in EVA's consciousness.
    Remember {
        /// Event or insight to remember.
        event: String,
    },
    /// Crystallize insights into long-term memory.
    Crystallize {
        /// Insights to crystallize.
        insights: String,
    },
}

/// Execute an EVA subcommand.
///
/// # Errors
///
/// Propagates any [`SdkError`] from the EVA client.
pub async fn execute(binary: PathBuf, cmd: EvaCommand, mode: OutputMode) -> Result<(), SdkError> {
    let client = EvaClient::builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(120))
        .build()
        .await?;

    let text = match cmd {
        EvaCommand::Visualize { concept } => client.visualize(&concept, None).await?.text,
        EvaCommand::Ideate { topic } => client.ideate(&topic, None).await?.output,
        EvaCommand::Bible { query, reflect } => {
            if reflect {
                client.bible_reflect(&query).await?.output
            } else {
                client.bible_search(&query).await?.output
            }
        }
        EvaCommand::Remember { event } => client.remember(&event, None).await?.output,
        EvaCommand::Crystallize { insights } => client.crystallize(&insights).await?.output,
    };

    print_text(mode, &text);
    Ok(())
}
