//! `lightarchitects seraph` subcommands.
//!
//! Wraps `SeraphClient` for SERAPH pentest orchestration. Binary path is
//! resolved from `GatewayConfig`. Pre-validates targets via `ScopeConstraint`
//! before spawning SERAPH.

use std::time::Duration;

use crate::cli::output::{OutputMode, print_value};
use crate::config::GatewayConfig;
use crate::error::GatewayError;
use lightarchitects::core::transport::StdioTransport;
use lightarchitects::seraph::SeraphClient;
use lightarchitects::seraph::scope::{ScopeConstraint, ScopeDomain};

/// Execute a SERAPH subcommand.
///
/// # Errors
///
/// Returns [`GatewayError`] if the SERAPH agent is not configured, the client
/// fails to connect, or the tool call returns an error.
pub async fn execute(
    config: &GatewayConfig,
    args: &[String],
    mode: OutputMode,
) -> Result<(), GatewayError> {
    let binary = config
        .agents
        .get("seraph")
        .ok_or_else(|| GatewayError::AgentNotEnabled("seraph".into()))?
        .binary_path();

    let client: SeraphClient<StdioTransport> = SeraphClient::builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(180))
        .build()
        .await
        .map_err(|e| GatewayError::Internal(format!("SERAPH client error: {e}")))?;

    match args.first().map(String::as_str) {
        Some("recon") => {
            let target = args.get(1).ok_or(GatewayError::MissingParam("target"))?;
            validate_target(target)?;
            let result = client
                .recon(target)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("survey") => {
            let target = args.get(1).ok_or(GatewayError::MissingParam("target"))?;
            validate_target(target)?;
            let result = client
                .survey(target)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("examine") => {
            let target = args.get(1).ok_or(GatewayError::MissingParam("target"))?;
            validate_target(target)?;
            let result = client
                .examine(target)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("strike") => {
            let target = args.get(1).ok_or(GatewayError::MissingParam("target"))?;
            validate_target(target)?;
            let result = client
                .strike(target)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("osint") => {
            let target = args.get(1).ok_or(GatewayError::MissingParam("target"))?;
            let depth = args.get(2).map(String::as_str);
            let result = client
                .osint(target, depth)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("status") => {
            let result = client
                .status()
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some(other) => {
            eprintln!("Unknown SERAPH subcommand: {other}");
            eprintln!("Available: recon, survey, examine, strike, osint, status, vault_sync");
            return Err(GatewayError::UnknownTool(other.to_owned()));
        }
        None => {
            eprintln!("Usage: lightarchitects seraph <subcommand> [args]");
            eprintln!("Subcommands: recon, survey, examine, strike, osint, status, vault_sync");
        }
    }
    Ok(())
}

/// Validate a target through SERAPH scope governance.
///
/// # Errors
///
/// Returns [`GatewayError::InvalidParam`] if the target fails scope validation.
fn validate_target(target: &str) -> Result<(), GatewayError> {
    ScopeConstraint::new(target, "scan", ScopeDomain::Web)
        .map_err(|e| GatewayError::InvalidParam(format!("target '{target}' rejected: {e}")))?;
    Ok(())
}
