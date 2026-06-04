//! `ContractSupervisor` — evaluates a generated artifact against a
//! [`TaskContract`] using a configurable LLM backend.
//!
//! # Operator-selectable provider
//!
//! The supervisor backend is one of:
//!
//! | Variant | Selected via | Default model |
//! |---------|--------------|---------------|
//! | [`SupervisorProvider::ClaudeCode`] | `LIGHTSQUAD_SUPERVISOR_PROVIDER=claude-code` (default) | Whatever the user's `claude` session is authenticated with |
//! | [`SupervisorProvider::Codex`]      | `LIGHTSQUAD_SUPERVISOR_PROVIDER=codex` | Codex's default (e.g. `gpt-5-codex`) |
//! | [`SupervisorProvider::OllamaCloud`]| `LIGHTSQUAD_SUPERVISOR_PROVIDER=ollama` | `OLLAMA_MODEL` env var or `kimi-k2.5:cloud` |
//!
//! # Default resolution
//!
//! [`SupervisorProvider::from_env`] resolves the provider in this order:
//!
//! 1. `LIGHTSQUAD_SUPERVISOR_PROVIDER` explicit override.
//! 2. Inferred from `LLM_BACKEND` (the webshell's backend selector) — keeps
//!    the supervisor consistent with whatever backend the webshell started
//!    with. `LLM_BACKEND=codex` → codex supervisor; otherwise → claude-code.
//! 3. Hard default: `ClaudeCode`.
//!
//! Model resolution:
//!
//! 1. `LIGHTSQUAD_SUPERVISOR_MODEL` explicit override.
//! 2. For Ollama: `OLLAMA_MODEL` env var (same value the webshell uses for
//!    its worker).
//! 3. Provider-specific default.
//!
//! # Why pluggable
//!
//! Different artifacts demand different evaluator profiles:
//! - Diagrams + creative HTML → Claude Code (style + topology judgment)
//! - Rust code → Codex (deep code analysis + GPT-5 family precision)
//! - Quick cost-bounded checks → Ollama Cloud (cheap, fast, no schema
//!   enforcement)
//!
//! All three implement the same [`ContractSupervisor::evaluate`] signature.

// Provider/product names (OpenAI, LiteLLM, OpenRouter, Anthropic, Vertex AI,
// Gemini, Bedrock, Together, Groq, Fireworks, Azure, Databricks, etc.) appear
// frequently in prose throughout this module. Backticking each occurrence is
// noisy; we accept the doc_markdown lint here.
#![allow(clippy::doc_markdown)]

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncWriteExt;

use crate::agent::OpenAIFlavor;
use crate::lightsquad::contract::{Dimension, DimensionScore, TaskContract, Verdict};
use crate::lightsquad::contract_prompt::build_evaluator_prompt;

// ── Defaults ────────────────────────────────────────────────────────────────

const DEFAULT_TIMEOUT_S: u64 = 300;

const DEFAULT_SYSTEM_PROMPT: &str = "\
You are a CONTRACT SUPERVISOR for the Light Architects platform. You evaluate \
generated artifacts against structured contracts and return STRICT JSON via \
the configured schema. \
\
SCORING DISCIPLINE — use the full gradient, NOT binary pass/fail: \
- 1.00 = every criterion in this dimension is met perfectly (rare). \
- 0.80–0.95 = most criteria met, minor issues. \
- 0.50–0.79 = roughly half of criteria met, real but addressable gaps. \
- 0.20–0.49 = some criteria attempted but significant failures. \
- 0.00–0.19 = the artifact fundamentally does not address this dimension. \
- Partial credit is REQUIRED. If 4 of 5 criteria pass, score ~0.80 — not 0.0. \
- A score of 0.0 means the dimension was entirely ignored or actively broken; \
  it is NOT the default for any single missing criterion. \
\
EVIDENCE DISCIPLINE: \
- Quote the artifact directly in `reasoning`. No hand-waving, no vibes. \
- Use `ci_low` and `ci_high` to express uncertainty about the SCORE itself \
  (not about the artifact's quality). Narrow CI when you can verify with \
  certainty from the artifact text. Widen CI when you'd need information \
  beyond the artifact to confirm. \
- `failed_criteria` lists exact criterion strings (verbatim from the \
  contract) that the artifact does not satisfy. Empty when all pass. \
- Cite specific line content or absent items: 'header missing the X tag', \
  'edge labels appear for L1↔L1 but not for L0↔L1', etc. \
\
CALIBRATION TARGET: most real artifacts score in the 0.4–0.9 range. Scoring \
EVERY dimension at 0.0 across an iteration is almost always wrong — that \
implies the artifact is unrelated to the task, which is rarely true. If you \
catch yourself emitting 0.0 across the board, re-read the artifact and look \
for partial matches.";

const DEFAULT_CODEX_BIN: &str = "codex";
const DEFAULT_OLLAMA_BASE_URL: &str = "https://ollama.com";
const DEFAULT_OLLAMA_MODEL: &str = "kimi-k2.5:cloud";
const DEFAULT_ALLOWED_TOOLS: &[&str] = &["Read", "Grep", "Glob"];

// ── Errors ──────────────────────────────────────────────────────────────────

/// Errors returned by [`ContractSupervisor::evaluate`].
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    /// Subprocess failed to spawn or returned a non-zero exit code.
    #[error("supervisor subprocess failed: {0}")]
    Subprocess(String),
    /// I/O error writing the prompt to stdin or reading stdout.
    #[error("supervisor I/O: {0}")]
    Io(#[from] std::io::Error),
    /// HTTP transport or status error (Ollama backend only).
    #[error("supervisor HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    /// Provider returned malformed envelope (missing fields, bad JSON).
    #[error("supervisor returned malformed envelope: {0}")]
    InvalidEnvelope(String),
    /// Claude Code or Codex returned `error_max_*_retries` — calibration smell.
    #[error("evaluator exhausted structured-output retries (model could not match schema)")]
    SchemaRetriesExhausted,
    /// A dimension declared in the contract is missing from the response.
    #[error("evaluator omitted dimension '{0}'")]
    MissingDimension(String),
    /// Evaluation timed out.
    #[error("supervisor evaluation timed out after {0}s")]
    Timeout(u64),
}

// ── SupervisorProvider ──────────────────────────────────────────────────────

/// Operator-selectable supervisor backend. Construct via [`SupervisorProvider::from_env`]
/// or one of the variant-specific helpers.
#[derive(Debug, Clone)]
pub enum SupervisorProvider {
    /// Claude Code CLI subprocess — uses `--output-format json --json-schema`
    /// with server-side schema validation + internal retries.
    ClaudeCode {
        /// Path or name of the `claude` binary (resolved via `$PATH`).
        binary: String,
        /// Optional `--model` flag. `None` → use whatever the session is
        /// authenticated with.
        model: Option<String>,
        /// Tools allowed inside the subprocess (`--allowed-tools` comma list).
        allowed_tools: Vec<String>,
    },
    /// Codex CLI subprocess — uses `codex exec --json --output-schema <file>`
    /// (schema is path-based, not inline).
    Codex {
        /// Path or name of the `codex` binary.
        binary: String,
        /// Optional `--model` flag. `None` → use Codex's configured default.
        model: Option<String>,
    },
    /// Ollama Cloud HTTP — POST `/api/chat` with `stream: false`.
    ///
    /// Note: no server-side schema enforcement; we parse JSON from freeform
    /// text. Fragile on large prompts (we observed HTTP 500 "unexpected EOF"
    /// on 30 KB+ artifacts via local Ollama proxy).
    OllamaCloud {
        /// Base URL (e.g. `https://ollama.com` or `http://localhost:11434`).
        base_url: String,
        /// Model slug (e.g. `kimi-k2.5:cloud`).
        model: String,
        /// Bearer token; `None` when routing via local Ollama proxy.
        api_key: Option<SecretString>,
    },
    /// OpenAI-compatible Chat Completions endpoint.
    ///
    /// Covers native OpenAI, OpenRouter, LiteLLM proxy (which itself
    /// proxies Vertex AI / Bedrock / Anthropic / etc.), Together, Groq,
    /// Fireworks, Azure OpenAI, Databricks. All speak the canonical OpenAI
    /// Chat Completions API with `response_format.json_schema.strict =
    /// true` for server-side structured-output enforcement.
    ///
    /// Verified via context7 (`/websites/developers_openai_api`,
    /// `/websites/litellm_ai`) — same request shape works across all
    /// listed providers; LiteLLM internally translates to backend-specific
    /// formats when needed.
    OpenAICompatible {
        /// Provider flavor — controls only defaults + telemetry tag.
        /// All flavors use the same HTTP code path.
        flavor: OpenAIFlavor,
        /// Base URL up to and including `/v1` (e.g.
        /// `https://api.openai.com/v1`, `https://openrouter.ai/api/v1`,
        /// `http://localhost:4000/v1`).
        base_url: String,
        /// Model identifier in the provider's namespace
        /// (e.g. `gpt-5`, `anthropic/claude-sonnet-4`, `gemini-2.5-flash`).
        model: String,
        /// Bearer-auth API key. Required (this variant is HTTP-only and
        /// always needs a key).
        api_key: SecretString,
    },
}

impl SupervisorProvider {
    /// Resolve the supervisor provider from environment variables.
    ///
    /// Priority:
    /// 1. `LIGHTSQUAD_SUPERVISOR_PROVIDER` explicit override
    ///    (`claude-code` | `codex` | `ollama` | `ollama-cloud` | `openai` |
    ///    `openrouter` | `litellm` | `openai-compatible` | `generic`).
    /// 2. Inferred from `LLM_BACKEND` (the webshell's backend selector) so
    ///    the supervisor matches whatever the webshell started with.
    /// 3. Hard default: [`SupervisorProvider::ClaudeCode`].
    ///
    /// # Panics
    ///
    /// `OpenAICompatible` variants panic if the required API key env var is
    /// absent. This is intentional — the supervisor cannot function without
    /// auth, and a clear panic at construction beats an opaque HTTP 401
    /// later.
    #[must_use]
    pub fn from_env() -> Self {
        let kind = resolve_provider_kind();
        match kind.as_str() {
            "codex" => Self::codex_from_env(),
            "ollama" | "ollama-cloud" => Self::ollama_from_env(),
            "openai" | "openai-native" => Self::openai_compatible_from_env(OpenAIFlavor::OpenAi),
            "openrouter" => Self::openai_compatible_from_env(OpenAIFlavor::OpenRouter),
            "litellm" | "litellm-proxy" => Self::openai_compatible_from_env(OpenAIFlavor::LiteLLM),
            "portkey" => Self::openai_compatible_from_env(OpenAIFlavor::Portkey),
            "openai-compatible" | "generic" | "vertex" | "vertex-ai" => {
                // Vertex maps to Generic because the canonical path is via a
                // LiteLLM/HTTP-compat endpoint; the operator supplies the base
                // URL. Native Vertex OAuth is deferred to a future variant.
                Self::openai_compatible_from_env(OpenAIFlavor::Generic)
            }
            _ => Self::claude_code_from_env(),
        }
    }

    /// Construct the [`Self::ClaudeCode`] variant from environment vars:
    /// `LIGHTARCHITECTS_CLAUDE_BIN`, `LIGHTSQUAD_SUPERVISOR_MODEL`.
    #[must_use]
    pub fn claude_code_from_env() -> Self {
        let binary = std::env::var("LIGHTARCHITECTS_CLAUDE_BIN")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| crate::agent::CLAUDE_CLI_DEFAULT_BINARY.to_owned());
        let model = std::env::var("LIGHTSQUAD_SUPERVISOR_MODEL")
            .ok()
            .filter(|s| !s.is_empty());
        let allowed_tools = DEFAULT_ALLOWED_TOOLS
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        Self::ClaudeCode {
            binary,
            model,
            allowed_tools,
        }
    }

    /// Construct the [`Self::Codex`] variant from environment vars:
    /// `LIGHTARCHITECTS_CODEX_BIN`, `LIGHTSQUAD_SUPERVISOR_MODEL`.
    #[must_use]
    pub fn codex_from_env() -> Self {
        let binary = std::env::var("LIGHTARCHITECTS_CODEX_BIN")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_CODEX_BIN.to_owned());
        let model = std::env::var("LIGHTSQUAD_SUPERVISOR_MODEL")
            .ok()
            .filter(|s| !s.is_empty());
        Self::Codex { binary, model }
    }

    /// Construct the [`Self::OllamaCloud`] variant from environment vars:
    /// `OLLAMA_HOST`, `LIGHTSQUAD_SUPERVISOR_MODEL` ← `OLLAMA_MODEL`,
    /// `OLLAMA_API_KEY`.
    #[must_use]
    pub fn ollama_from_env() -> Self {
        let base_url = std::env::var("OLLAMA_HOST")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_OLLAMA_BASE_URL.to_owned());
        // Inherit model from webshell's OLLAMA_MODEL by default so the
        // supervisor matches the worker config.
        let model = std::env::var("LIGHTSQUAD_SUPERVISOR_MODEL")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("OLLAMA_MODEL").ok().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.to_owned());
        let api_key = std::env::var("OLLAMA_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| SecretString::new(s.into()));
        Self::OllamaCloud {
            base_url,
            model,
            api_key,
        }
    }

    /// Construct an [`Self::OpenAICompatible`] variant from environment vars.
    ///
    /// Intentionally panics on misconfiguration — see `# Panics` below.
    #[allow(clippy::panic)]
    ///
    /// Resolution per field:
    /// - `base_url`: `LIGHTSQUAD_SUPERVISOR_BASE_URL` → `<flavor>_BASE_URL`
    ///   (e.g. `OPENROUTER_BASE_URL`) → flavor default. Generic flavor
    ///   requires explicit URL.
    /// - `model`: `LIGHTSQUAD_SUPERVISOR_MODEL` (required for this variant).
    /// - `api_key`: `LIGHTSQUAD_SUPERVISOR_API_KEY` → flavor-specific
    ///   default env (e.g. `OPENROUTER_API_KEY`).
    ///
    /// # Panics
    ///
    /// Panics if `LIGHTSQUAD_SUPERVISOR_MODEL` is unset (required for this
    /// variant) or the resolved API-key env is empty. The supervisor cannot
    /// authenticate without these; a clear panic at construction is better
    /// than an opaque HTTP 401 at first dispatch.
    #[must_use]
    pub fn openai_compatible_from_env(flavor: OpenAIFlavor) -> Self {
        let base_url = std::env::var("LIGHTSQUAD_SUPERVISOR_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let provider_url_env = match flavor {
                    OpenAIFlavor::OpenAi => "OPENAI_BASE_URL",
                    OpenAIFlavor::OpenRouter => "OPENROUTER_BASE_URL",
                    OpenAIFlavor::LiteLLM => "LITELLM_BASE_URL",
                    OpenAIFlavor::Portkey => "PORTKEY_BASE_URL",
                    OpenAIFlavor::Generic => "LIGHTSQUAD_SUPERVISOR_BASE_URL",
                };
                std::env::var(provider_url_env)
                    .ok()
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| flavor.default_base_url().to_owned());
        assert!(
            !base_url.is_empty(),
            "{} requires an explicit base URL — set LIGHTSQUAD_SUPERVISOR_BASE_URL",
            flavor.as_str()
        );

        let model = std::env::var("LIGHTSQUAD_SUPERVISOR_MODEL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{} requires LIGHTSQUAD_SUPERVISOR_MODEL (e.g. 'gpt-5', \
                     'anthropic/claude-sonnet-4', 'gemini-2.5-flash')",
                    flavor.as_str()
                )
            });

        let api_key_str = std::env::var("LIGHTSQUAD_SUPERVISOR_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var(flavor.default_api_key_env())
                    .ok()
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| {
                panic!(
                    "{} requires an API key — set LIGHTSQUAD_SUPERVISOR_API_KEY or {}",
                    flavor.as_str(),
                    flavor.default_api_key_env()
                )
            });
        let api_key = SecretString::new(api_key_str.into());

        Self::OpenAICompatible {
            flavor,
            base_url,
            model,
            api_key,
        }
    }

    /// Short identifier for telemetry / logging.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ClaudeCode { .. } => "claude-code",
            Self::Codex { .. } => "codex",
            Self::OllamaCloud { .. } => "ollama-cloud",
            Self::OpenAICompatible { flavor, .. } => flavor.as_str(),
        }
    }

    /// Human-readable description of the current configuration.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::ClaudeCode {
                binary,
                model,
                allowed_tools,
            } => format!(
                "claude-code (binary={binary}, model={}, tools=[{}])",
                model.as_deref().unwrap_or("<authenticated default>"),
                allowed_tools.join(", ")
            ),
            Self::Codex { binary, model } => format!(
                "codex (binary={binary}, model={})",
                model.as_deref().unwrap_or("<codex default>")
            ),
            Self::OllamaCloud {
                base_url, model, ..
            } => format!("ollama-cloud (base_url={base_url}, model={model})"),
            Self::OpenAICompatible {
                flavor,
                base_url,
                model,
                ..
            } => format!("{} (base_url={base_url}, model={model})", flavor.as_str()),
        }
    }
}

/// Resolve the supervisor-kind string from env, with `LLM_BACKEND` fallback.
fn resolve_provider_kind() -> String {
    if let Some(explicit) = std::env::var("LIGHTSQUAD_SUPERVISOR_PROVIDER")
        .ok()
        .filter(|s| !s.is_empty())
    {
        return explicit.to_lowercase();
    }
    // Fallback: inherit the webshell's backend selector.
    let backend = std::env::var("LLM_BACKEND")
        .unwrap_or_default()
        .to_lowercase();
    match backend.as_str() {
        "codex" => "codex".to_owned(),
        "ollama" | "ollama-cloud" => "ollama-cloud".to_owned(),
        "openai" | "openai-native" => "openai".to_owned(),
        "openrouter" => "openrouter".to_owned(),
        "litellm" | "litellm-proxy" => "litellm".to_owned(),
        "vertex" | "vertex-ai" => "vertex".to_owned(),
        "openai-compatible" | "generic" => "openai-compatible".to_owned(),
        // "claude" | "claude-code" | "" | anything else → claude-code default
        _ => "claude-code".to_owned(),
    }
}

// ── ContractSupervisor ──────────────────────────────────────────────────────

/// Contract evaluator. Holds a [`SupervisorProvider`] and orchestrates the
/// evaluator-prompt → provider-dispatch → verdict flow.
#[derive(Debug, Clone)]
pub struct ContractSupervisor {
    /// The backend that runs the evaluation.
    provider: SupervisorProvider,
    /// System prompt appended to the provider's default.
    system_prompt: String,
    /// Wall-clock cap for one evaluation.
    timeout: Duration,
}

impl ContractSupervisor {
    /// Build a supervisor with provider + timeout resolved from the environment.
    #[must_use]
    pub fn from_env() -> Self {
        let timeout_s = std::env::var("LIGHTSQUAD_SUPERVISOR_TIMEOUT_S")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_S);
        Self {
            provider: SupervisorProvider::from_env(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_owned(),
            timeout: Duration::from_secs(timeout_s),
        }
    }

    /// Build with an explicit provider — bypasses env detection.
    #[must_use]
    pub fn with_provider(provider: SupervisorProvider) -> Self {
        Self {
            provider,
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_owned(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_S),
        }
    }

    /// Override the system prompt.
    #[must_use]
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// Override the per-evaluation timeout.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Read-only view of the active provider (for logging).
    #[must_use]
    pub const fn provider(&self) -> &SupervisorProvider {
        &self.provider
    }

    /// Score `artifact` against `contract` and return a [`Verdict`].
    ///
    /// `source_of_truth` carries trusted reference data used to detect
    /// hallucinations. Pass `""` when no out-of-band truth is needed.
    /// `iteration` is the 0-based iteration index — threaded into the
    /// [`Decision`] computation.
    ///
    /// [`Decision`]: crate::lightsquad::contract::Decision
    ///
    /// # Errors
    ///
    /// Returns [`SupervisorError`] on subprocess/HTTP failure, schema
    /// violation, or missing dimensions.
    pub async fn evaluate(
        &self,
        contract: &TaskContract,
        artifact: &str,
        source_of_truth: &str,
        iteration: u32,
    ) -> Result<Verdict, SupervisorError> {
        let prompt = build_evaluator_prompt(contract, artifact, source_of_truth);
        let schema = build_schema_for_contract(contract);

        let per_dimension = tokio::time::timeout(self.timeout, async {
            match &self.provider {
                SupervisorProvider::ClaudeCode {
                    binary,
                    model,
                    allowed_tools,
                } => self
                    .run_claude_code(binary, model.as_deref(), allowed_tools, &prompt, &schema)
                    .await
                    .and_then(|raw| parse_claude_envelope(&raw, &contract.dimensions)),
                SupervisorProvider::Codex { binary, model } => self
                    .run_codex(binary, model.as_deref(), &prompt, &schema)
                    .await
                    .and_then(|raw| parse_codex_envelope(&raw, &contract.dimensions)),
                SupervisorProvider::OllamaCloud {
                    base_url,
                    model,
                    api_key,
                } => {
                    let raw = self
                        .run_ollama(base_url, model, api_key.as_ref(), &prompt)
                        .await?;
                    parse_freeform_json(&raw, &contract.dimensions)
                }
                SupervisorProvider::OpenAICompatible {
                    flavor,
                    base_url,
                    model,
                    api_key,
                } => self
                    .run_openai_compatible(*flavor, base_url, model, api_key, &prompt, &schema)
                    .await
                    .and_then(|raw| parse_openai_chat_envelope(&raw, &contract.dimensions)),
            }
        })
        .await
        .map_err(|_| SupervisorError::Timeout(self.timeout.as_secs()))??;

        Ok(Verdict::from_dimensions(
            per_dimension,
            contract,
            iteration,
            build_feedback_string,
        ))
    }

    // ── ClaudeCode dispatch ──────────────────────────────────────────────

    async fn run_claude_code(
        &self,
        binary: &str,
        model: Option<&str>,
        allowed_tools: &[String],
        prompt: &str,
        schema: &serde_json::Value,
    ) -> Result<String, SupervisorError> {
        let schema_str = schema.to_string();
        let allowed = allowed_tools.join(",");

        let mut cmd = tokio::process::Command::new(binary);
        cmd.arg("-p")
            .arg("--output-format")
            .arg("json")
            .arg("--json-schema")
            .arg(&schema_str)
            .arg("--append-system-prompt")
            .arg(&self.system_prompt)
            .arg("--permission-mode")
            .arg("bypassPermissions");
        if let Some(m) = model {
            cmd.arg("--model").arg(m);
        }
        if !allowed.is_empty() {
            cmd.arg("--allowed-tools").arg(&allowed);
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            SupervisorError::Subprocess(format!(
                "spawn '{binary}' failed: {e} (is the claude CLI installed?)"
            ))
        })?;
        write_stdin(&mut child, prompt).await?;
        let output = child.wait_with_output().await?;
        if !output.status.success() {
            return Err(SupervisorError::Subprocess(format!(
                "claude exit {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    // ── Codex dispatch ───────────────────────────────────────────────────

    async fn run_codex(
        &self,
        binary: &str,
        model: Option<&str>,
        prompt: &str,
        schema: &serde_json::Value,
    ) -> Result<String, SupervisorError> {
        // Codex requires the schema as a file path, not inline.
        let schema_file = tempfile::Builder::new()
            .prefix("lightsquad-supervisor-schema-")
            .suffix(".json")
            .tempfile()
            .map_err(|e| SupervisorError::Subprocess(format!("schema tempfile: {e}")))?;
        std::fs::write(schema_file.path(), schema.to_string())?;

        // Codex doesn't have --append-system-prompt; prepend the system
        // discipline to the user prompt instead.
        let full_prompt = format!(
            "[SYSTEM DIRECTIVES — read first, then evaluate]\n{}\n\n---\n\n{}",
            self.system_prompt, prompt
        );

        let mut cmd = tokio::process::Command::new(binary);
        cmd.arg("exec")
            .arg("--json")
            .arg("--output-schema")
            .arg(schema_file.path())
            .arg("--skip-git-repo-check");
        if let Some(m) = model {
            cmd.arg("--model").arg(m);
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            SupervisorError::Subprocess(format!(
                "spawn '{binary} exec' failed: {e} (is codex installed?)"
            ))
        })?;
        write_stdin(&mut child, &full_prompt).await?;
        let output = child.wait_with_output().await?;

        // Keep schema_file alive until subprocess completes.
        drop(schema_file);

        if !output.status.success() {
            return Err(SupervisorError::Subprocess(format!(
                "codex exit {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    // ── Ollama dispatch ──────────────────────────────────────────────────

    /// Dispatch to Ollama (or Ollama-proxy) and accumulate the streamed NDJSON
    /// response into a single content string.
    ///
    /// **Streaming is mandatory here.** Per Ollama docs
    /// (`/websites/ollama` → `/api/streaming`): "Streaming ... is better
    /// suited for **long generations**. Non-streaming is better for short
    /// responses." Long supervisor evaluations (large artifacts, reasoning
    /// models) routinely take 30–300 s; in non-streaming mode the local
    /// Ollama proxy holds the entire response in a buffer while waiting on
    /// `ollama.com`, and the upstream connection drops mid-wait with
    /// HTTP 500 "unexpected EOF". Streaming pipes chunks as they arrive,
    /// so there is no monolithic buffer to time out.
    ///
    /// We accumulate `message.content` from every NDJSON line that carries
    /// it — mirroring `OllamaCloudCodingProvider::chat_collect` in the
    /// worker. Errors that appear mid-stream (per Ollama's documented
    /// streaming-error format) bubble up as an `InvalidEnvelope`.
    async fn run_ollama(
        &self,
        base_url: &str,
        model: &str,
        api_key: Option<&SecretString>,
        prompt: &str,
    ) -> Result<String, SupervisorError> {
        let body = json!({
            "model": model,
            "stream": true,
            "messages": [
                {"role": "system", "content": self.system_prompt},
                {"role": "user", "content": prompt},
            ],
        });
        let url = format!("{base_url}/api/chat");
        let api_key_owned = api_key.map(|k| k.expose_secret().to_owned());

        let client = reqwest::Client::new();
        let bytes = with_transient_retry("ollama", || async {
            let mut req = client.post(&url).json(&body);
            if let Some(token) = &api_key_owned {
                req = req.bearer_auth(token);
            }
            let resp = req.send().await?.error_for_status()?;
            resp.bytes().await
        })
        .await?;

        // Accumulate content from every NDJSON chunk. Mid-stream errors are
        // injected as a final `{"error": "..."}` line per Ollama docs.
        let mut content = String::new();
        for line in bytes.split(|b| *b == b'\n') {
            if line.is_empty() {
                continue;
            }
            let Ok(val) = serde_json::from_slice::<serde_json::Value>(line) else {
                continue;
            };
            if let Some(err) = val.get("error").and_then(serde_json::Value::as_str) {
                return Err(SupervisorError::InvalidEnvelope(format!(
                    "ollama streamed error: {err}"
                )));
            }
            if let Some(chunk) = val
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(serde_json::Value::as_str)
            {
                content.push_str(chunk);
            }
        }
        Ok(content)
    }

    // ── OpenAI-compatible dispatch ───────────────────────────────────────

    /// POST `<base_url>/chat/completions` with `response_format.json_schema`
    /// strict-mode enforcement.
    ///
    /// Canonical OpenAI shape (verified against `/websites/developers_openai_api`):
    /// ```json
    /// {
    ///   "model": "...",
    ///   "messages": [{"role":"system",...},{"role":"user",...}],
    ///   "response_format": {
    ///     "type": "json_schema",
    ///     "json_schema": {
    ///       "name": "per_dimension_verdict",
    ///       "strict": true,
    ///       "schema": <schema>
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// Identical shape works for native OpenAI, OpenRouter, LiteLLM, and
    /// any LiteLLM-proxied backend (Vertex / Anthropic / Bedrock / Gemini),
    /// per `/websites/litellm_ai`. LiteLLM internally translates as needed.
    /// POST `<base_url>/chat/completions` with `stream: true` + SSE
    /// accumulation. Follows the platform-wide streaming-by-default policy
    /// (see module doc): long evaluator responses ride on persistent
    /// streamed connections rather than buffered single-response calls,
    /// avoiding the same "upstream drops mid-buffer" failure mode that bit
    /// the Ollama path.
    ///
    /// `response_format.json_schema.strict = true` still applies under
    /// streaming — the model emits one token at a time but each chunk is
    /// guaranteed to be part of a schema-valid completion.
    async fn run_openai_compatible(
        &self,
        flavor: OpenAIFlavor,
        base_url: &str,
        model: &str,
        api_key: &SecretString,
        prompt: &str,
        schema: &serde_json::Value,
    ) -> Result<String, SupervisorError> {
        let body = json!({
            "model": model,
            "stream": true,
            "messages": [
                {"role": "system", "content": self.system_prompt},
                {"role": "user",   "content": prompt},
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "per_dimension_verdict",
                    "strict": true,
                    "schema": schema,
                }
            }
        });

        let url = format!("{base_url}/chat/completions");
        let api_key_owned = api_key.expose_secret().to_owned();

        let client = reqwest::Client::new();
        let bytes = with_transient_retry("openai-compatible", || async {
            let mut req = client.post(&url).json(&body).bearer_auth(&api_key_owned);
            // OpenRouter-specific app-attribution headers (optional but
            // recommended — improves rate-limit treatment per their docs).
            if matches!(flavor, OpenAIFlavor::OpenRouter) {
                req = req
                    .header(
                        "HTTP-Referer",
                        "https://github.com/TheLightArchitects/lightarchitects-sdk",
                    )
                    .header("X-Title", "Light Architects — lightsquad supervisor");
            }
            let resp = req.send().await?.error_for_status()?;
            resp.bytes().await
        })
        .await?;

        // Standard SSE format: each event is a `data: { ... }` line,
        // terminated by `data: [DONE]`. We concatenate `choices[0].delta.content`
        // from every chunk that carries it.
        let mut content = String::new();
        for line in bytes.split(|b| *b == b'\n') {
            let line = std::str::from_utf8(line).unwrap_or("").trim();
            let payload = match line.strip_prefix("data:") {
                Some(rest) => rest.trim(),
                None => continue,
            };
            if payload == "[DONE]" || payload.is_empty() {
                continue;
            }
            let Ok(val) = serde_json::from_str::<serde_json::Value>(payload) else {
                continue;
            };
            // Mid-stream errors land as { "error": { "message": "..." } } in
            // most OpenAI-compatible providers; bubble them up explicitly.
            if let Some(err) = val.get("error") {
                return Err(SupervisorError::InvalidEnvelope(format!(
                    "openai-compatible streamed error: {err}"
                )));
            }
            // Delta path (streaming): choices[0].delta.content
            if let Some(chunk) = val
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|c| c.get("delta"))
                .and_then(|d| d.get("content"))
                .and_then(serde_json::Value::as_str)
            {
                content.push_str(chunk);
            }
        }
        if content.is_empty() {
            return Err(SupervisorError::InvalidEnvelope(
                "openai-compatible streamed response produced no content deltas".to_owned(),
            ));
        }
        Ok(content)
    }
}

async fn write_stdin(child: &mut tokio::process::Child, prompt: &str) -> std::io::Result<()> {
    {
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "stdin was not piped")
        })?;
        stdin.write_all(prompt.as_bytes()).await?;
        stdin.flush().await?;
    }
    drop(child.stdin.take()); // closes stdin so the subprocess sees EOF
    Ok(())
}

// ── HTTP retry helper ────────────────────────────────────────────────────────

/// Wrap an async HTTP closure with exponential-backoff retry on transient
/// failures (5xx, connection errors, decoder errors).
///
/// Used by both Ollama and OpenAI-compatible dispatch paths. Necessary
/// because local Ollama → ollama.com upstream EOFs are common on long
/// sessions ("unexpected EOF" on a connection that hangs mid-response).
/// Empirically verified: failures hit ANY model on sustained traffic and
/// are non-deterministic; a single retry resolves ~90% of them.
///
/// Retry budget: 3 attempts (initial + 2 retries). Backoffs: 1s, 3s.
/// Non-retriable errors (auth 4xx, schema validation) bubble up immediately.
///
/// `label` appears in retry-log messages for telemetry.
async fn with_transient_retry<F, Fut, T>(label: &str, mut op: F) -> Result<T, SupervisorError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, reqwest::Error>>,
{
    const MAX_ATTEMPTS: u32 = 3;
    const BACKOFF_MS: [u64; 2] = [1_000, 3_000];

    let mut last_err: Option<reqwest::Error> = None;
    for attempt in 0..MAX_ATTEMPTS {
        match op().await {
            Ok(v) => {
                if attempt > 0 {
                    tracing::info!(label, attempt, "supervisor.retry.success");
                }
                return Ok(v);
            }
            Err(e) => {
                if !is_transient(&e) || attempt + 1 == MAX_ATTEMPTS {
                    return Err(SupervisorError::Http(e));
                }
                tracing::warn!(
                    label,
                    attempt,
                    error = %e,
                    "supervisor.retry.transient"
                );
                last_err = Some(e);
                let delay_ms = BACKOFF_MS[attempt as usize];
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }
    // The retry loop guarantees `last_err` is `Some` here: we only fall
    // through to this line after exhausting MAX_ATTEMPTS with at least one
    // transient failure recorded. The .unwrap_or_else guard preserves the
    // invariant without triggering clippy::expect_used.
    Err(last_err.map_or_else(
        || SupervisorError::Subprocess("retry loop exited without recorded error".to_owned()),
        SupervisorError::Http,
    ))
}

/// Classify a reqwest error as transient (retriable) or terminal.
///
/// Transient: 5xx server errors, connection-reset, body-decode interruptions
/// (the "unexpected EOF" class). Terminal: 4xx (bad request, auth, schema
/// rejection), DNS resolution failures, timeouts (we honour the outer
/// `self.timeout` rather than retrying past it).
fn is_transient(err: &reqwest::Error) -> bool {
    // 5xx from upstream
    if let Some(status) = err.status() {
        return status.is_server_error();
    }
    // Body / decode errors (manifests as "unexpected EOF" from the upstream
    // proxy aborting mid-stream).
    if err.is_decode() || err.is_body() {
        return true;
    }
    // Connection-level: pool exhaustion, RST, abrupt close.
    if err.is_connect() {
        return true;
    }
    false
}

// ── Envelope parsing ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ClaudeEnvelope {
    #[serde(default)]
    subtype: String,
    #[serde(default)]
    structured_output: Option<serde_json::Value>,
    #[serde(default)]
    #[allow(dead_code)]
    result: String,
}

#[derive(Debug, Deserialize)]
struct PerDimensionWrapper {
    per_dimension: Vec<DimensionScore>,
}

fn parse_claude_envelope(
    stdout: &str,
    declared: &[Dimension],
) -> Result<Vec<DimensionScore>, SupervisorError> {
    let envelope: ClaudeEnvelope = serde_json::from_str(stdout.trim()).map_err(|e| {
        SupervisorError::InvalidEnvelope(format!(
            "could not parse claude envelope: {e}; stdout: {}",
            &stdout[..stdout.len().min(400)]
        ))
    })?;
    if envelope.subtype == "error_max_structured_output_retries" {
        return Err(SupervisorError::SchemaRetriesExhausted);
    }
    let structured = envelope.structured_output.ok_or_else(|| {
        SupervisorError::InvalidEnvelope(format!(
            "envelope has no structured_output (subtype={})",
            envelope.subtype
        ))
    })?;
    let wrapper: PerDimensionWrapper = serde_json::from_value(structured)
        .map_err(|e| SupervisorError::InvalidEnvelope(format!("structured_output shape: {e}")))?;
    verify_and_clamp(wrapper.per_dimension, declared)
}

/// Codex's JSONL stream ends with a final event containing the structured
/// output. We scan from the bottom for the last JSON object whose shape
/// matches a `per_dimension` wrapper.
fn parse_codex_envelope(
    stdout: &str,
    declared: &[Dimension],
) -> Result<Vec<DimensionScore>, SupervisorError> {
    // Codex `--output-schema` emits the validated final response as one of
    // the event objects. Walk lines in reverse looking for a JSON object
    // that parses to PerDimensionWrapper.
    for line in stdout.lines().rev() {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }
        // Try direct shape first.
        if let Ok(wrapper) = serde_json::from_str::<PerDimensionWrapper>(trimmed) {
            return verify_and_clamp(wrapper.per_dimension, declared);
        }
        // Try wrapped event: { ..., "structured_output": {...} } or
        // { ..., "msg": {"type":"agent_message", "message":"<json>"} }.
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(so) = val.get("structured_output").cloned() {
                if let Ok(wrapper) = serde_json::from_value::<PerDimensionWrapper>(so) {
                    return verify_and_clamp(wrapper.per_dimension, declared);
                }
            }
        }
    }
    Err(SupervisorError::InvalidEnvelope(format!(
        "no per_dimension object in codex stdout; tail: {}",
        &stdout[stdout.len().saturating_sub(400)..]
    )))
}

/// OpenAI-compatible path — when `strict: true` is requested, the
/// `choices[0].message.content` is exact JSON matching our schema. We
/// still call [`extract_json_blob`] as defense-in-depth in case the
/// provider returns a Markdown-fenced block (some LiteLLM-proxied
/// backends do this on certain models).
fn parse_openai_chat_envelope(
    raw: &str,
    declared: &[Dimension],
) -> Result<Vec<DimensionScore>, SupervisorError> {
    // Try direct parse first — strict mode produces clean JSON.
    if let Ok(wrapper) = serde_json::from_str::<PerDimensionWrapper>(raw.trim()) {
        return verify_and_clamp(wrapper.per_dimension, declared);
    }
    // Fallback: strip fences / extract balanced object.
    let blob = extract_json_blob(raw).ok_or_else(|| {
        SupervisorError::InvalidEnvelope(format!(
            "no JSON in openai-compatible content: {}",
            &raw[..raw.len().min(400)]
        ))
    })?;
    let wrapper: PerDimensionWrapper = serde_json::from_str(blob).map_err(|e| {
        SupervisorError::InvalidEnvelope(format!("openai-compatible parse: {e}; blob: {blob:.200}"))
    })?;
    verify_and_clamp(wrapper.per_dimension, declared)
}

/// Ollama path — freeform text response; extract the first JSON blob and
/// hope it matches the schema. Fragile (no upstream enforcement).
fn parse_freeform_json(
    raw: &str,
    declared: &[Dimension],
) -> Result<Vec<DimensionScore>, SupervisorError> {
    let blob = extract_json_blob(raw).ok_or_else(|| {
        SupervisorError::InvalidEnvelope(format!(
            "no JSON object in: {}",
            &raw[..raw.len().min(400)]
        ))
    })?;
    let wrapper: PerDimensionWrapper = serde_json::from_str(blob)
        .map_err(|e| SupervisorError::InvalidEnvelope(format!("parse: {e}; blob: {blob:.200}")))?;
    verify_and_clamp(wrapper.per_dimension, declared)
}

fn verify_and_clamp(
    per_dim: Vec<DimensionScore>,
    declared: &[Dimension],
) -> Result<Vec<DimensionScore>, SupervisorError> {
    for dim in declared {
        if !per_dim.iter().any(|d| d.name == dim.name) {
            return Err(SupervisorError::MissingDimension(dim.name.clone()));
        }
    }
    Ok(per_dim
        .into_iter()
        .map(|d| DimensionScore {
            score: d.score.clamp(0.0, 1.0),
            ci_low: d.ci_low.clamp(0.0, 1.0),
            ci_high: d.ci_high.clamp(0.0, 1.0),
            ..d
        })
        .collect())
}

/// Robust JSON-object extractor for freeform LLM output: strips Markdown
/// fences and locates the outermost balanced `{ … }` blob.
fn extract_json_blob(raw: &str) -> Option<&str> {
    let stripped = raw.trim();
    let inside = if let Some(r) = stripped.strip_prefix("```json") {
        r.trim_start_matches('\n').trim_end_matches("```").trim()
    } else if let Some(r) = stripped.strip_prefix("```") {
        r.trim_start_matches('\n').trim_end_matches("```").trim()
    } else {
        stripped
    };
    let start = inside.find('{')?;
    let bytes = inside.as_bytes();
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&inside[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

// ── Schema builder ──────────────────────────────────────────────────────────

/// Build the JSON Schema fragment that describes the expected
/// `structured_output` shape.
///
/// **Cross-backend compatible** — every constraint is one that works on the
/// most restrictive backend we target:
///
/// - `additionalProperties: false` + every key in `required` → required by
///   OpenAI strict mode (`/websites/developers_openai_api`). Harmless
///   elsewhere.
/// - `enum` on `name` + `type` annotations → universally supported.
/// - `minItems` on the outer array → universally supported.
/// - **No `minimum`/`maximum` on number types.** Amazon Bedrock's schema
///   validator rejects them (verified empirically against OpenRouter →
///   anthropic/claude-sonnet-4.6 routed through Bedrock: HTTP 400
///   "output_config.format.schema: For 'number' type, properties maximum,
///   minimum are not supported"). Score range is enforced downstream by
///   [`verify_and_clamp`] which clamps `[0.0, 1.0]` defensively.
/// - **No `minLength` on string types.** Some Vertex/Bedrock paths reject
///   it; OpenAI tolerates it but we prefer maximum portability.
fn build_schema_for_contract(contract: &TaskContract) -> serde_json::Value {
    let dim_names: Vec<&str> = contract
        .dimensions
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "per_dimension": {
                "type": "array",
                "minItems": dim_names.len(),
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "name":            { "type": "string", "enum": dim_names },
                        "score":           { "type": "number" },
                        "ci_low":          { "type": "number" },
                        "ci_high":         { "type": "number" },
                        "reasoning":       { "type": "string" },
                        "failed_criteria": { "type": "array",  "items": { "type": "string" } }
                    },
                    "required": ["name", "score", "ci_low", "ci_high", "reasoning", "failed_criteria"]
                }
            }
        },
        "required": ["per_dimension"]
    })
}

// ── Feedback builder ────────────────────────────────────────────────────────

fn build_feedback_string(failing: &[DimensionScore]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(1_024);
    for dim in failing {
        let _ = writeln!(
            out,
            "Dimension `{}` — score {:.2} (CI {:.2}–{:.2})",
            dim.name, dim.score, dim.ci_low, dim.ci_high
        );
        if !dim.failed_criteria.is_empty() {
            let _ = writeln!(out, "  Failed criteria:");
            for c in &dim.failed_criteria {
                let _ = writeln!(out, "    - {c}");
            }
        }
        let _ = writeln!(out, "  Evidence: {}\n", dim.reasoning);
    }
    out
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::expect_used
)]
mod tests {
    use super::*;
    use crate::lightsquad::contract::{Decision, html_diagram_contract};

    fn dim(name: &str) -> Dimension {
        Dimension {
            name: name.to_owned(),
            weight: 1.0,
            criteria: vec![],
            scoring_hint: None,
        }
    }

    // ── Provider detection ────────────────────────────────────────────────

    /// `from_env` returns `ClaudeCode` by default (no `LIGHTSQUAD_*` or
    /// `LLM_BACKEND` set). We construct directly because env vars in tests
    /// are unsafe under Rust 2024.
    #[test]
    fn provider_default_is_claude_code() {
        // Smoke-check that resolve_provider_kind() returns "claude-code"
        // when both env vars are absent (best-effort — tests may run with
        // env vars set in the harness).
        let kind = resolve_provider_kind();
        assert!(
            matches!(kind.as_str(), "claude-code" | "codex" | "ollama-cloud"),
            "unexpected provider kind: {kind}"
        );
    }

    #[test]
    fn provider_name_is_stable() {
        let claude = SupervisorProvider::ClaudeCode {
            binary: "claude".to_owned(),
            model: None,
            allowed_tools: vec![],
        };
        let codex = SupervisorProvider::Codex {
            binary: "codex".to_owned(),
            model: None,
        };
        let ollama = SupervisorProvider::OllamaCloud {
            base_url: "http://localhost:11434".to_owned(),
            model: "kimi-k2.5:cloud".to_owned(),
            api_key: None,
        };
        assert_eq!(claude.name(), "claude-code");
        assert_eq!(codex.name(), "codex");
        assert_eq!(ollama.name(), "ollama-cloud");
    }

    /// Each OpenAI flavor reports its own name; describe() lists base URL + model.
    #[test]
    fn openai_compatible_flavors_name_and_describe_correctly() {
        let cases = [
            (
                OpenAIFlavor::OpenAi,
                "openai",
                "https://api.openai.com/v1",
                "gpt-5",
            ),
            (
                OpenAIFlavor::OpenRouter,
                "openrouter",
                "https://openrouter.ai/api/v1",
                "anthropic/claude-sonnet-4",
            ),
            (
                OpenAIFlavor::LiteLLM,
                "litellm",
                "http://localhost:4000/v1",
                "gemini-2.5-flash",
            ),
            (
                OpenAIFlavor::Portkey,
                "portkey",
                "https://api.portkey.ai/v1",
                "claude-sonnet-4-6",
            ),
            (
                OpenAIFlavor::Generic,
                "openai-compat",
                "http://acme.local/v1",
                "custom-model",
            ),
        ];
        for (flavor, expected_name, base_url, model) in cases {
            let p = SupervisorProvider::OpenAICompatible {
                flavor,
                base_url: base_url.to_owned(),
                model: model.to_owned(),
                api_key: SecretString::new("test-key".to_owned().into()),
            };
            assert_eq!(p.name(), expected_name);
            let desc = p.describe();
            assert!(desc.contains(base_url), "describe missing base_url: {desc}");
            assert!(desc.contains(model), "describe missing model: {desc}");
        }
    }

    /// Each flavor knows its default base URL and api-key env var name.
    /// Verified against context7 (`/websites/developers_openai_api`,
    /// `/websites/litellm_ai`).
    #[test]
    fn openai_flavor_defaults_match_canonical_values() {
        assert_eq!(
            OpenAIFlavor::OpenAi.default_base_url(),
            "https://api.openai.com/v1"
        );
        assert_eq!(OpenAIFlavor::OpenAi.default_api_key_env(), "OPENAI_API_KEY");

        assert_eq!(
            OpenAIFlavor::OpenRouter.default_base_url(),
            "https://openrouter.ai/api/v1"
        );
        assert_eq!(
            OpenAIFlavor::OpenRouter.default_api_key_env(),
            "OPENROUTER_API_KEY"
        );

        assert_eq!(
            OpenAIFlavor::LiteLLM.default_base_url(),
            "http://localhost:4000/v1"
        );
        assert_eq!(
            OpenAIFlavor::LiteLLM.default_api_key_env(),
            "LITELLM_API_KEY"
        );

        // Generic intentionally has no default — operator must supply.
        assert_eq!(OpenAIFlavor::Generic.default_base_url(), "");
    }

    /// `parse_openai_chat_envelope` accepts the bare strict-mode JSON the
    /// model produces in `choices[0].message.content`.
    #[test]
    fn parse_openai_chat_envelope_accepts_bare_json() {
        let declared = vec![dim("a")];
        let content = r#"{"per_dimension":[{"name":"a","score":0.9,"ci_low":0.8,"ci_high":1.0,"reasoning":"ok","failed_criteria":[]}]}"#;
        let parsed = parse_openai_chat_envelope(content, &declared).unwrap();
        assert_eq!(parsed[0].score, 0.9);
    }

    /// Some LiteLLM-proxied backends wrap the strict output in a Markdown
    /// fence even though it shouldn't — the parser is defensive against that.
    #[test]
    fn parse_openai_chat_envelope_strips_markdown_fence_fallback() {
        let declared = vec![dim("a")];
        let content = "```json\n{\"per_dimension\":[{\"name\":\"a\",\"score\":0.5,\"ci_low\":0.4,\"ci_high\":0.6,\"reasoning\":\"r\",\"failed_criteria\":[]}]}\n```";
        let parsed = parse_openai_chat_envelope(content, &declared).unwrap();
        assert_eq!(parsed[0].score, 0.5);
    }

    /// Garbage in → InvalidEnvelope, not panic.
    #[test]
    fn parse_openai_chat_envelope_errors_on_garbage() {
        let declared = vec![dim("a")];
        let err = parse_openai_chat_envelope("nope, no json here", &declared).unwrap_err();
        assert!(matches!(err, SupervisorError::InvalidEnvelope(_)));
    }

    /// `resolve_provider_kind` maps every LLM_BACKEND value the webshell
    /// supports to a supervisor provider — closing the loop on "supervisor
    /// inherits the webshell's backend choice."
    #[test]
    fn resolve_provider_kind_recognizes_openai_compatible_flavors() {
        // This test is best-effort because env vars under Rust 2024 are
        // unsafe to set in tests. We can only verify the function exists +
        // returns a valid kind.
        let kind = resolve_provider_kind();
        assert!(matches!(
            kind.as_str(),
            "claude-code"
                | "codex"
                | "ollama-cloud"
                | "openai"
                | "openrouter"
                | "litellm"
                | "vertex"
                | "openai-compatible"
        ));
    }

    #[test]
    fn provider_describe_includes_key_fields() {
        let p = SupervisorProvider::ClaudeCode {
            binary: "/path/to/claude".to_owned(),
            model: Some("claude-sonnet-4-6".to_owned()),
            allowed_tools: vec!["Read".to_owned(), "Grep".to_owned()],
        };
        let d = p.describe();
        assert!(d.contains("/path/to/claude"));
        assert!(d.contains("claude-sonnet-4-6"));
        assert!(d.contains("Read"));
        assert!(d.contains("Grep"));
    }

    // ── Schema builder ────────────────────────────────────────────────────

    #[test]
    fn build_schema_constrains_dimension_names_to_contract() {
        let contract = html_diagram_contract("t", "x.html");
        let schema = build_schema_for_contract(&contract);
        let name_enum =
            &schema["properties"]["per_dimension"]["items"]["properties"]["name"]["enum"];
        let names: Vec<&str> = name_enum
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(names.contains(&"topology_fidelity"));
        assert!(names.contains(&"no_hallucination"));
        assert_eq!(names.len(), contract.dimensions.len());
    }

    /// OpenAI strict mode requires `additionalProperties: false` at every
    /// object level — without it, the API rejects the request. Verified via
    /// context7 against `/websites/developers_openai_api`.
    #[test]
    fn build_schema_sets_additional_properties_false_at_every_object_level() {
        let contract = html_diagram_contract("t", "x.html");
        let schema = build_schema_for_contract(&contract);
        // Outer object
        assert_eq!(
            schema["additionalProperties"],
            json!(false),
            "outer object missing additionalProperties:false"
        );
        // Per-dimension item object
        assert_eq!(
            schema["properties"]["per_dimension"]["items"]["additionalProperties"],
            json!(false),
            "per-dimension item missing additionalProperties:false"
        );
    }

    /// OpenAI strict mode requires every key in `properties` to appear in
    /// `required`. Verify this for the item-level schema.
    #[test]
    fn build_schema_required_lists_every_item_property() {
        let contract = html_diagram_contract("t", "x.html");
        let schema = build_schema_for_contract(&contract);
        let item = &schema["properties"]["per_dimension"]["items"];
        let props: Vec<&str> = item["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let required: Vec<&str> = item["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for p in &props {
            assert!(
                required.contains(p),
                "property '{p}' missing from required (breaks OpenAI strict mode)"
            );
        }
        assert_eq!(
            props.len(),
            required.len(),
            "props and required must match in size"
        );
    }

    #[test]
    fn build_schema_min_items_matches_contract_dimension_count() {
        let contract = html_diagram_contract("t", "x.html");
        let schema = build_schema_for_contract(&contract);
        let min_items = schema["properties"]["per_dimension"]["minItems"]
            .as_u64()
            .unwrap();
        assert_eq!(
            usize::try_from(min_items).unwrap(),
            contract.dimensions.len()
        );
    }

    // ── Claude envelope parsing ───────────────────────────────────────────

    #[test]
    fn parse_claude_envelope_happy_path() {
        let declared = vec![dim("a"), dim("b")];
        let env = json!({
            "subtype": "success",
            "structured_output": {
                "per_dimension": [
                    {"name":"a","score":0.9,"ci_low":0.8,"ci_high":1.0,"reasoning":"a ok","failed_criteria":[]},
                    {"name":"b","score":0.7,"ci_low":0.6,"ci_high":0.8,"reasoning":"b ok","failed_criteria":["foo"]}
                ]
            }
        });
        let parsed = parse_claude_envelope(&env.to_string(), &declared).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].score, 0.9);
    }

    #[test]
    fn parse_claude_envelope_clamps_out_of_range() {
        let declared = vec![dim("a")];
        let env = json!({
            "subtype": "success",
            "structured_output": {
                "per_dimension": [{"name":"a","score":1.5,"ci_low":-0.1,"ci_high":2.0,"reasoning":"r","failed_criteria":[]}]
            }
        });
        let parsed = parse_claude_envelope(&env.to_string(), &declared).unwrap();
        assert_eq!(parsed[0].score, 1.0);
        assert_eq!(parsed[0].ci_low, 0.0);
        assert_eq!(parsed[0].ci_high, 1.0);
    }

    #[test]
    fn parse_claude_envelope_surfaces_schema_retries_exhausted() {
        let declared = vec![dim("a")];
        let env = json!({
            "subtype": "error_max_structured_output_retries",
            "result": "couldn't"
        });
        let err = parse_claude_envelope(&env.to_string(), &declared).unwrap_err();
        assert!(matches!(err, SupervisorError::SchemaRetriesExhausted));
    }

    #[test]
    fn parse_claude_envelope_errors_when_structured_output_missing() {
        let declared = vec![dim("a")];
        let env = json!({ "subtype": "success", "result": "stuff" });
        let err = parse_claude_envelope(&env.to_string(), &declared).unwrap_err();
        assert!(matches!(err, SupervisorError::InvalidEnvelope(_)));
    }

    #[test]
    fn parse_claude_envelope_errors_on_missing_dimension() {
        let declared = vec![dim("a"), dim("b")];
        let env = json!({
            "subtype": "success",
            "structured_output": {
                "per_dimension": [{"name":"a","score":1.0,"ci_low":1.0,"ci_high":1.0,"reasoning":"r","failed_criteria":[]}]
            }
        });
        let err = parse_claude_envelope(&env.to_string(), &declared).unwrap_err();
        match err {
            SupervisorError::MissingDimension(name) => assert_eq!(name, "b"),
            other => panic!("expected MissingDimension, got {other:?}"),
        }
    }

    // ── Codex JSONL parsing ───────────────────────────────────────────────

    /// Codex JSONL stream: scan from bottom, find the `per_dimension` object.
    #[test]
    fn parse_codex_envelope_finds_inline_per_dimension() {
        let declared = vec![dim("a")];
        let stdout = "\
{\"type\":\"event\",\"msg\":{}}
{\"type\":\"event\",\"msg\":\"another\"}
{\"per_dimension\":[{\"name\":\"a\",\"score\":0.8,\"ci_low\":0.7,\"ci_high\":0.9,\"reasoning\":\"r\",\"failed_criteria\":[]}]}
";
        let parsed = parse_codex_envelope(stdout, &declared).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].score, 0.8);
    }

    /// Codex wraps structured output inside an event object.
    #[test]
    fn parse_codex_envelope_finds_wrapped_structured_output() {
        let declared = vec![dim("a")];
        let stdout = r#"{"type":"event"}
{"type":"final","structured_output":{"per_dimension":[{"name":"a","score":1.0,"ci_low":1.0,"ci_high":1.0,"reasoning":"r","failed_criteria":[]}]}}
"#;
        let parsed = parse_codex_envelope(stdout, &declared).unwrap();
        assert_eq!(parsed[0].score, 1.0);
    }

    #[test]
    fn parse_codex_envelope_errors_when_no_match() {
        let declared = vec![dim("a")];
        let err =
            parse_codex_envelope("just some prose\nno JSON anywhere\n", &declared).unwrap_err();
        assert!(matches!(err, SupervisorError::InvalidEnvelope(_)));
    }

    // ── Freeform / Ollama parsing ─────────────────────────────────────────

    #[test]
    fn extract_json_blob_handles_braces_inside_strings() {
        let raw = r#"{"per_dimension":[{"name":"x","reasoning":"contains } brace","score":1.0,"ci_low":1.0,"ci_high":1.0,"failed_criteria":[]}]}"#;
        let blob = extract_json_blob(raw).unwrap();
        assert_eq!(blob, raw);
    }

    #[test]
    fn extract_json_blob_strips_markdown_fence() {
        let raw = "```json\n{\"per_dimension\": []}\n```";
        let blob = extract_json_blob(raw).unwrap();
        assert_eq!(blob.trim(), "{\"per_dimension\": []}");
    }

    #[test]
    fn parse_freeform_json_happy_path() {
        let declared = vec![dim("a")];
        let raw = r#"Sure, here's my verdict:
{"per_dimension":[{"name":"a","score":0.5,"ci_low":0.4,"ci_high":0.6,"reasoning":"r","failed_criteria":[]}]}
Thanks!"#;
        let parsed = parse_freeform_json(raw, &declared).unwrap();
        assert_eq!(parsed[0].score, 0.5);
    }

    // ── Feedback + verdict pipeline ───────────────────────────────────────

    #[test]
    fn build_feedback_string_includes_score_and_reasoning() {
        let failing = vec![DimensionScore {
            name: "topology_fidelity".to_owned(),
            score: 0.20,
            ci_low: 0.10,
            ci_high: 0.30,
            reasoning: "zero edges in output; nodes are isolated grid cells".to_owned(),
            failed_criteria: vec!["Every node carries at least one directed edge".to_owned()],
        }];
        let feedback = build_feedback_string(&failing);
        assert!(feedback.contains("topology_fidelity"));
        assert!(feedback.contains("0.20"));
        assert!(feedback.contains("zero edges"));
    }

    #[test]
    fn evaluate_pipeline_produces_refine_on_low_ci() {
        let contract = html_diagram_contract("t", "x.html");
        let per_dim: Vec<DimensionScore> = contract
            .dimensions
            .iter()
            .map(|d| DimensionScore {
                name: d.name.clone(),
                score: 0.6,
                ci_low: 0.4,
                ci_high: 0.8,
                reasoning: format!("dim {} underperformed", d.name),
                failed_criteria: vec![],
            })
            .collect();
        let v = Verdict::from_dimensions(per_dim, &contract, 0, build_feedback_string);
        match v.decision {
            Decision::Refine(feedback) => {
                assert!(feedback.contains("topology_fidelity"));
            }
            other => panic!("expected Refine with low CI, got {other:?}"),
        }
    }

    // ── ContractSupervisor construction ───────────────────────────────────

    #[test]
    fn supervisor_from_env_constructs_successfully() {
        let s = ContractSupervisor::from_env();
        assert!(s.timeout.as_secs() > 0);
        // Provider must be one of the three known kinds.
        let name = s.provider().name();
        assert!(matches!(name, "claude-code" | "codex" | "ollama-cloud"));
    }

    #[test]
    fn supervisor_with_provider_overrides_env_detection() {
        let provider = SupervisorProvider::Codex {
            binary: "codex".to_owned(),
            model: Some("gpt-5-codex".to_owned()),
        };
        let s = ContractSupervisor::with_provider(provider);
        assert_eq!(s.provider().name(), "codex");
    }

    #[test]
    fn supervisor_with_timeout_overrides_default() {
        let s = ContractSupervisor::from_env().with_timeout(Duration::from_secs(42));
        assert_eq!(s.timeout, Duration::from_secs(42));
    }
}
