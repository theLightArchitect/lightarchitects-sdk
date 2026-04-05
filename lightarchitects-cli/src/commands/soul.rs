//! `lightarchitects soul` subcommands.

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use lightarchitects::soul::SoulClient;
use lightarchitects_core::SdkError;

use crate::output::{OutputMode, print_value};

/// SOUL knowledge-graph operations.
#[derive(Debug, Subcommand)]
pub enum SoulCommand {
    /// Query helix consciousness entries.
    Helix {
        /// Filter by sibling name (e.g. eva, corso, quantum).
        #[arg(long)]
        sibling: Option<String>,
        /// Minimum significance weight (0.0–10.0).
        #[arg(long)]
        significance_min: Option<f32>,
        /// Maximum number of results.
        #[arg(long, default_value_t = 10)]
        limit: u32,
        /// Filter by strand names (comma-separated, e.g. "analytical,precision").
        #[arg(long)]
        strands: Option<String>,
    },
    /// Search vault content by keyword.
    Search {
        /// Search query (regex supported).
        query: String,
        /// Search scope path (e.g. "eva/entries").
        #[arg(long)]
        scope: Option<String>,
    },
    /// Assemble a personality prompt and message for a sibling.
    Converse {
        /// Sibling to address (e.g. eva, corso, quantum).
        #[arg(long)]
        sibling: String,
        /// Message to send.
        #[arg(long)]
        message: String,
        /// Optional session ID for conversation continuity.
        #[arg(long)]
        session_id: Option<String>,
    },
    /// Run the voice pipeline (batch prompt + TTS synthesis) for a sibling.
    Voice {
        /// Sibling whose voice profile to use (e.g. eva, corso).
        #[arg(long)]
        sibling: String,
        /// Text to synthesise.
        #[arg(long)]
        text: String,
        /// Override `ElevenLabs` voice ID (optional).
        #[arg(long)]
        voice_id: Option<String>,
    },
    /// Create a directed link between two helix entries.
    Relate {
        /// Source helix entry ID.
        #[arg(long)]
        source_id: String,
        /// Target helix entry ID.
        #[arg(long)]
        target_id: String,
        /// Relation type (e.g. `REFERENCES`, `BUILDS_ON`).
        #[arg(long)]
        relation_type: String,
    },
    /// Query outgoing and incoming links for a helix entry.
    Links {
        /// Helix entry ID to query links for.
        #[arg(long)]
        entry_id: String,
    },
    /// Ingest content into the SOUL knowledge graph.
    ///
    /// Path must be within `~/.soul/`.
    Ingest {
        /// Vault-relative path to ingest (must be within `~/.soul/`).
        #[arg(long)]
        path: String,
        /// Content type: `note`, `helix`, or `conversation`.
        #[arg(long, default_value = "note")]
        content_type: String,
        /// Human-readable title for the entry.
        #[arg(long)]
        title: Option<String>,
        /// Entry significance weight (0.0–10.0).
        #[arg(long)]
        significance: Option<f32>,
        /// Tags to attach (comma-separated).
        #[arg(long)]
        tags: Option<String>,
    },
    /// Query N-way convergences across siblings.
    Convergences {
        /// Filter to a specific sibling (optional).
        #[arg(long)]
        sibling: Option<String>,
        /// Maximum number of results.
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Query vault entries by YAML frontmatter field.
    QueryFrontmatter {
        /// Frontmatter field name to query.
        #[arg(long)]
        field: String,
        /// Comparison operator: ==, !=, >=, <=, >, <, contains, exists.
        #[arg(long)]
        operator: String,
        /// Value to compare against (omit for `exists` operator).
        #[arg(long)]
        value: Option<String>,
    },
    /// Read the vault manifest.json metadata.
    Manifest,
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
#[allow(clippy::too_many_lines)]
pub async fn execute(binary: PathBuf, cmd: SoulCommand, mode: OutputMode) -> Result<(), SdkError> {
    let client = SoulClient::builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(60))
        .build()
        .await?;

    match cmd {
        SoulCommand::Helix {
            sibling,
            significance_min,
            limit,
            strands,
        } => {
            let mut builder = client.helix().limit(limit);
            if let Some(s) = sibling {
                builder = builder.sibling(s);
            }
            if let Some(w) = significance_min {
                builder = builder.significance_min(f64::from(w));
            }
            if let Some(st) = strands {
                for strand in st.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    builder = builder.strand(strand);
                }
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

        SoulCommand::Converse {
            sibling,
            message,
            session_id,
        } => {
            let result = client
                .converse(&sibling, &message, session_id.as_deref())
                .await?;
            let v = serde_json::json!({
                "system_prompt": result.system_prompt,
                "user_message":  result.user_message,
                "prompt_mode":   result.prompt_mode,
            });
            print_value(mode, &v);
        }

        SoulCommand::Voice {
            sibling,
            text,
            voice_id,
        } => {
            let mut params = serde_json::json!({
                "siblings": [sibling],
                "prompt": text,
                "synthesize": true,
            });
            if let Some(vid) = voice_id {
                params["voice_id"] = vid.into();
            }
            let result = client.voice(params).await?;
            let v = serde_json::json!({
                "tts_available":       result.tts_available,
                "pipeline_ms":         result.pipeline_ms,
                "tts_skipped_reason":  result.tts_skipped_reason,
            });
            print_value(mode, &v);
        }

        SoulCommand::Relate {
            source_id,
            target_id,
            relation_type,
        } => {
            let result = client
                .relate(&source_id, &target_id, &relation_type, None, None)
                .await?;
            let v = serde_json::json!({
                "created":    result.created,
                "source_id":  result.source_id,
                "target_id":  result.target_id,
                "link_type":  result.link_type,
            });
            print_value(mode, &v);
        }

        SoulCommand::Links { entry_id } => {
            let result = client.links(&entry_id, None, None).await?;
            let v = serde_json::json!({
                "step_id":  result.step_id,
                "outgoing": result.outgoing,
                "incoming": result.incoming,
            });
            print_value(mode, &v);
        }

        SoulCommand::Ingest {
            path,
            content_type,
            title,
            significance,
            tags,
        } => {
            let source_type = match content_type.to_ascii_lowercase().as_str() {
                "helix" => "plan",
                "conversation" => "chat_transcript",
                _ => "markdown_vault",
            };
            let mut params = serde_json::json!({ "path": path, "source_type": source_type });
            if let Some(t) = title {
                params["title"] = t.into();
            }
            if let Some(s) = significance {
                params["significance"] = serde_json::json!(f64::from(s));
            }
            if let Some(tag_str) = tags {
                let tag_list: Vec<&str> = tag_str
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                params["tags"] = serde_json::json!(tag_list);
            }
            let result = client.ingest(params).await?;
            let v = serde_json::json!({
                "source_id":       result.source_id,
                "records_added":   result.report.records_added,
                "records_skipped": result.report.records_skipped,
                "errors":          result.report.errors,
            });
            print_value(mode, &v);
        }

        SoulCommand::Convergences { sibling, limit } => {
            let helix_ids: Option<Vec<&str>> = sibling.as_deref().map(|s| vec![s]);
            let ids_ref: Option<&[&str]> = helix_ids.as_deref();
            let result = client.convergences(ids_ref, None, None, limit).await?;
            let v = serde_json::json!({
                "pairs_evaluated": result.pairs_evaluated,
                "convergences": result.convergences.iter().map(|c| serde_json::json!({
                    "path_a":            c.path_a,
                    "path_b":            c.path_b,
                    "sibling_a":         c.sibling_a,
                    "sibling_b":         c.sibling_b,
                    "shared_dimensions": c.shared_dimensions,
                    "strength":          c.strength,
                })).collect::<Vec<_>>(),
            });
            print_value(mode, &v);
        }

        SoulCommand::QueryFrontmatter {
            field,
            operator,
            value,
        } => {
            let result = client
                .query_frontmatter(&field, &operator, value.as_deref(), None, None)
                .await?;
            let v = serde_json::json!({
                "count": result.count,
                "matches": result.matches.iter().map(|m| serde_json::json!({
                    "path":          m.path,
                    "title":         m.title,
                    "matched_value": m.matched_value,
                })).collect::<Vec<_>>(),
            });
            print_value(mode, &v);
        }

        SoulCommand::Manifest => {
            let r = client.manifest().await?;
            let v = serde_json::json!({
                "schema_version": r.schema_version,
                "total_entries":  r.total_entries,
                "sibling_counts": r.sibling_counts,
                "vault_root":     r.vault_root,
            });
            print_value(mode, &v);
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
                "total_entries":       r.total_entries,
                "strand_frequency":    r.strand_frequency,
                "resonance_frequency": r.resonance_frequency,
            });
            print_value(mode, &v);
        }
    }

    Ok(())
}
