//! `lightarchitects corso` subcommands.
//!
//! Wraps `CorsoClient` for CORSO operations platform. Binary path is
//! resolved from `GatewayConfig`.

use std::time::Duration;

use crate::cli::output::{OutputMode, print_value};
use crate::config::GatewayConfig;
use crate::error::GatewayError;
use lightarchitects::core::transport::StdioTransport;
use lightarchitects::corso::CorsoClient;

/// Execute a CORSO subcommand.
///
/// # Errors
///
/// Returns [`GatewayError`] if the CORSO agent is not configured, the client
/// fails to connect, or the tool call returns an error.
pub async fn execute(
    config: &GatewayConfig,
    args: &[String],
    mode: OutputMode,
) -> Result<(), GatewayError> {
    let binary = config
        .agents
        .get("corso")
        .ok_or_else(|| GatewayError::AgentNotEnabled("corso".into()))?
        .binary_path();

    let client: CorsoClient<StdioTransport> = CorsoClient::local_builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(120))
        .build()
        .await
        .map_err(|e| GatewayError::Internal(format!("CORSO client error: {e}")))?;

    match args.first().map(String::as_str) {
        Some("guard") => {
            let target = args.get(1).ok_or(GatewayError::MissingParam("target"))?;
            let result = client
                .guard(target)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("fetch") => {
            let query = args.get(1).ok_or(GatewayError::MissingParam("query"))?;
            let result = client
                .fetch(query)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("sniff") => {
            let target = args.get(1).ok_or(GatewayError::MissingParam("target"))?;
            let result = client
                .sniff(target)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("generate") => {
            let spec = args.get(1).ok_or(GatewayError::MissingParam("spec"))?;
            let result = client
                .generate_code(spec)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("docs") => {
            let query = args.get(1).ok_or(GatewayError::MissingParam("query"))?;
            let result = client
                .search_documentation(query)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some(other) => {
            eprintln!("Unknown CORSO subcommand: {other}");
            eprintln!("Available: guard, fetch, sniff, generate, docs");
            return Err(GatewayError::UnknownTool(other.to_owned()));
        }
        None => {
            eprintln!("Usage: lightarchitects corso <subcommand> [args]");
            eprintln!("Subcommands: guard, fetch, sniff, generate, docs");
        }
    }
    Ok(())
}
