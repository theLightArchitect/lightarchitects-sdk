//! `lightarchitects quantum` subcommands.
//!
//! Wraps `QuantumClient` for QUANTUM investigation toolkit. Binary path is
//! resolved from `GatewayConfig`.

use std::time::Duration;

use crate::cli::output::{OutputMode, print_value};
use crate::config::GatewayConfig;
use crate::error::GatewayError;
use lightarchitects::core::transport::StdioTransport;
use lightarchitects::quantum::QuantumClient;

/// Execute a QUANTUM subcommand.
///
/// # Errors
///
/// Returns [`GatewayError`] if the QUANTUM agent is not configured, the client
/// fails to connect, or the tool call returns an error.
#[allow(clippy::too_many_lines)]
pub async fn execute(
    config: &GatewayConfig,
    args: &[String],
    mode: OutputMode,
) -> Result<(), GatewayError> {
    let binary = config
        .agents
        .get("quantum")
        .ok_or_else(|| GatewayError::AgentNotEnabled("quantum".into()))?
        .binary_path();

    let client: QuantumClient<StdioTransport> = QuantumClient::builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(120))
        .build()
        .await
        .map_err(|e| GatewayError::Internal(format!("QUANTUM client error: {e}")))?;

    match args.first().map(String::as_str) {
        Some("triage") => {
            let subject = args.get(1).ok_or(GatewayError::MissingParam("subject"))?;
            let result = client
                .triage(subject)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("sweep") => {
            let subject = args.get(1).ok_or(GatewayError::MissingParam("subject"))?;
            let result = client
                .sweep(subject)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("trace") => {
            let subject = args.get(1).ok_or(GatewayError::MissingParam("subject"))?;
            let result = client
                .trace(subject)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("probe") => {
            let subject = args.get(1).ok_or(GatewayError::MissingParam("subject"))?;
            let result = client
                .probe(subject)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("theorize") => {
            let subject = args.get(1).ok_or(GatewayError::MissingParam("subject"))?;
            let context = args.get(2).map(String::as_str);
            let result = client
                .theorize(subject, context)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("verify") => {
            let hypothesis = args
                .get(1)
                .ok_or(GatewayError::MissingParam("hypothesis"))?;
            let result = client
                .verify(hypothesis)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("close") => {
            let subject = args.get(1).ok_or(GatewayError::MissingParam("subject"))?;
            let result = client
                .close(subject)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("quick") => {
            let question = args.get(1).ok_or(GatewayError::MissingParam("question"))?;
            let result = client
                .quick(question)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("research") => {
            let query = args.get(1).ok_or(GatewayError::MissingParam("query"))?;
            let result = client
                .research(query)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some(other) => {
            eprintln!("Unknown QUANTUM subcommand: {other}");
            eprintln!(
                "Available: triage, sweep, trace, probe, theorize, verify, close, quick, research"
            );
            return Err(GatewayError::UnknownTool(other.to_owned()));
        }
        None => {
            eprintln!("Usage: lightarchitects quantum <subcommand> [args]");
            eprintln!(
                "Subcommands: triage, sweep, trace, probe, theorize, verify, close, quick, research"
            );
        }
    }
    Ok(())
}
