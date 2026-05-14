//! Shared prompt-building and error-mapping helpers for inline sibling handlers.
//!
//! Centralises two functions that were duplicated across every LLM-dispatch
//! handler (G4-M2 tracked debt). Import via
//! `use super::common::{build_prompt, map_provider_error};`.

use lightarchitects::core::handler::HandlerError;
use serde_json::Value;

use crate::spawner::llm_agent::ProviderError;

/// Maximum bytes allowed for pretty-printed params before prompt construction.
///
/// Headroom below `MAX_PARAM_BYTES` (8192) to leave room for the action header.
pub(super) const MAX_PARAMS_PRETTY_BYTES: usize = 4_096;

/// Build the LLM prompt for a dispatched sibling action.
///
/// # Errors
///
/// Returns [`HandlerError::InvalidParams`] if the pretty-printed params exceed
/// [`MAX_PARAMS_PRETTY_BYTES`]. This guards against params that are compact as
/// JSON Values but expand significantly when pretty-printed (G1 / HIGH-2).
pub(super) fn build_prompt(
    sibling: &str,
    action: &str,
    params: &Value,
) -> Result<String, HandlerError> {
    let params_str = serde_json::to_string_pretty(params).unwrap_or_else(|_| "{}".to_owned());
    if params_str.len() > MAX_PARAMS_PRETTY_BYTES {
        return Err(HandlerError::invalid_params(
            sibling,
            action,
            format!(
                "params payload too large after serialization ({} > {MAX_PARAMS_PRETTY_BYTES} bytes)",
                params_str.len()
            ),
        ));
    }
    Ok(format!("Action: {action}\n\nParameters:\n{params_str}"))
}

/// Map a [`ProviderError`] to the appropriate [`HandlerError`] variant.
pub(super) fn map_provider_error(sibling: &str, action: &str, e: ProviderError) -> HandlerError {
    match e {
        ProviderError::ParamSanitizationFailed { param_name, reason } => {
            HandlerError::invalid_params(sibling, action, format!("{param_name}: {reason}"))
        }
        ProviderError::Internal(msg) => HandlerError::internal(sibling, action, msg),
        other => HandlerError::service_error(sibling, action, other.to_string()),
    }
}
