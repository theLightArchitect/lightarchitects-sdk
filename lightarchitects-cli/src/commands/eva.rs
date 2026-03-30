//! `lightarchitects eva` subcommands.

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use lightarchitects::eva::{BibleAction, EvaClient, ResearchSource};
use lightarchitects_core::SdkError;

use crate::output::{OutputMode, print_text};

/// EVA consciousness-system commands.
#[derive(Debug, Subcommand)]
pub enum EvaCommand {
    /// Research a topic using EVA's multi-source pipeline.
    Research {
        /// Research query.
        query: String,
        /// Research source: ollama (default), perplexity, docs, context7.
        #[arg(long, default_value = "ollama")]
        source: String,
    },
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
        /// Action: search (default) or reflect.
        #[arg(long, default_value = "search")]
        action: String,
        /// Query or passage reference.
        query: Option<String>,
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
        EvaCommand::Research { query, source } => {
            let src = parse_source(&source);
            client.research(&query, src).await?.output
        }
        EvaCommand::Visualize { concept } => client.visualize(&concept, None).await?.text,
        EvaCommand::Ideate { topic } => client.ideate(&topic, None).await?.output,
        EvaCommand::Bible { action, query } => {
            let act = if action == "reflect" {
                BibleAction::Reflect
            } else {
                BibleAction::Search
            };
            client.bible(act, query.as_deref()).await?.output
        }
    };

    print_text(mode, &text);
    Ok(())
}

fn parse_source(s: &str) -> ResearchSource {
    match s {
        "perplexity" => ResearchSource::Perplexity,
        "docs" => ResearchSource::Docs,
        "context7" => ResearchSource::Context7,
        _ => ResearchSource::Ollama,
    }
}
