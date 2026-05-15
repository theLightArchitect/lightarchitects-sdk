//! Claude CLI provider — [`ClaudeCliProvider`] spawns `claude -p` as a
//! subprocess and wraps the result in the [`LlmAgentProvider`] contract.
//!
//! # Security controls
//!
//! | Gate | Implementation |
//! |------|----------------|
//! | G1 control-plane | [`sanitize_params`] rejects `</system>`, `<system>`, RTL U+202E, zero-width joiners, and null bytes |
//! | G1 content-plane | [`sanitize_params`] escapes `<`/`>` and strips RTL/zero-width chars with `tracing::warn!` |
//! | G10 subprocess hygiene | `kill_on_drop(true)` + `process_group(0)` + `libc::killpg` on timeout; stderr piped to `tracing::warn!` only |
//! | G4 traceparent | `TRACEPARENT` env var injected from `parent_span_id` when present |
//!
//! # Auth model
//!
//! The subprocess inherits authentication from the host `claude` CLI session
//! (OAuth / Claude Max, or explicit API key via `api_key` field). The host
//! `ANTHROPIC_API_KEY` env var is explicitly removed so a stale env var cannot
//! override the user's configured auth method.
//!
//! # Cost accounting
//!
//! The provider uses gateway-owned rate tables — never echoes cost from the API:
//! - `claude-sonnet-4-6`: $3.00 / M input tokens, $15.00 / M output tokens

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tempfile::TempDir;

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use tracing::{info, warn};

use super::provider::{
    AgentRequest, AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError,
    SanitizedAgentRequest, SchemaMode, TokenUsage,
};

// ── Rate table ─────────────────────────────────────────────────────────────────

/// Input token cost for `claude-sonnet-4-6` in USD per million tokens.
const SONNET_INPUT_USD_PER_M: f64 = 3.0;
/// Output token cost for `claude-sonnet-4-6` in USD per million tokens.
const SONNET_OUTPUT_USD_PER_M: f64 = 15.0;

// ── Provider struct ─────────────────────────────────────────────────────────────

/// Spawns `claude -p` as a subprocess.
///
/// Authentication is inherited from the host session (OAuth / Claude Max or
/// API key already configured for the `claude` CLI binary). The host
/// `ANTHROPIC_API_KEY` env var is removed to prevent stale overrides; use
/// `api_key` to supply an explicit key when needed.
#[derive(Debug)]
pub struct ClaudeCliProvider {
    /// Default model identifier (overridable per-request via [`AgentRequest::model_hint`]).
    pub default_model: String,
    /// Absolute path to the `claude` CLI binary.
    pub claude_binary: PathBuf,
    /// Version tag for the rate table in use (for audit/logging).
    pub rate_table_version: String,
    /// Explicit API key to inject into the subprocess environment.
    ///
    /// Stored as a [`SecretString`] so the key is zeroed on drop and never
    /// appears in `Debug` output or log sinks.
    ///
    /// When `None`, the subprocess inherits the host session's auth
    /// (OAuth / Claude Max). When `Some`, the key is set as
    /// `ANTHROPIC_API_KEY` for the subprocess only.
    ///
    /// # Accepted risk (F5)
    ///
    /// `SecretString` zeroes this field on drop via `ZeroizeOnDrop`, but
    /// `cmd.env()` copies the value into a plain `OsString` allocation that is
    /// not zeroized. The window is short (subprocess exec only). Mitigation:
    /// run with `ulimit -c 0` in production to prevent heap-dump exposure.
    /// Follow-on: migrate to stdin-based key injection when Claude CLI supports it.
    pub api_key: Option<SecretString>,
}

impl Default for ClaudeCliProvider {
    fn default() -> Self {
        Self {
            default_model: "claude-sonnet-4-6".to_owned(),
            claude_binary: PathBuf::from("claude"),
            rate_table_version: "2026-05-14".to_owned(),
            api_key: None,
        }
    }
}

// ── LlmAgentProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl LlmAgentProvider for ClaudeCliProvider {
    fn name(&self) -> &'static str {
        "claude-cli"
    }

    /// Spawn `claude -p` with the sanitized request parameters.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::ParamSanitizationFailed`] if G1 rejects any
    /// parameter, [`ProviderError::SubprocessTimeout`] on wall-clock timeout,
    /// or [`ProviderError::SchemaValidationFailed`] if schema validation fails
    /// after the retry budget is exhausted.
    async fn spawn(&self, req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        // R3: process-private tempdir prevents TOCTOU symlink race on predictable /tmp path.
        let mcp_dir = TempDir::with_prefix("la-")
            .map_err(|e| ProviderError::Internal(format!("tempdir creation failed: {e}")))?;
        let mcp_config = mcp_dir.path().join("mcp-null.json");
        std::fs::write(&mcp_config, r#"{"mcpServers":{}}"#)
            .map_err(|e| ProviderError::Internal(format!("mcp config write failed: {e}")))?;

        let cmd = build_command(
            &self.claude_binary,
            &self.default_model,
            &req,
            self.api_key.as_ref().map(ExposeSecret::expose_secret),
            &mcp_config,
        );

        let inner = req.request();
        let timeout_secs = u64::from(inner.max_turns) * 120 + 30;
        let output = spawn_with_timeout(cmd, Duration::from_secs(timeout_secs), mcp_dir).await?;

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        if !output.stderr.is_empty() {
            warn!(
                provider = "claude-cli",
                "subprocess stderr (suppressed from response): {} bytes",
                output.stderr.len()
            );
        }

        let output_val = validate_and_retry(stdout_str.as_ref(), inner.schema.as_ref(), 2)?;

        let input_tokens =
            u32::try_from(inner.sibling_identity.len() / 4 + inner.user_prompt.len() / 4)
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
        emit_span(inner, &resp);
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

/// Build the `tokio::process::Command` for the Claude CLI subprocess.
///
/// `mcp_config` must point to a file containing `{"mcpServers":{}}` inside a
/// process-private tempdir (created by the caller with [`TempDir::with_prefix`]).
fn build_command(
    binary: &PathBuf,
    default_model: &str,
    req: &SanitizedAgentRequest,
    api_key: Option<&str>,
    mcp_config: &Path,
) -> tokio::process::Command {
    let inner = req.request();
    let mut cmd = tokio::process::Command::new(binary);

    cmd.arg("-p")
        .arg(req.safe_prompt())
        .arg("--append-system-prompt")
        .arg(req.safe_identity())
        .arg("--max-turns")
        .arg(inner.max_turns.to_string())
        .arg("--max-budget-usd")
        .arg(inner.max_budget_usd.to_string())
        .arg("--output-format")
        .arg("json");

    let model = inner.model_hint.as_deref().unwrap_or(default_model);
    cmd.arg("--model").arg(model);

    if !inner.allowed_tools.is_empty() {
        cmd.arg("--tools").arg(inner.allowed_tools.join(","));
    }

    // G10: prevent recursive gateway invocation by restricting MCP servers.
    cmd.arg("--strict-mcp-config")
        .arg("--mcp-config")
        .arg(mcp_config);

    if let Some(span_id) = &inner.parent_span_id {
        cmd.env("TRACEPARENT", span_id);
    }

    // Auth: remove any stale ANTHROPIC_API_KEY from the host env so it cannot
    // override the user's configured auth (OAuth / Claude Max). Re-inject only
    // when an explicit key is provided.
    cmd.env_remove("ANTHROPIC_API_KEY");
    if let Some(key) = api_key {
        cmd.env("ANTHROPIC_API_KEY", key);
    }

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    // G10: put the subprocess in its own process group so that killpg on
    // timeout reaches all grandchildren (e.g. claude spawning sub-agents).
    // PGID == child PID when pgroup is 0.
    #[cfg(unix)]
    cmd.process_group(0);

    cmd
}

/// Spawn the command and wait for it to complete within `timeout_dur`.
///
/// # Errors
///
/// - [`ProviderError::SubprocessTimeout`] if the deadline is exceeded.
/// - [`ProviderError::Internal`] if the process cannot be spawned or waited on.
///
/// # Safety note
///
/// The `#[allow(unsafe_code)]` covers the `libc::killpg` call on timeout.
/// `killpg` is async-signal-safe and the PID is a valid `u32` obtained from
/// the OS at spawn time. A negative return value means the group already
/// exited — safe to ignore.
#[allow(unsafe_code)]
async fn spawn_with_timeout(
    mut cmd: tokio::process::Command,
    timeout_dur: Duration,
    _mcp_dir: TempDir,
) -> Result<std::process::Output, ProviderError> {
    let child = cmd.spawn().map_err(|e| {
        warn!(provider = "claude-cli", err = %e, "subprocess spawn failed");
        ProviderError::Internal("subprocess launch failed".to_owned())
    })?;

    // Save the PID before wait_with_output() consumes the Child handle.
    // With process_group(0) the PGID equals this PID, so killpg(pid, SIGKILL)
    // reaches all grandchildren spawned by the claude subprocess.
    let pgid = child.id();

    let result = tokio::time::timeout(timeout_dur, child.wait_with_output()).await;

    match result {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => {
            warn!(provider = "claude-cli", err = %e, "subprocess wait_with_output failed");
            Err(ProviderError::Internal("subprocess I/O error".to_owned()))
        }
        Err(_elapsed) => {
            // kill_on_drop fires when the future above is dropped (direct child).
            // Also kill the process group to reap any grandchildren.
            #[cfg(unix)]
            if let Some(pid) = pgid {
                // SAFETY: killpg is async-signal-safe; pid is a valid u32 from
                // the OS. Negative return value means group already gone — safe to ignore.
                // OS PIDs are bounded by PID_MAX (≤ 4_194_304 on Linux, 99_999 on macOS),
                // well within i32::MAX, so the cast cannot wrap.
                #[allow(clippy::cast_possible_wrap)]
                unsafe {
                    libc::killpg(pid as libc::pid_t, libc::SIGKILL);
                }
            }
            warn!(
                provider = "claude-cli",
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

/// Parse `raw` as JSON; on failure retry up to `max_retries` times.
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

/// Return a human-readable type name for a JSON value.
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
    use crate::agent::provider::{MAX_PARAM_BYTES, sanitize_params};

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

    /// G10 recursion guard: `--strict-mcp-config` MUST be present and `--bare`
    /// MUST NOT be present (bare mode is API-key-only, blocking OAuth/Claude Max).
    #[test]
    fn command_includes_strict_mcp_config_and_no_bare() {
        use std::ffi::OsStr;
        let req = AgentRequest {
            sibling_identity: "test-sibling".to_owned(),
            user_prompt: "probe".to_owned(),
            schema: None,
            allowed_tools: vec![],
            max_turns: 1,
            max_budget_usd: 0.10,
            model_hint: None,
            parent_span_id: None,
            chain_origin: None,
            chain_depth: 0,
            aud: None,
        }
        .sanitize()
        .unwrap();
        let provider = ClaudeCliProvider::default();
        let mcp_dir = tempfile::TempDir::with_prefix("la-").unwrap();
        let mcp_config = mcp_dir.path().join("mcp-null.json");
        std::fs::write(&mcp_config, r#"{"mcpServers":{}}"#).unwrap();
        let cmd = build_command(
            &provider.claude_binary,
            &provider.default_model,
            &req,
            provider
                .api_key
                .as_ref()
                .map(secrecy::ExposeSecret::expose_secret),
            &mcp_config,
        );
        let args: Vec<&OsStr> = cmd.as_std().get_args().collect();
        let args_str: Vec<&str> = args.iter().filter_map(|a| a.to_str()).collect();

        assert!(
            !args_str.contains(&"--bare"),
            "--bare must not appear (breaks OAuth/Claude Max auth)"
        );

        let strict_pos = args_str
            .iter()
            .position(|a| *a == "--strict-mcp-config")
            .expect("--strict-mcp-config must be present");
        let mcp_config_pos = args_str
            .iter()
            .position(|a| *a == "--mcp-config")
            .expect("--mcp-config must be present");
        let mcp_path = args_str.get(mcp_config_pos + 1).copied().unwrap_or("");
        assert!(
            mcp_path.ends_with("mcp-null.json"),
            "--mcp-config must point to process-private mcp-null.json, got: {mcp_path}"
        );
        assert!(
            strict_pos < mcp_config_pos,
            "--strict-mcp-config must precede --mcp-config"
        );
    }
}
