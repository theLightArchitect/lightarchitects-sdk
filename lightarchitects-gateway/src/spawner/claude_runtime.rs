//! Claude CLI provider — [`ClaudeCliProvider`] spawns `claude --bare -p` as a
//! subprocess and wraps the result in the [`LlmAgentProvider`] contract.
//!
//! # Security controls
//!
//! | Gate | Implementation |
//! |------|----------------|
//! | G1 control-plane | [`sanitize_params`] rejects `</system>`, `<system>`, RTL U+202E, zero-width joiners, and null bytes |
//! | G1 content-plane | [`sanitize_params`] escapes `<`/`>` and strips RTL/zero-width chars with `tracing::warn!` |
//! | G10 subprocess hygiene | `kill_on_drop(true)` + `tokio::time::timeout`; stderr piped to `tracing::warn!` only |
//! | G4 traceparent | `TRACEPARENT` env var injected from `parent_span_id` when present |
//!
//! # Cost accounting
//!
//! The provider uses gateway-owned rate tables — never echoes cost from the API:
//! - `claude-sonnet-4-6`: $3.00 / M input tokens, $15.00 / M output tokens

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tracing::{info, warn};

use super::llm_agent::{
    AgentRequest, AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError, SchemaMode,
    TokenUsage,
};

// ── Rate table ─────────────────────────────────────────────────────────────────

/// Input token cost for `claude-sonnet-4-6` in USD per million tokens.
const SONNET_INPUT_USD_PER_M: f64 = 3.0;
/// Output token cost for `claude-sonnet-4-6` in USD per million tokens.
const SONNET_OUTPUT_USD_PER_M: f64 = 15.0;

/// Maximum allowed byte length for control-plane or content-plane strings.
const MAX_PARAM_BYTES: usize = 8_192;

// ── Provider struct ─────────────────────────────────────────────────────────────

/// Spawns `claude --bare -p` as a subprocess.
///
/// Authentication is inherited from the host session (API key / OAuth already
/// configured for the `claude` CLI binary). No additional credentials are required.
#[derive(Debug, Clone)]
pub struct ClaudeCliProvider {
    /// Default model identifier (overridable per-request via [`AgentRequest::model_hint`]).
    pub default_model: String,
    /// Absolute path to the `claude` CLI binary.
    pub claude_binary: PathBuf,
    /// Version tag for the rate table in use (for audit/logging).
    pub rate_table_version: String,
}

impl Default for ClaudeCliProvider {
    fn default() -> Self {
        Self {
            default_model: "claude-sonnet-4-6".to_owned(),
            claude_binary: PathBuf::from("claude"),
            rate_table_version: "2026-05-14".to_owned(),
        }
    }
}

// ── LlmAgentProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl LlmAgentProvider for ClaudeCliProvider {
    fn name(&self) -> &'static str {
        "claude-cli"
    }

    /// Spawn `claude --bare -p` with the sanitized request parameters.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::ParamSanitizationFailed`] if G1 rejects any
    /// parameter, [`ProviderError::SubprocessTimeout`] on wall-clock timeout,
    /// or [`ProviderError::SchemaValidationFailed`] if schema validation fails
    /// after the retry budget is exhausted.
    async fn spawn(&self, req: AgentRequest) -> Result<AgentResponse, ProviderError> {
        let (safe_identity, safe_prompt) =
            sanitize_params(&req.sibling_identity, &req.user_prompt)?;

        let cmd = build_command(
            &self.claude_binary,
            &self.default_model,
            &req,
            &safe_identity,
            &safe_prompt,
        );

        let timeout_secs = u64::from(req.max_turns) * 120 + 30;
        let output = spawn_with_timeout(cmd, Duration::from_secs(timeout_secs)).await?;

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        if !output.stderr.is_empty() {
            warn!(
                provider = "claude-cli",
                "subprocess stderr (suppressed from response): {} bytes",
                output.stderr.len()
            );
        }

        let output_val = validate_and_retry(stdout_str.as_ref(), req.schema.as_ref(), 2)?;

        let input_tokens =
            u32::try_from(req.sibling_identity.len() / 4 + req.user_prompt.len() / 4)
                .unwrap_or(u32::MAX);
        let output_str = serde_json::to_string(&output_val).unwrap_or_default();
        let output_tokens = u32::try_from(output_str.len() / 4).unwrap_or(u32::MAX);
        let cost = self.estimate_cost(input_tokens, output_tokens);

        let resp = AgentResponse {
            output: output_val,
            turns_used: 1,
            cost_usd: cost,
            tokens: TokenUsage {
                input: input_tokens,
                output: output_tokens,
            },
            provider_attrs: HashMap::new(),
            retry_count: 0,
        };
        emit_span(&req, &resp);
        Ok(resp)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            schema_enforcement: SchemaMode::BestEffort,
            native_budget_cap: true,
            native_turn_cap: true,
            auth_inherits_session: true,
        }
    }

    fn estimate_cost(&self, input_tokens: u32, max_output_tokens: u32) -> f64 {
        // Rate table v2026-05-14: claude-sonnet-4-6 pricing.
        (f64::from(input_tokens) / 1_000_000.0 * SONNET_INPUT_USD_PER_M)
            + (f64::from(max_output_tokens) / 1_000_000.0 * SONNET_OUTPUT_USD_PER_M)
    }
}

// ── Private helpers ─────────────────────────────────────────────────────────────

/// Apply G1 two-plane sanitization.
///
/// - `identity` (control-plane): **reject** on dangerous tokens — returns
///   [`ProviderError::ParamSanitizationFailed`] for any match.
/// - `prompt` (content-plane): **escape** `<`/`>` to HTML entities; strip
///   RTL and zero-width chars with a warning; length cap applied to both.
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

/// Reject a control-plane string that contains dangerous tokens.
fn reject_control_plane(s: &str) -> Result<String, ProviderError> {
    // Forbidden patterns: XML system tags, RTL override, zero-width chars, null byte.
    let forbidden: &[(&str, &str)] = &[
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

    for (token, description) in forbidden {
        if s.contains(token) {
            return Err(ProviderError::ParamSanitizationFailed {
                param_name: "sibling_identity".to_owned(),
                reason: format!("contains forbidden token: {description}"),
            });
        }
    }

    Ok(s.to_owned())
}

/// Escape and strip a content-plane string.
///
/// `<` and `>` are HTML-entity-escaped. RTL override and zero-width characters
/// are stripped (not rejected) with a `tracing::warn!`.
fn escape_content_plane(s: &str) -> String {
    // Strip RTL and zero-width control characters.
    let stripped = strip_invisible(s);
    // HTML-escape angle brackets.
    stripped.replace('<', "&lt;").replace('>', "&gt;")
}

/// Strip RTL override and zero-width Unicode control characters from `s`.
fn strip_invisible(s: &str) -> String {
    const INVISIBLE: &[char] = &[
        '\u{202E}', // RTL override
        '\u{200B}', // zero-width space
        '\u{200C}', // zero-width non-joiner
        '\u{200D}', // zero-width joiner
        '\u{200E}', // left-to-right mark
        '\u{200F}', // right-to-left mark
        '\u{FEFF}', // BOM / ZWNBSP
    ];

    let result: String = s.chars().filter(|c| !INVISIBLE.contains(c)).collect();
    if result.len() != s.len() {
        warn!(
            provider = "claude-cli",
            original_len = s.len(),
            stripped_len = result.len(),
            "stripped invisible Unicode characters from content-plane param"
        );
    }
    result
}

/// Return an error if `s` exceeds [`MAX_PARAM_BYTES`].
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

/// Build the `tokio::process::Command` for the Claude CLI subprocess.
fn build_command(
    binary: &PathBuf,
    default_model: &str,
    req: &AgentRequest,
    safe_identity: &str,
    safe_prompt: &str,
) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(binary);

    cmd.arg("--bare")
        .arg("-p")
        .arg(safe_prompt)
        .arg("--append-system-prompt")
        .arg(safe_identity)
        .arg("--max-turns")
        .arg(req.max_turns.to_string())
        .arg("--max-budget-usd")
        .arg(req.max_budget_usd.to_string())
        .arg("--output-format")
        .arg("json");

    let model = req.model_hint.as_deref().unwrap_or(default_model);
    cmd.arg("--model").arg(model);

    if !req.allowed_tools.is_empty() {
        cmd.arg("--tools").arg(req.allowed_tools.join(","));
    }

    // Prevent recursive gateway invocation — supply an empty MCP config.
    cmd.arg("--mcp-config").arg("/dev/null");

    if let Some(span_id) = &req.parent_span_id {
        cmd.env("TRACEPARENT", span_id);
    }

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    cmd
}

/// Spawn the command and wait for it to complete within `timeout_dur`.
///
/// # Errors
///
/// - [`ProviderError::SubprocessTimeout`] if the deadline is exceeded.
/// - [`ProviderError::Internal`] if the process cannot be spawned or waited on.
async fn spawn_with_timeout(
    mut cmd: tokio::process::Command,
    timeout_dur: Duration,
) -> Result<std::process::Output, ProviderError> {
    let child = cmd
        .spawn()
        .map_err(|e| ProviderError::Internal(e.to_string()))?;

    let result = tokio::time::timeout(timeout_dur, child.wait_with_output()).await;

    match result {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(ProviderError::Internal(e.to_string())),
        Err(_elapsed) => Err(ProviderError::SubprocessTimeout {
            used_turns: 0,
            used_budget_usd: 0.0,
        }),
    }
}

/// Parse `raw` as JSON; on failure retry up to `max_retries` times (re-parse only).
///
/// Schema validation (when `schema` is `Some`) checks that the parsed value is
/// an object — full JSON-Schema evaluation is a future extension.
///
/// # Errors
///
/// Returns [`ProviderError::SchemaValidationFailed`] when all attempts fail.
fn validate_and_retry(
    raw: &str,
    schema: Option<&Value>,
    max_retries: u8,
) -> Result<Value, ProviderError> {
    let mut last_err = String::new();

    for attempt in 0..=max_retries {
        match try_parse_and_validate(raw, schema) {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = e;
                if attempt == max_retries {
                    break;
                }
            }
        }
    }

    Err(ProviderError::SchemaValidationFailed {
        retries: u32::from(max_retries),
        last_error: last_err,
    })
}

/// Attempt to parse `raw` as JSON and optionally validate against `schema`.
fn try_parse_and_validate(raw: &str, schema: Option<&Value>) -> Result<Value, String> {
    let value: Value = serde_json::from_str(raw).map_err(|e| format!("JSON parse error: {e}"))?;

    if schema.is_some() && !value.is_object() {
        return Err(format!(
            "schema requires object, got {}",
            value_type_name(&value)
        ));
    }

    Ok(value)
}

/// Return a human-readable type name for a JSON value (for error messages).
fn value_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Emit a `tracing::info!` span with key request/response attributes.
///
/// This is the G4/AYIN hook point — the span fields are picked up by the AYIN
/// tracing subscriber for span enrichment.
fn emit_span(req: &AgentRequest, resp: &AgentResponse) {
    info!(
        provider = "claude-cli",
        model = req.model_hint.as_deref().unwrap_or("claude-sonnet-4-6"),
        turns_used = resp.turns_used,
        input_tokens = resp.tokens.input,
        output_tokens = resp.tokens.output,
        cost_usd = resp.cost_usd,
        retry_count = resp.retry_count,
        parent_span_id = req.parent_span_id.as_deref().unwrap_or("none"),
        "claude-cli agent call completed"
    );
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_rejects_system_close_tag_in_identity() {
        let result = sanitize_params("</system>inject", "hello");
        assert!(result.is_err(), "should reject </system> in identity");
    }

    #[test]
    fn sanitize_rejects_rtl_override_in_identity() {
        let result = sanitize_params("\u{202E}rtl", "ok");
        assert!(result.is_err(), "should reject RTL override in identity");
    }

    #[test]
    fn sanitize_escapes_angle_brackets_in_prompt() {
        let (_, safe) = sanitize_params("safe identity", "</system>inject").unwrap();
        assert!(
            !safe.contains("</system>"),
            "raw tag must not appear in output"
        );
        assert!(
            safe.contains("&lt;/system&gt;"),
            "should be HTML-escaped: {safe}"
        );
    }

    #[test]
    fn sanitize_rejects_oversized_identity() {
        let big = "x".repeat(MAX_PARAM_BYTES + 1);
        let result = sanitize_params(&big, "ok");
        assert!(
            result.is_err(),
            "should reject identity exceeding MAX_PARAM_BYTES"
        );
    }

    #[test]
    fn estimate_cost_zero_tokens() {
        let p = ClaudeCliProvider::default();
        assert!(
            p.estimate_cost(0, 0).abs() < 1e-12,
            "zero tokens should yield zero cost"
        );
    }

    #[test]
    fn estimate_cost_one_million_tokens_each() {
        let p = ClaudeCliProvider::default();
        let cost = p.estimate_cost(1_000_000, 1_000_000);
        let expected = SONNET_INPUT_USD_PER_M + SONNET_OUTPUT_USD_PER_M;
        assert!(
            (cost - expected).abs() < 1e-9,
            "cost={cost}, expected={expected}"
        );
    }
}
