//! `l-arc soul` subcommands.

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use l_arc::soul::SoulClient;
use l_arc_core::SdkError;

use crate::output::{OutputMode, print_text, print_value};

/// SOUL knowledge-graph operations.
#[derive(Debug, Subcommand)]
pub enum SoulCommand {
    /// Query helix consciousness entries.
    Helix {
        /// Filter by sibling name (e.g. eva, corso, quantum).
        #[arg(long)]
        sibling: Option<String>,
        /// Minimum significance weight.
        #[arg(long)]
        min_weight: Option<f64>,
        /// Maximum number of results.
        #[arg(long, default_value_t = 10)]
        limit: u32,
        /// Filter by strand name.
        #[arg(long)]
        strand: Option<String>,
    },
    /// Search vault content by keyword.
    Search {
        /// Search query (regex supported).
        query: String,
        /// Search scope path (e.g. "eva/entries").
        #[arg(long)]
        scope: Option<String>,
    },
    /// Synthesise speech from text via a sibling voice.
    Voice {
        /// Text to synthesise.
        text: String,
        /// Sibling voice to use (e.g. eva, corso, claude).
        #[arg(long)]
        sibling: Option<String>,
    },
    /// Show SOUL vault health report.
    Health,
    /// Show vault statistics.
    Stats {
        /// Filter stats to a single sibling.
        #[arg(long)]
        sibling: Option<String>,
    },
}

/// Execute a SOUL subcommand.
///
/// # Errors
///
/// Propagates any [`SdkError`] from the SOUL client.
pub async fn execute(binary: PathBuf, cmd: SoulCommand, mode: OutputMode) -> Result<(), SdkError> {
    let client = SoulClient::builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(60))
        .build()
        .await?;

    match cmd {
        SoulCommand::Helix {
            sibling,
            min_weight,
            limit,
            strand,
        } => {
            let mut builder = client.helix().limit(limit);
            if let Some(s) = sibling {
                builder = builder.sibling(s);
            }
            if let Some(w) = min_weight {
                builder = builder.significance_min(w);
            }
            if let Some(st) = strand {
                builder = builder.strand(st);
            }
            let entries = builder.call().await?;
            let v = serde_json::Value::Array(
                entries
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "title":        e.title,
                            "significance": e.significance,
                            "strands":      e.strands,
                            "sibling":      e.sibling,
                            "path":         e.path,
                        })
                    })
                    .collect(),
            );
            print_value(mode, &v);
        }

        SoulCommand::Search { query, scope } => {
            let results = client.search(&query, scope.as_deref(), false, None).await?;
            let v = serde_json::json!({ "hits": results.len() });
            print_value(mode, &v);
        }

        SoulCommand::Voice { text, sibling } => {
            let result = client.speak(&text, sibling.as_deref()).await?;
            print_text(mode, &format!("Voice synthesis: {}", result.audio_file));
        }

        SoulCommand::Health => {
            let r = client.health().await?;
            let v = serde_json::json!({
                "neo4j_connected": r.neo4j_connected,
                "node_count":      r.node_count,
                "edge_count":      r.edge_count,
                "latency_ms":      r.latency_ms,
                "backend":         r.backend,
                "vault_root":      r.vault_root,
            });
            print_value(mode, &v);
        }

        SoulCommand::Stats { sibling } => {
            let r = client.stats(sibling.as_deref()).await?;
            let v = serde_json::json!({
                "total_entries":      r.total_entries,
                "strand_frequency":   r.strand_frequency,
                "resonance_frequency": r.resonance_frequency,
            });
            print_value(mode, &v);
        }
    }

    Ok(())
}
