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

    /// An unexpected internal error occurred.
    #[error("internal: {0}")]
    Internal(String),
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

    /// Spawn the agent with the given request and await its output.
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] if sanitization, subprocess execution,
    /// budget/turn enforcement, or schema validation fails.
    async fn spawn(&self, req: AgentRequest) -> Result<AgentResponse, ProviderError>;

    /// Declare the capabilities of this provider.
    fn capabilities(&self) -> ProviderCapabilities;

    /// Estimate the USD cost for a call with the given token counts.
    ///
    /// Implementations MUST use gateway-owned rate tables, NOT echo values
    /// from the upstream API.
    fn estimate_cost(&self, input_tokens: u32, max_output_tokens: u32) -> f64;
}
