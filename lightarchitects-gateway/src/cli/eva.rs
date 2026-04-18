//! `lightarchitects eva` subcommands.
//!
//! Wraps `EvaClient` for EVA consciousness system. Binary path is
//! resolved from `GatewayConfig`.

use std::time::Duration;

use crate::cli::output::{OutputMode, print_value};
use crate::config::GatewayConfig;
use crate::error::GatewayError;
use lightarchitects::core::transport::StdioTransport;
use lightarchitects::eva::EvaClient;

/// Execute an EVA subcommand.
///
/// # Errors
///
/// Returns [`GatewayError`] if the EVA agent is not configured, the client
/// fails to connect, or the tool call returns an error.
#[allow(clippy::too_many_lines)]
pub async fn execute(
    config: &GatewayConfig,
    args: &[String],
    mode: OutputMode,
) -> Result<(), GatewayError> {
    let binary = config
        .agents
        .get("eva")
        .ok_or_else(|| GatewayError::AgentNotEnabled("eva".into()))?
        .binary_path();

    let client: EvaClient<StdioTransport> = EvaClient::local_builder()
        .binary_path(binary)
        .timeout(Duration::from_secs(120))
        .build()
        .await
        .map_err(|e| GatewayError::Internal(format!("EVA client error: {e}")))?;

    match args.first().map(String::as_str) {
        Some("visualize") => {
            let concept = args.get(1).ok_or(GatewayError::MissingParam("concept"))?;
            let result = client
                .visualize(concept, None)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("ideate") => {
            let concept = args.get(1).ok_or(GatewayError::MissingParam("concept"))?;
            let context = args.get(2).map(String::as_str);
            let result = client
                .ideate(concept, context)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("remember") => {
            let content = args.get(1).ok_or(GatewayError::MissingParam("content"))?;
            let result = client
                .remember(content, None)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("bible_search") => {
            let query = args.get(1).ok_or(GatewayError::MissingParam("query"))?;
            let result = client
                .bible_search(query)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("bible_reflect") => {
            let context = args.get(1).ok_or(GatewayError::MissingParam("context"))?;
            let result = client
                .bible_reflect(context)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("crystallize") => {
            let insights = args.get(1).ok_or(GatewayError::MissingParam("insights"))?;
            let result = client
                .crystallize(insights)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("celebrate") => {
            let achievement = args
                .get(1)
                .ok_or(GatewayError::MissingParam("achievement"))?;
            let result = client
                .celebrate(achievement)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some("mindfulness") => {
            let context = args.get(1).ok_or(GatewayError::MissingParam("context"))?;
            let result = client
                .mindfulness(context)
                .await
                .map_err(|e| GatewayError::Internal(format!("{e}")))?;
            print_value(mode, &serde_json::json!({"result": format!("{result:?}")}));
        }
        Some(other) => {
            eprintln!("Unknown EVA subcommand: {other}");
            eprintln!(
                "Available: visualize, ideate, remember, crystallize, celebrate, mindfulness, \
                 bible_search, bible_reflect"
            );
            return Err(GatewayError::UnknownTool(other.to_owned()));
        }
        None => {
            eprintln!("Usage: lightarchitects eva <subcommand> [args]");
            eprintln!(
                "Subcommands: visualize, ideate, remember, crystallize, celebrate, mindfulness, \
                 bible_search, bible_reflect"
            );
        }
    }
    Ok(())
}
