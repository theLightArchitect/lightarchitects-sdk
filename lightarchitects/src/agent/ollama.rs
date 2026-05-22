//! Ollama CLI provider — [`OllamaCliProvider`] spawns `ollama run` as a subprocess.
//!
//! # Security controls
//!
//! | Gate | Implementation |
//! |------|----------------|
//! | G1 content-plane | [`sanitize_for_dispatch`]: control chars rejected before subprocess exec |
//! | G10 subprocess hygiene | `kill_on_drop(true)` + `process_group(0)` + `tokio::time::timeout` |
//! | Registry guard | Model slug validated against `CLOUD_MODEL_REGISTRY` before dispatch |
//! | No shell interpolation | `Command::new("ollama")` with args as separate `Vec` items — execve(2) semantics |
//!
//! Phase 2 provides the provider foundation: direct `ollama run` dispatch.
//! Phase 3 layers the ADK Python substrate for multi-turn tool use and
//! schema-validated output.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::Value;
use tracing::{info, warn};

use super::cloud_models::{CostTier, lookup};
use super::error::OllamaError;
use super::provider::{
    AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError, SanitizedAgentRequest,
    SchemaMode, TokenUsage,
};
use super::translator::sanitize_prompt;

// ── Rate table (approximate Ollama Cloud pricing by CostTier) ──────────────────
// Exact per-model rates will be locked once Ollama publishes billing details.

const LOW_INPUT_USD_PER_M: f64 = 0.10;
const LOW_OUTPUT_USD_PER_M: f64 = 0.30;
const MEDIUM_INPUT_USD_PER_M: f64 = 0.50;
const MEDIUM_OUTPUT_USD_PER_M: f64 = 1.50;
const HIGH_INPUT_USD_PER_M: f64 = 2.00;
const HIGH_OUTPUT_USD_PER_M: f64 = 6.00;
const PREMIUM_INPUT_USD_PER_M: f64 = 5.00;
const PREMIUM_OUTPUT_USD_PER_M: f64 = 15.00;

// ── Provider struct ─────────────────────────────────────────────────────────────

/// Spawns `ollama run <model>` as a subprocess for one-shot completions.
///
/// Requires `ollama` on `PATH` and a valid account configured via
/// `~/.ollama/config` or `OLLAMA_HOST` / `OLLAMA_API_KEY` env vars.
///
/// # Example
///
/// ```rust,no_run
/// # use lightarchitects::agent::OllamaCliProvider;
/// let provider = OllamaCliProvider::new("glm-5.1:cloud").unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct OllamaCliProvider {
    /// Default model slug — must be present in `CLOUD_MODEL_REGISTRY`.
    pub default_model: String,
    /// Rate-table version tag written to AYIN spans for audit purposes.
    pub rate_table_version: &'static str,
}

impl OllamaCliProvider {
    /// Construct a new provider validated against the cloud model registry.
    ///
    /// # Errors
    ///
    /// Returns [`OllamaError::UnknownModel`] if `model_slug` is not in
    /// `CLOUD_MODEL_REGISTRY`.
    pub fn new(model_slug: impl Into<String>) -> Result<Self, OllamaError> {
        let slug = model_slug.into();
        if lookup(&slug).is_none() {
            return Err(OllamaError::UnknownModel(slug));
        }
        Ok(Self {
            default_model: slug,
            rate_table_version: "2026-05-21",
        })
    }
}

// ── LlmAgentProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl LlmAgentProvider for OllamaCliProvider {
    fn name(&self) -> &'static str {
        "ollama-cli"
    }

    /// Dispatch a one-shot prompt via `ollama run <model>`.
    ///
    /// The effective model is `model_hint` when provided, otherwise
    /// `default_model`. Both must be present in `CLOUD_MODEL_REGISTRY`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Internal`] for unknown model, missing CLI, or
    /// subprocess failure. Returns [`ProviderError::SubprocessTimeout`] when the
    /// wall-clock budget is exceeded.
    async fn spawn(&self, req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        let inner = req.request();
        let model = inner.model_hint.as_deref().unwrap_or(&self.default_model);

        let model_meta = lookup(model).ok_or_else(|| {
            ProviderError::Internal(format!(
                "ollama-cli: unknown model '{model}' (not in CLOUD_MODEL_REGISTRY)"
            ))
        })?;

        let prompt = req.safe_prompt();
        sanitize_prompt(prompt).map_err(|_| ProviderError::ParamSanitizationFailed {
            param_name: "user_prompt".to_owned(),
            reason: "prompt contains characters forbidden by translator sanitizer".to_owned(),
        })?;

        let t0 = Instant::now();
        let raw = dispatch_ollama(prompt, model, inner.max_turns).await?;
        let latency_ms = u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX);

        let input_tokens = u32::try_from(prompt.len() / 4).unwrap_or(u32::MAX);
        let output_tokens = u32::try_from(raw.len() / 4).unwrap_or(u32::MAX);
        let cost = cost_for_tier(model_meta.cost_tier, input_tokens, output_tokens);

        let mut attrs = HashMap::new();
        attrs.insert(
            "model.family".to_owned(),
            Value::String(model_meta.family.to_owned()),
        );
        attrs.insert(
            "model.provider_org".to_owned(),
            Value::String(model_meta.provider_org.to_owned()),
        );
        attrs.insert(
            "model.cost_tier".to_owned(),
            Value::String(model_meta.cost_tier.as_str().to_owned()),
        );
        attrs.insert(
            "agent.provider".to_owned(),
            Value::String("ollama-cli".to_owned()),
        );
        attrs.insert("latency_ms".to_owned(), Value::Number(latency_ms.into()));

        info!(
            provider = "ollama-cli",
            model,
            family = model_meta.family,
            cost_tier = model_meta.cost_tier.as_str(),
            input_tokens,
            output_tokens,
            cost_usd = cost,
            latency_ms,
            "ollama agent call completed"
        );

        Ok(AgentResponse {
            output: Value::String(raw),
            turns_used: 1,
            cost_usd: cost,
            tokens: TokenUsage {
                input: input_tokens,
                output: output_tokens,
            },
            provider_attrs: attrs,
            retry_count: 0,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            // Phase 2: plain text output, no JSON schema enforcement.
            // Phase 3 (ADK layer) upgrades this to BestEffort.
            schema_enforcement: SchemaMode::None,
            native_budget_cap: false,
            native_turn_cap: false,
            auth_inherits_session: false,
        }
    }

    fn estimate_cost(&self, input_tokens: u32, max_output_tokens: u32) -> f64 {
        let tier = lookup(&self.default_model).map_or(CostTier::Medium, |m| m.cost_tier);
        cost_for_tier(tier, input_tokens, max_output_tokens)
    }
}

// ── Internal helpers ───────────────────────────────────────────────────────────

/// Invoke `ollama run <model> <prompt>` and return trimmed stdout.
///
/// Uses `kill_on_drop(true)` and a `process_group(0)` so that `killpg` on
/// timeout reaches any grandchild processes the ollama CLI may spawn.
async fn dispatch_ollama(
    prompt: &str,
    model: &str,
    max_turns: u32,
) -> Result<String, ProviderError> {
    let timeout_dur = Duration::from_secs(u64::from(max_turns) * 120 + 30);

    // execve(2) semantics — no shell; args are separate Vec items.
    // model is validated against CLOUD_MODEL_REGISTRY (known slugs) before this call.
    let mut cmd = tokio::process::Command::new("ollama");
    cmd.args(["run", model, prompt])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    #[cfg(unix)]
    cmd.process_group(0);

    let child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            warn!(provider = "ollama-cli", "ollama binary not found on PATH");
            ProviderError::Internal("ollama CLI not found on PATH".to_owned())
        } else {
            warn!(provider = "ollama-cli", err = %e, "subprocess spawn failed");
            ProviderError::Internal(format!("ollama spawn failed: {e}"))
        }
    })?;

    let pgid = child.id();

    let result = tokio::time::timeout(timeout_dur, child.wait_with_output()).await;

    match result {
        Ok(Ok(out)) => {
            if !out.stderr.is_empty() {
                warn!(
                    provider = "ollama-cli",
                    stderr_bytes = out.stderr.len(),
                    "subprocess wrote to stderr"
                );
            }
            if !out.status.success() {
                let code = out.status.code().unwrap_or(-1);
                warn!(provider = "ollama-cli", exit_code = code, "non-zero exit");
                return Err(ProviderError::Internal(format!(
                    "ollama exited with status {code}"
                )));
            }
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
        }
        Ok(Err(e)) => {
            warn!(provider = "ollama-cli", err = %e, "wait_with_output failed");
            Err(ProviderError::Internal("subprocess I/O error".to_owned()))
        }
        Err(_elapsed) => {
            #[cfg(unix)]
            if let Some(pid) = pgid {
                // SAFETY: killpg is async-signal-safe. `pid` is a valid u32 from the OS,
                // bounded by PID_MAX (≤4_194_304 on Linux, 99_999 on macOS) — well within
                // i32::MAX, so the cast cannot wrap. Negative return value means the group
                // already exited; safe to ignore.
                #[allow(unsafe_code, clippy::cast_possible_wrap)]
                unsafe {
                    libc::killpg(pid as libc::pid_t, libc::SIGKILL);
                }
            }
            warn!(
                provider = "ollama-cli",
                timeout_secs = timeout_dur.as_secs(),
                "subprocess timed out; process group killed"
            );
            Err(ProviderError::SubprocessTimeout {
                used_turns: 0,
                used_budget_usd: 0.0,
            })
        }
    }
}

/// Compute estimated cost in USD from token counts and a cost tier.
fn cost_for_tier(tier: CostTier, input_tokens: u32, output_tokens: u32) -> f64 {
    let (in_rate, out_rate) = match tier {
        CostTier::Low => (LOW_INPUT_USD_PER_M, LOW_OUTPUT_USD_PER_M),
        CostTier::Medium => (MEDIUM_INPUT_USD_PER_M, MEDIUM_OUTPUT_USD_PER_M),
        CostTier::High => (HIGH_INPUT_USD_PER_M, HIGH_OUTPUT_USD_PER_M),
        CostTier::Premium => (PREMIUM_INPUT_USD_PER_M, PREMIUM_OUTPUT_USD_PER_M),
    };
    (f64::from(input_tokens) / 1_000_000.0 * in_rate)
        + (f64::from(output_tokens) / 1_000_000.0 * out_rate)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::agent::cloud_models::CLOUD_MODEL_REGISTRY;

    #[test]
    fn provider_new_valid_slug_succeeds() {
        let p = OllamaCliProvider::new("glm-5.1:cloud").unwrap();
        assert_eq!(p.default_model, "glm-5.1:cloud");
        assert_eq!(p.name(), "ollama-cli");
    }

    #[test]
    fn provider_new_unknown_slug_fails() {
        let err = OllamaCliProvider::new("not-a-model:cloud").unwrap_err();
        assert!(
            matches!(err, OllamaError::UnknownModel(ref s) if s == "not-a-model:cloud"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn capabilities_reports_no_native_enforcement() {
        let p = OllamaCliProvider::new("glm-5.1:cloud").unwrap();
        let caps = p.capabilities();
        assert!(
            !caps.native_budget_cap,
            "ollama-cli has no native budget cap in phase 2"
        );
        assert!(
            !caps.native_turn_cap,
            "ollama-cli has no native turn cap in phase 2"
        );
        assert!(!caps.auth_inherits_session);
        assert_eq!(caps.schema_enforcement, SchemaMode::None);
    }

    #[test]
    fn estimate_cost_non_negative_for_all_registry_models() {
        for m in CLOUD_MODEL_REGISTRY {
            let p = OllamaCliProvider::new(m.slug).unwrap();
            let cost = p.estimate_cost(1_000, 500);
            assert!(
                cost >= 0.0,
                "estimate_cost returned negative for '{}'",
                m.slug
            );
        }
    }

    #[test]
    fn low_tier_cheaper_than_premium_tier() {
        let low = cost_for_tier(CostTier::Low, 100_000, 100_000);
        let premium = cost_for_tier(CostTier::Premium, 100_000, 100_000);
        assert!(
            low < premium,
            "Low tier ({low}) must cost less than Premium ({premium})"
        );
    }
}
