//! `lightarchitects soul` subcommands.
//!
//! Wraps `SoulClient` for SOUL knowledge-graph operations. Binary path is
//! resolved from `GatewayConfig`.

use std::time::Duration;

use crate::cli::output::{OutputMode, print_value};
use crate::config::GatewayConfig;
use crate::error::GatewayError;
use lightarchitects::core::transport::StdioTransport;
use lightarchitects::soul::SoulClient;

/// Execute a SOUL subcommand.
///
/// # Errors
///
/// Returns [`GatewayError`] if the SOUL agent is not configured, the client
/// fails to connect, or the tool call returns an error.
#[allow(clippy::too_many_lines)]
pub async fn execute(
    config: &GatewayConfig,
    args: &[String],
    mode: OutputMode,
) -> Result<(), GatewayError> {
    let binary = config
        .agents
        .get("soul")
        .ok_or_else(|| GatewayError::AgentNotEnabled("soul".into()))?
        .binary_path();

    let client: SoulClient<StdioTransport> = SoulClient::local_builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(60))
        .build()
        .await
        .map_err(|e| GatewayError::Internal(format!("SOUL client error: {e}")))?;

    match args.first().map(String::as_str) {
        Some("health") => {
            let r = client
                .health()
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
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
        Some("stats") => {
            let sibling = args.get(1).cloned();
            let r = client
                .stats(sibling.as_deref())
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            let v = serde_json::json!({
                "total_entries":       r.total_entries,
                "strand_frequency":    r.strand_frequency,
                "resonance_frequency": r.resonance_frequency,
            });
            print_value(mode, &v);
        }
        Some("manifest") => {
            let r = client
                .manifest()
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            let v = serde_json::json!({
                "schema_version": r.schema_version,
                "total_entries":  r.total_entries,
                "sibling_counts": r.sibling_counts,
                "vault_root":     r.vault_root,
            });
            print_value(mode, &v);
        }
        Some("search") => {
            let query = args.get(1).ok_or(GatewayError::MissingParam("query"))?;
            let scope = args.get(2).cloned();
            let results = client
                .search(query, scope.as_deref(), false, None)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            let v = serde_json::json!({ "hits": results.len() });
            print_value(mode, &v);
        }
        Some("helix") => {
            let mut builder = client.helix().limit(10);
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--sibling" => {
                        if let Some(s) = args.get(i + 1) {
                            builder = builder.sibling(s);
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    "--significance-min" => {
                        if let Some(w) = args.get(i + 1) {
                            if let Ok(weight) = w.parse::<f64>() {
                                builder = builder.significance_min(weight);
                            }
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    "--limit" => {
                        if let Some(n) = args.get(i + 1) {
                            if let Ok(limit) = n.parse::<u32>() {
                                builder = builder.limit(limit);
                            }
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    "--strands" => {
                        if let Some(st) = args.get(i + 1) {
                            for strand in st.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                                builder = builder.strand(strand);
                            }
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            let entries = builder
                .call()
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
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
        Some(other) => {
            eprintln!("Unknown SOUL subcommand: {other}");
            eprintln!(
                "Available: health, stats, manifest, search, helix, convergences, converse, \
                 voice, relate, links, ingest, query_frontmatter"
            );
            return Err(GatewayError::UnknownTool(other.to_owned()));
        }
        None => {
            eprintln!("Usage: lightarchitects soul <subcommand> [args]");
            eprintln!(
                "Subcommands: health, stats, manifest, search, helix, convergences, converse, \
                 voice, relate, links, ingest, query_frontmatter"
            );
        }
    }
    Ok(())
}
