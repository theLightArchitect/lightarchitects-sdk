//! LLM agent provider trait and associated request/response types.
//!
//! [`LlmAgentProvider`] is the contract every provider must satisfy.
//! Inline sibling handlers dispatch LLM calls through the provider rather
//! than through the MCP subprocess spawner.
//!
//! # Security model
//!
//! - **G1**: All string parameters entering a provider MUST pass the two-plane
//!   sanitization defined in [`super::sanitize_params`]:
//!   control-plane (identity) → reject on dangerous tokens;
//!   content-plane (prompt) → escape and strip.
//! - **G4**: `parent_span_id` carries the W3C `traceparent` header value;
//!   providers SHOULD propagate it via the `TRACEPARENT` environment variable.
//! - **G10**: Providers that launch subprocesses MUST set `kill_on_drop(true)`
//!   and wrap execution in a hard `tokio::time::timeout`.

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use tracing::warn;

/// Maximum allowed byte length for control-plane or content-plane strings.
pub const MAX_PARAM_BYTES: usize = 8_192;

/// Maximum allowed depth for a multi-agent chain (Canon §2.6).
///
/// `dispatch_action` rejects any request whose `chain_depth` exceeds this value.
pub const MAX_CHAIN_DEPTH: u8 = 7;

/// Input to an LLM agent invocation.
#[derive(Debug, Clone)]
pub struct AgentRequest {
    /// System prompt / sibling identity (control-plane — G1 rejects dangerous tokens).
    pub sibling_identity: String,
    /// User-visible content prompt (content-plane — G1 escapes dangerous tokens).
    pub user_prompt: String,
    /// Optional JSON Schema the provider should validate the output against.
    pub schema: Option<Value>,
    /// Tool names the Claude CLI subprocess is permitted to use.
    pub allowed_tools: Vec<String>,
    /// Maximum conversation turns the subprocess may execute.
    pub max_turns: u32,
    /// Hard budget cap in USD; providers MUST enforce this before returning.
    pub max_budget_usd: f64,
    /// Optional model override (e.g. `"claude-sonnet-4-6"`).
    pub model_hint: Option<String>,
    /// W3C `traceparent` value for distributed tracing (G4).
    pub parent_span_id: Option<String>,
    /// Identifier of the session or agent that originated this chain (Canon §2.6).
    pub chain_origin: Option<String>,
    /// Depth of this call in a multi-agent chain; 0 = direct operator call (Canon §2.6).
    /// Must not exceed [`MAX_CHAIN_DEPTH`].
    pub chain_depth: u8,
    /// Audience claim — intended recipient of this request (Canon §2.6).
    pub aud: Option<String>,
}

impl AgentRequest {
    /// Apply G1 two-plane sanitization and return a [`SanitizedAgentRequest`].
    ///
    /// This is the only way to construct a `SanitizedAgentRequest`. Callers that
    /// hold one have compile-time proof that G1 sanitization has been applied.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::ParamSanitizationFailed`] if `sibling_identity`
    /// contains dangerous tokens or either field exceeds [`MAX_PARAM_BYTES`].
    pub fn sanitize(self) -> Result<SanitizedAgentRequest, ProviderError> {
        let (safe_identity, safe_prompt) =
            sanitize_params(&self.sibling_identity, &self.user_prompt)?;
        Ok(SanitizedAgentRequest {
            inner: self,
            safe_identity,
            safe_prompt,
        })
    }
}

/// A pre-sanitized agent request: compile-time proof that G1 has been applied.
///
/// Can only be constructed via [`AgentRequest::sanitize`]. Providers that accept
/// this type are guaranteed to receive only sanitized inputs.
pub struct SanitizedAgentRequest {
    inner: AgentRequest,
    safe_identity: String,
    safe_prompt: String,
}

impl SanitizedAgentRequest {
    /// The G1-sanitized system prompt (control-plane — dangerous tokens rejected).
    pub fn safe_identity(&self) -> &str {
        &self.safe_identity
    }

    /// The G1-sanitized user prompt (content-plane — escaped and stripped).
    pub fn safe_prompt(&self) -> &str {
        &self.safe_prompt
    }

    /// Borrow the underlying [`AgentRequest`].
    pub fn request(&self) -> &AgentRequest {
        &self.inner
    }

    /// Consume this wrapper and return the raw, **unsanitized** [`AgentRequest`].
    ///
    /// # Warning
    ///
    /// The returned [`AgentRequest`] exposes the original `sibling_identity` and
    /// `user_prompt` strings *before* G1 sanitization. This method exists for test
    /// inspection only — provider implementors MUST use [`safe_identity`] and
    /// [`safe_prompt`] when passing data to subprocesses.
    ///
    /// [`safe_identity`]: SanitizedAgentRequest::safe_identity
    /// [`safe_prompt`]: SanitizedAgentRequest::safe_prompt
    pub fn into_inner_unchecked(self) -> AgentRequest {
        self.inner
    }
}

/// Apply G1 two-plane sanitization to identity and prompt strings.
///
/// - `identity` (control-plane): reject on dangerous tokens.
/// - `prompt` (content-plane): escape `<`/`>` to HTML entities; strip RTL and
///   zero-width characters with a warning; length cap applied to both.
///
/// Returns `(sanitized_identity, sanitized_prompt)` on success.
///
/// # Errors
///
/// Returns [`ProviderError::ParamSanitizationFailed`] if the identity string
/// contains dangerous tokens, or either string exceeds [`MAX_PARAM_BYTES`].
pub fn sanitize_params(identity: &str, prompt: &str) -> Result<(String, String), ProviderError> {
    check_length("sibling_identity", identity)?;
    check_length("user_prompt", prompt)?;
    let safe_identity = reject_control_plane(identity)?;
    let safe_prompt = escape_content_plane(prompt);
    Ok((safe_identity, safe_prompt))
}

fn reject_control_plane(s: &str) -> Result<String, ProviderError> {
    const FORBIDDEN: &[(&str, &str)] = &[
        ("</system>", "XML system-close tag"),
        ("<system>", "XML system-open tag"),
        ("\u{202E}", "RTL override (U+202E)"),
        ("\x00", "null byte"),
        ("\u{200B}", "zero-width space (U+200B)"),
        ("\u{200C}", "zero-width non-joiner (U+200C)"),
        ("\u{200D}", "zero-width joiner (U+200D)"),
        ("\u{200E}", "left-to-right mark (U+200E)"),
        ("\u{200F}", "right-to-left mark (U+200F)"),
        ("\u{FEFF}", "BOM / zero-width no-break space (U+FEFF)"),
    ];
    for (token, description) in FORBIDDEN {
        if s.contains(token) {
            return Err(ProviderError::ParamSanitizationFailed {
                param_name: "sibling_identity".to_owned(),
                reason: format!("contains forbidden token: {description}"),
            });
        }
    }
    Ok(s.to_owned())
}

fn escape_content_plane(s: &str) -> String {
    strip_invisible(s).replace('<', "&lt;").replace('>', "&gt;")
}

fn strip_invisible(s: &str) -> String {
    const INVISIBLE: &[char] = &[
        '\u{202E}', '\u{200B}', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}', '\u{FEFF}',
    ];
    let result: String = s.chars().filter(|c| !INVISIBLE.contains(c)).collect();
    if result.len() != s.len() {
        warn!(
            original_len = s.len(),
            stripped_len = result.len(),
            "stripped invisible Unicode characters from content-plane param"
        );
    }
    result
}

fn check_length(param_name: &str, s: &str) -> Result<(), ProviderError> {
    if s.len() > MAX_PARAM_BYTES {
        return Err(ProviderError::ParamSanitizationFailed {
            param_name: param_name.to_owned(),
            reason: format!(
                "exceeds maximum byte length ({} > {MAX_PARAM_BYTES})",
                s.len()
            ),
        });
    }
    Ok(())
}

/// Token usage counters for a completed agent invocation.
#[derive(Debug, Clone)]
pub struct TokenUsage {
    /// Approximate input token count (estimated from byte length / 4).
    pub input: u32,
    /// Approximate output token count (estimated from byte length / 4).
    pub output: u32,
}

/// Result of a successful LLM agent invocation.
#[derive(Debug, Clone)]
pub struct AgentResponse {
    /// Parsed output value (JSON).
    pub output: Value,
    /// Number of turns the agent actually consumed.
    pub turns_used: u32,
    /// Gateway-computed cost in USD — **not** echoed from the API.
    pub cost_usd: f64,
    /// Token usage breakdown.
    pub tokens: TokenUsage,
    /// Provider-specific key/value attributes forwarded to AYIN span enrichment.
    pub provider_attrs: HashMap<String, Value>,
    /// Number of output-schema validation retries performed.
    pub retry_count: u8,
}

/// How strictly the provider enforces the caller-supplied output schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaMode {
    /// Provider rejects outputs that fail schema validation after retries.
    Strict,
    /// Provider attempts validation and retries, but returns best-effort output
    /// on persistent failure rather than erroring.
    BestEffort,
    /// No schema enforcement; output is returned as-is.
    None,
}

/// Capabilities advertised by a provider at registration time.
#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    /// How the provider handles caller-supplied output schemas.
    pub schema_enforcement: SchemaMode,
    /// Whether the provider natively enforces `max_budget_usd`.
    pub native_budget_cap: bool,
    /// Whether the provider natively enforces `max_turns`.
    pub native_turn_cap: bool,
    /// Whether authentication is inherited from the parent session (no extra creds).
    pub auth_inherits_session: bool,
}

/// Errors that can occur during an LLM agent invocation.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// The provider could not authenticate (missing credentials, expired token, etc.).
    #[error("authentication failure: {0}")]
    AuthFailure(String),

    /// The invocation was aborted because the estimated cost would exceed the cap.
    #[error("budget exceeded: cap=${cap_usd:.4}, actual=${actual_usd:.4}")]
    BudgetExceeded {
        /// The configured budget ceiling in USD.
        cap_usd: f64,
        /// The actual cost that would have been incurred.
        actual_usd: f64,
    },

    /// The subprocess consumed more turns than `max_turns` allowed.
    #[error("turns exceeded: cap={cap}")]
    TurnsExceeded {
        /// The configured turn ceiling.
        cap: u32,
    },

    /// Output schema validation failed on all retry attempts.
    #[error("schema validation failed after {retries} retries: {last_error}")]
    SchemaValidationFailed {
        /// Number of attempts made before giving up.
        retries: u32,
        /// Description of the last validation error.
        last_error: String,
    },

    /// A parameter failed G1 sanitization and cannot be forwarded to the subprocess.
    #[error("param sanitization failed for '{param_name}': {reason}")]
    ParamSanitizationFailed {
        /// Name of the failing parameter (`sibling_identity` or `user_prompt`).
        param_name: String,
        /// Human-readable rejection reason.
        reason: String,
    },

    /// The subprocess did not complete within the allowed wall-clock time.
    #[error("subprocess timeout after {used_turns} turns, ${used_budget_usd:.4}")]
    SubprocessTimeout {
        /// Turns consumed before the timeout fired.
        used_turns: u32,
        /// Estimated cost at timeout.
        used_budget_usd: f64,
    },

    /// The chain depth exceeds the platform maximum (Canon §2.6).
    #[error("chain depth {depth} exceeds maximum {MAX_CHAIN_DEPTH}")]
    ChainDepthExceeded {
        /// The depth value that triggered the rejection.
        depth: u8,
    },

    /// An unexpected internal error occurred.
    #[error("internal: {0}")]
    Internal(String),

    /// Spawn was rejected because `require_permission_matrix` is set but no
    /// [`crate::agent::permissions::PermissionMatrix`] was provided.
    #[error("a PermissionMatrix is required but was not provided")]
    MissingPermissionMatrix,
}

/// Contract for a provider that can spawn an LLM agent and return structured output.
///
/// Implementors MUST:
/// - Apply G1 sanitization before passing strings to any subprocess.
/// - Enforce `max_budget_usd` and `max_turns` from [`AgentRequest`].
/// - Set `kill_on_drop(true)` on any `tokio::process::Command` they spawn (G10).
/// - Propagate `parent_span_id` as the `TRACEPARENT` environment variable (G4).
#[async_trait]
pub trait LlmAgentProvider: Send + Sync {
    /// Human-readable provider identifier (e.g. `"claude-cli"`).
    fn name(&self) -> &'static str;

    /// Spawn the agent with the given pre-sanitized request and await its output.
    ///
    /// Accepts only a [`SanitizedAgentRequest`] — holding one is compile-time
    /// proof that G1 sanitization has already been applied. Providers MUST NOT
    /// re-sanitize inputs; they may use [`SanitizedAgentRequest::safe_identity`]
    /// and [`SanitizedAgentRequest::safe_prompt`] directly.
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] if subprocess execution, budget/turn
    /// enforcement, or schema validation fails.
    async fn spawn(&self, req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError>;

    /// Declare the capabilities of this provider.
    fn capabilities(&self) -> ProviderCapabilities;

    /// Estimate the USD cost for a call with the given token counts.
    ///
    /// Implementations MUST use gateway-owned rate tables, NOT echo values
    /// from the upstream API.
    fn estimate_cost(&self, input_tokens: u32, max_output_tokens: u32) -> f64;
}
