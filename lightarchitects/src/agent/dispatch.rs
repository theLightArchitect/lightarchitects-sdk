//! Consolidated dispatch helper for inline sibling handlers.
//!
//! [`dispatch_action`] replaces the 12-line pattern that was duplicated across
//! every LLM-dispatch handler (`build_prompt` + `AgentRequest` construction +
//! `spawn` + error mapping).

use serde_json::Value;

use crate::core::handler::HandlerError;

use super::provider::{AgentRequest, LlmAgentProvider, ProviderError};

/// Maximum bytes allowed for pretty-printed params before prompt construction.
///
/// Headroom below `MAX_PARAM_BYTES` (8192) to leave room for the action header.
const MAX_PARAMS_PRETTY_BYTES: usize = 4_096;

/// Build the LLM prompt for a dispatched sibling action.
fn build_prompt(sibling: &str, action: &str, params: &Value) -> Result<String, HandlerError> {
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
fn map_provider_error(sibling: &str, action: &str, e: ProviderError) -> HandlerError {
    match e {
        ProviderError::ParamSanitizationFailed { param_name, reason } => {
            HandlerError::invalid_params(sibling, action, format!("{param_name}: {reason}"))
        }
        ProviderError::Internal(msg) => HandlerError::internal(sibling, action, msg),
        other => HandlerError::service_error(sibling, action, other.to_string()),
    }
}

/// Dispatch a sibling action through an [`LlmAgentProvider`].
///
/// Constructs the prompt, wraps it in an [`AgentRequest`], calls
/// `provider.spawn`, and maps any [`ProviderError`] to [`HandlerError`].
///
/// # Errors
///
/// Returns [`HandlerError::InvalidParams`] if the params payload exceeds
/// [`MAX_PARAMS_PRETTY_BYTES`], or a mapped [`HandlerError`] if the provider
/// returns a [`ProviderError`].
pub async fn dispatch_action(
    provider: &dyn LlmAgentProvider,
    sibling: &str,
    action: &str,
    params: &Value,
    identity: &str,
    max_budget_usd: f64,
) -> Result<Value, HandlerError> {
    let prompt = build_prompt(sibling, action, params)?;

    let req = AgentRequest {
        sibling_identity: identity.to_owned(),
        user_prompt: prompt,
        schema: None,
        allowed_tools: vec![],
        max_turns: 5,
        max_budget_usd,
        model_hint: None,
        parent_span_id: None,
    };

    let resp = provider
        .spawn(req)
        .await
        .map_err(|e| map_provider_error(sibling, action, e))?;

    Ok(resp.output)
}
