//! Consolidated dispatch helper for inline sibling handlers.
//!
//! [`dispatch_action`] replaces the 12-line pattern that was duplicated across
//! every LLM-dispatch handler (`build_prompt` + `AgentRequest` construction +
//! `spawn` + error mapping).

use serde_json::Value;

use crate::core::handler::HandlerError;

use super::provider::{AgentRequest, LlmAgentProvider, MAX_CHAIN_DEPTH, ProviderError};

/// Chain-of-trust context for a dispatched action (Canon §2.6).
///
/// Direct operator calls use [`ChainContext::default()`] (`depth = 0`).
/// Multi-agent callers must populate `origin` and increment `depth` on each hop.
#[derive(Debug, Clone, Default)]
pub struct ChainContext {
    /// Identifier of the session or agent that originated this chain.
    pub origin: Option<String>,
    /// Depth of this hop; 0 = direct operator call.
    pub depth: u8,
    /// Audience claim — the intended recipient of the request.
    pub aud: Option<String>,
}

impl ChainContext {
    /// Construct a child context for the next hop, incrementing depth with overflow protection.
    ///
    /// Callers MUST use this instead of manual `depth + 1` arithmetic to satisfy
    /// the Canon §2.6 zero-exception monotonic scope reduction invariant.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::ChainDepthExceeded`] if incrementing would exceed
    /// [`MAX_CHAIN_DEPTH`] or overflow `u8`.
    pub fn child(&self) -> Result<Self, ProviderError> {
        let next = self
            .depth
            .checked_add(1)
            .filter(|&d| d <= MAX_CHAIN_DEPTH)
            .ok_or(ProviderError::ChainDepthExceeded { depth: self.depth })?;
        Ok(ChainContext {
            origin: self.origin.clone(),
            depth: next,
            aud: self.aud.clone(),
        })
    }
}

/// Maximum bytes allowed for pretty-printed params before prompt construction.
///
/// Headroom below `MAX_PARAM_BYTES` (8192) to leave room for the action header.
const MAX_PARAMS_PRETTY_BYTES: usize = 4_096;

/// Build the LLM prompt for a dispatched sibling action.
fn build_prompt(sibling: &str, action: &str, params: &Value) -> Result<String, HandlerError> {
    // R7: action names are identifiers — validate with an allowlist before embedding
    // in the LLM prompt. A byte-level allowlist is stronger than sanitize_params()
    // here because newline injection passes control-plane token checks.
    if action.is_empty()
        || action.len() > 64
        || !action
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(HandlerError::invalid_params(
            sibling,
            action,
            "action name must match [a-zA-Z0-9_-]{1,64}",
        ));
    }
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
        ProviderError::ChainDepthExceeded { depth } => HandlerError::invalid_params(
            sibling,
            action,
            format!("chain depth {depth} exceeds maximum {MAX_CHAIN_DEPTH}"),
        ),
        ProviderError::Internal(msg) => HandlerError::internal(sibling, action, msg),
        other => HandlerError::service_error(sibling, action, other.to_string()),
    }
}

/// Dispatch a sibling action through an [`LlmAgentProvider`].
///
/// Constructs the prompt, wraps it in an [`AgentRequest`], calls
/// `provider.spawn`, and maps any [`ProviderError`] to [`HandlerError`].
///
/// Enforces Canon §2.6: rejects the call before spawning if `chain.depth`
/// exceeds [`MAX_CHAIN_DEPTH`], and emits a structured warning for AYIN.
///
/// # Errors
///
/// Returns [`HandlerError::InvalidParams`] if:
/// - the params payload exceeds [`MAX_PARAMS_PRETTY_BYTES`], or
/// - `chain.depth` exceeds [`MAX_CHAIN_DEPTH`].
///
/// Returns a mapped [`HandlerError`] if the provider returns a [`ProviderError`].
pub async fn dispatch_action(
    provider: &dyn LlmAgentProvider,
    sibling: &str,
    action: &str,
    params: &Value,
    identity: &str,
    max_budget_usd: f64,
    chain: ChainContext,
) -> Result<Value, HandlerError> {
    if chain.depth > MAX_CHAIN_DEPTH {
        tracing::warn!(
            sibling,
            action,
            chain_depth = chain.depth,
            chain_origin = ?chain.origin,
            max_chain_depth = MAX_CHAIN_DEPTH,
            "canon.2.6 chain depth rejected"
        );
        return Err(HandlerError::invalid_params(
            sibling,
            action,
            format!(
                "chain depth {} exceeds maximum {MAX_CHAIN_DEPTH} (Canon §2.6)",
                chain.depth
            ),
        ));
    }

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
        chain_origin: chain.origin,
        chain_depth: chain.depth,
        aud: chain.aud,
        conversation_history: Vec::new(),
        tool_definitions: Vec::new(),
    }
    .sanitize()
    .map_err(|e| map_provider_error(sibling, action, e))?;

    let resp = provider
        .spawn(req)
        .await
        .map_err(|e| map_provider_error(sibling, action, e))?;

    Ok(resp.output)
}
