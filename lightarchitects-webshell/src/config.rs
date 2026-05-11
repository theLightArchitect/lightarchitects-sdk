//! Webshell runtime configuration resolved from CLI args + environment.
//!
//! Token resolution order:
//!   1. `LIGHTARCHITECTS_WEBSHELL_TOKEN` env var (explicit override)
//!   2. OS keyring (macOS Keychain / Linux Secret Service)
//!   3. `~/.lightarchitects/webshell/.token` (auto-generated on first run)
//!   4. If none exists: generate a random UUID token and persist to keyring + file.

use std::{ffi::OsString, path::PathBuf};

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

/// Environment variable for explicit token override.
pub const TOKEN_ENV: &str = "LIGHTARCHITECTS_WEBSHELL_TOKEN";

/// Keyring service name for the webshell token.
pub const KEYRING_SERVICE: &str = "lightarchitects";

/// Keyring username for the webshell token.
pub const KEYRING_USERNAME: &str = "webshell-token";

/// Default bind port.
pub const DEFAULT_PORT: u16 = 8733;

/// Default PTY host command.
pub const DEFAULT_HOST_CMD: &str = "claude";

/// Default Ollama base URL (Anthropic-compat endpoint).
pub const DEFAULT_OLLAMA_BASE_URL: &str = "http://localhost:11434";

/// Default Ollama model when the cloud profile is selected without an explicit model.
pub const DEFAULT_OLLAMA_MODEL: &str = "qwen3-coder:480b-cloud";

/// Which CLI binary runs in the embedded PTY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    /// Anthropic's Claude Code CLI (binary: `claude`).
    Lightarchitects,
    /// `OpenAI` Codex CLI (binary: `codex`).
    Codex,
    /// lÆx0 native binary (`lightarchitects-cli`).
    LightarchitectsNative,
}

impl Default for AgentKind {
    fn default() -> Self {
        Self::Lightarchitects
    }
}

/// Backend routing choice for the Claude Code agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeBackendKind {
    /// Use Claude Code's built-in Anthropic auth (subscription or API key).
    Anthropic,
    /// Persistent subprocess via Ollama — replicates `ollama launch claude --model <model>`.
    OllamaLaunch,
    /// Stateless HTTP to Ollama's Anthropic-compat `/v1/messages` endpoint.
    Ollama,
}

impl Default for ClaudeBackendKind {
    fn default() -> Self {
        Self::Anthropic
    }
}

/// Model + base-URL for an Ollama-launched subprocess session.
///
/// Shared between [`ClaudeBackend::OllamaLaunch`] and [`CodexBackend::OllamaLaunch`].
/// Replicates the env vars injected by `ollama launch <tool> --model <model>`:
///
/// **Claude Code**: `ANTHROPIC_AUTH_TOKEN=ollama`, `ANTHROPIC_API_KEY=""`,
/// `ANTHROPIC_BASE_URL=<base_url>`, plus per-tier model overrides so Claude's
/// internal model-switching lands on Ollama instead of `api.anthropic.com`.
///
/// **Codex**: `OPENAI_BASE_URL=<base_url>/v1`, `OPENAI_API_KEY=ollama`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OllamaLaunchConfig {
    /// Model name — local or cloud (`:cloud` suffix).
    /// Examples: `qwen3.5`, `kimi-k2.5:cloud`, `glm-5:cloud`, `gpt-oss:120b-cloud`.
    pub model: String,
    /// Ollama base URL (default: `http://127.0.0.1:11434` — matches what
    /// `ollama launch` actually injects, verified empirically).
    #[serde(default = "default_ollama_launch_base_url")]
    pub base_url: String,
}

fn default_ollama_launch_base_url() -> String {
    "http://127.0.0.1:11434".to_owned()
}

impl Default for OllamaLaunchConfig {
    fn default() -> Self {
        Self {
            model: DEFAULT_OLLAMA_MODEL.to_owned(),
            base_url: default_ollama_launch_base_url(),
        }
    }
}

/// Ollama HTTP routing configuration — loaded from disk + CLI, merged per-build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Anthropic-compat base URL (default: `http://localhost:11434`).
    pub base_url: String,
    /// Model name; cloud models use `:cloud` suffix (e.g. `qwen3-coder:480b-cloud`).
    pub model: String,
    /// Auth token. For local-only use, `"ollama"` is the documented placeholder.
    pub auth_token: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_OLLAMA_BASE_URL.to_owned(),
            model: DEFAULT_OLLAMA_MODEL.to_owned(),
            auth_token: "ollama".to_owned(),
        }
    }
}

/// Fully-resolved Claude Code backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClaudeBackend {
    /// Normal Anthropic auth — OAuth subscription or `ANTHROPIC_API_KEY`.
    Anthropic,
    /// Persistent subprocess via Ollama (replicates `ollama launch claude`).
    OllamaLaunch(OllamaLaunchConfig),
    /// Stateless HTTP to Ollama's Anthropic-compat endpoint.
    Ollama(OllamaConfig),
}

impl Default for ClaudeBackend {
    fn default() -> Self {
        Self::Anthropic
    }
}

impl ClaudeBackend {
    /// Return the kind without the embedded config.
    #[must_use]
    pub fn kind(&self) -> ClaudeBackendKind {
        match self {
            Self::Anthropic => ClaudeBackendKind::Anthropic,
            Self::OllamaLaunch(_) => ClaudeBackendKind::OllamaLaunch,
            Self::Ollama(_) => ClaudeBackendKind::Ollama,
        }
    }
}

/// Backend routing for the Codex CLI agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CodexBackend {
    /// Default `OpenAI` backend — uses `OPENAI_API_KEY` from the environment.
    OpenAi,
    /// Route Codex through Ollama (replicates `ollama launch codex --model <model>`).
    OllamaLaunch(OllamaLaunchConfig),
}

impl Default for CodexBackend {
    fn default() -> Self {
        Self::OpenAi
    }
}

fn default_codex_model() -> String {
    "o3".to_owned()
}

/// Codex CLI agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexConfig {
    /// Model name. Default `"o3"` for `OpenAI`; use cloud model for `OllamaLaunch`
    /// (e.g. `"gpt-oss:120b-cloud"`, `"glm-5:cloud"`).
    #[serde(default = "default_codex_model")]
    pub model: String,
    /// Which backend supplies the LLM.
    #[serde(default)]
    pub backend: CodexBackend,
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            model: default_codex_model(),
            backend: CodexBackend::default(),
        }
    }
}

/// Configuration for the lÆx0 native binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightarchitectsNativeConfig {
    /// Path to the `lightarchitects-cli` binary. Default: `"lightarchitects-cli"` (assumes on `$PATH`).
    #[serde(default = "default_lightarchitects_cli_binary")]
    pub binary: String,
    /// Selected model ID (e.g. `"nemotron-3-super:cloud"`).
    /// The CLI reads this from its own TOML config; this field is for UI introspection.
    #[serde(default)]
    pub model: Option<String>,
}

fn default_lightarchitects_cli_binary() -> String {
    "lightarchitects-cli".to_owned()
}

impl Default for LightarchitectsNativeConfig {
    fn default() -> Self {
        Self {
            binary: default_lightarchitects_cli_binary(),
            model: None,
        }
    }
}

/// Complete agent session — which CLI to spawn + how it routes its LLM calls.
///
/// The outer discriminator is the [`AgentKind`]; each kind carries its own
/// backend enum, making invalid combinations (e.g. Claude + OpenAI-only backend)
/// unrepresentable at the type level.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "agent", rename_all = "snake_case")]
pub enum AgentSession {
    /// Claude Code CLI (`claude` binary) with a Claude-specific backend.
    Lightarchitects(ClaudeBackend),
    /// `OpenAI` Codex CLI (`codex` binary).
    Codex(CodexConfig),
    /// lÆx0 native binary (`lightarchitects-cli`).
    LightarchitectsNative(LightarchitectsNativeConfig),
}

impl Default for AgentSession {
    fn default() -> Self {
        Self::Lightarchitects(ClaudeBackend::default())
    }
}

impl AgentSession {
    /// Return the agent kind (for display / serialization).
    #[must_use]
    pub fn kind(&self) -> AgentKind {
        match self {
            Self::Lightarchitects(_) => AgentKind::Lightarchitects,
            Self::Codex(_) => AgentKind::Codex,
            Self::LightarchitectsNative(_) => AgentKind::LightarchitectsNative,
        }
    }
}

/// Command-line arguments parsed via `clap::Parser`.
#[derive(Debug, Parser)]
#[command(
    name = "lightarchitects-webshell",
    version,
    about = "Local web GUI for the active coding agent",
    long_about = None
)]
pub struct Cli {
    /// TCP port to bind the webshell HTTP server to.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,

    /// Command to spawn inside the embedded PTY terminal.
    #[arg(long, default_value = DEFAULT_HOST_CMD)]
    pub host_cmd: OsString,

    /// Working directory for the spawned host command. Defaults to cwd.
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Agent CLI to spawn (`l-aex0` = claude, `codex` = openai codex).
    #[arg(long, value_enum, default_value_t = AgentKind::Lightarchitects)]
    pub agent: AgentKind,

    /// Backend for the selected agent.
    /// - `anthropic`: Claude Code OAuth/API-key auth (default for l-aex0).
    /// - `ollama-launch`: persistent subprocess via Ollama (all agents).
    /// - `ollama`: stateless HTTP to Ollama Anthropic-compat endpoint (l-aex0 only).
    #[arg(long, value_enum, default_value_t = ClaudeBackendKind::Anthropic)]
    pub backend: ClaudeBackendKind,

    /// Ollama base URL (applies to `--backend=ollama` and `--backend=ollama-launch`).
    /// Default for ollama-launch: `http://127.0.0.1:11434`.
    #[arg(long)]
    pub ollama_base_url: Option<String>,

    /// Model name for Ollama backends or Codex (`--backend=ollama`, `--backend=ollama-launch`,
    /// or `--agent=codex`). Cloud models: append `:cloud` suffix (e.g. `glm-5:cloud`).
    #[arg(long)]
    pub ollama_model: Option<String>,

    /// Auth token override for the stateless Ollama HTTP backend (`--backend=ollama`).
    /// Redacted in all logs.
    #[arg(long)]
    pub ollama_key: Option<String>,

    /// Name of a Claude Code agent template to launch (e.g., `engineer`, `quality`,
    /// `security`, `ops`, `researcher`). Maps to `claude --agent <name>` on spawn.
    /// Only applies when `--agent=claude-code` (currently the only option).
    ///
    /// Singleton templates are registered in Claude's agent registry
    /// (`~/.claude/agents/` or via `--agents` JSON). The webshell does not
    /// register them; it only passes the name through.
    #[arg(long)]
    pub claude_agent: Option<String>,

    /// Path to the `lightarchitects-cli` binary (default: `lightarchitects-cli`, assumes on `$PATH`).
    /// Only used when `--agent=lightarchitects-native`.
    #[arg(long)]
    pub lightarchitects_cli_binary: Option<String>,
}

impl Default for Cli {
    /// Default CLI instance — all fields at their default values.
    ///
    /// Primarily used by integration tests that construct a `Cli` by hand
    /// and want struct-update syntax (`Cli { port: 9000, ..Default::default() }`).
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            host_cmd: OsString::from(DEFAULT_HOST_CMD),
            cwd: None,
            agent: AgentKind::Lightarchitects,
            backend: ClaudeBackendKind::Anthropic,
            ollama_base_url: None,
            ollama_model: None,
            ollama_key: None,
            claude_agent: None,
            lightarchitects_cli_binary: None,
        }
    }
}

/// Resolved webshell configuration with the HMAC token attached.
#[derive(Debug, Clone)]
pub struct Config {
    /// TCP port the HTTP server binds to.
    pub port: u16,
    /// Host command spawned inside the PTY terminal.
    pub host_cmd: OsString,
    /// Working directory used when spawning the host command.
    pub cwd: PathBuf,
    /// Bearer token — sourced from env var, keyring, or auto-generated.
    pub token: String,
    /// Source the token was loaded from (for display in startup banner).
    pub token_source: TokenSource,
    /// Default agent session (kind + backend) for new build sessions.
    pub agent: AgentSession,
    /// Optional Claude Code agent template name (e.g., `engineer`, `quality`, `security`).
    ///
    /// Applied as `claude --agent <name>` only when `agent` is `Lightarchitects`.
    /// Codex sessions ignore this field (Phase 2 will introduce a
    /// `codex_agent_template` parallel field if that direction makes sense).
    ///
    /// Per-build `POST /api/builds` requests may override this; the `Config`
    /// value is just the default for builds that don't specify.
    pub claude_agent_template: Option<String>,
    /// Container mode override (env var `LA_CONTAINER_MODE`).
    pub container_mode: crate::container::ContainerMode,
}

/// Where the auth token was resolved from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    /// `LIGHTARCHITECTS_WEBSHELL_TOKEN` env var.
    EnvVar,
    /// OS keyring (macOS Keychain / Linux Secret Service).
    Keyring,
    /// `~/.lightarchitects/webshell/.token` file.
    File,
    /// Ephemeral — generated but not persisted.
    Ephemeral,
}

/// Errors that can surface while resolving configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The current working directory could not be determined.
    #[error("could not resolve current working directory: {0}")]
    InvalidCwd(#[source] std::io::Error),
}

/// Returns the canonical token file path: `~/.lightarchitects/webshell/.token`.
fn token_file_path() -> Option<PathBuf> {
    lightarchitects::core::paths::root().map(|root| root.join("webshell").join(".token"))
}

/// Returns the canonical Ollama config path: `~/.lightarchitects/webshell/ollama.json`.
fn ollama_config_path() -> Option<PathBuf> {
    lightarchitects::core::paths::root().map(|root| root.join("webshell").join("ollama.json"))
}

/// Returns the canonical lightarchitects-cli config path:
/// `~/lightarchitects/soul/config/lightarchitects-cli.toml`.
pub(crate) fn lightarchitects_cli_config_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|h| {
            h.join("lightarchitects")
                .join("soul")
                .join("config")
                .join("lightarchitects-cli.toml")
        })
}

/// Load Ollama config from disk, or return `None` if the file is missing/unreadable.
fn load_ollama_config() -> Option<OllamaConfig> {
    let path = ollama_config_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str::<OllamaConfig>(&content).ok()
}

/// Persist Ollama config to disk with 0600 permissions.
fn save_ollama_config(cfg: &OllamaConfig) -> Result<(), std::io::Error> {
    let Some(path) = ollama_config_path() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "home directory unavailable",
        ));
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Resolve the effective [`ClaudeBackend`] from CLI flags + stored Ollama config.
///
/// `OllamaLaunch` builds an [`OllamaLaunchConfig`] from `--ollama-model` +
/// `--ollama-base-url`, replicating the env vars injected by `ollama launch claude`.
///
/// `Ollama` (stateless HTTP) loads/persists `ollama.json` and overlays CLI overrides.
fn resolve_claude_backend(cli: &Cli) -> ClaudeBackend {
    match cli.backend {
        ClaudeBackendKind::Anthropic => ClaudeBackend::Anthropic,
        ClaudeBackendKind::OllamaLaunch => ClaudeBackend::OllamaLaunch(OllamaLaunchConfig {
            model: cli
                .ollama_model
                .clone()
                .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.to_owned()),
            base_url: cli
                .ollama_base_url
                .clone()
                .unwrap_or_else(default_ollama_launch_base_url),
        }),
        ClaudeBackendKind::Ollama => {
            let mut cfg = load_ollama_config().unwrap_or_default();
            let mut modified = false;
            if let Some(url) = cli.ollama_base_url.clone() {
                cfg.base_url = url;
                modified = true;
            }
            if let Some(model) = cli.ollama_model.clone() {
                cfg.model = model;
                modified = true;
            }
            if let Some(key) = cli.ollama_key.clone() {
                cfg.auth_token = key;
                modified = true;
            }
            if modified {
                if let Err(e) = save_ollama_config(&cfg) {
                    tracing::warn!(target: "webshell", "Failed to persist ollama.json: {e}");
                } else {
                    tracing::info!(target: "webshell", "Ollama config persisted to disk");
                }
            }
            ClaudeBackend::Ollama(cfg)
        }
    }
}

/// Resolve the effective [`AgentSession`] from CLI flags.
fn resolve_agent_session(cli: &Cli) -> AgentSession {
    match cli.agent {
        AgentKind::Lightarchitects => AgentSession::Lightarchitects(resolve_claude_backend(cli)),
        AgentKind::Codex => AgentSession::Codex(resolve_codex_config(cli)),
        AgentKind::LightarchitectsNative => {
            AgentSession::LightarchitectsNative(resolve_lightarchitects_cli_native_config(cli))
        }
    }
}

/// Build [`LightarchitectsNativeConfig`] from CLI flags.
fn resolve_lightarchitects_cli_native_config(cli: &Cli) -> LightarchitectsNativeConfig {
    LightarchitectsNativeConfig {
        binary: cli
            .lightarchitects_cli_binary
            .clone()
            .unwrap_or_else(default_lightarchitects_cli_binary),
        model: None,
    }
}

/// Build [`CodexConfig`] from CLI flags.
///
/// `--ollama-model` sets the model for all backends; `--backend=ollama-launch`
/// routes through a local Ollama daemon; any other backend defaults to `OpenAI`.
fn resolve_codex_config(cli: &Cli) -> CodexConfig {
    let model = cli.ollama_model.clone().unwrap_or_else(default_codex_model);
    let backend = match cli.backend {
        ClaudeBackendKind::OllamaLaunch => CodexBackend::OllamaLaunch(OllamaLaunchConfig {
            model: model.clone(),
            base_url: cli
                .ollama_base_url
                .clone()
                .unwrap_or_else(default_ollama_launch_base_url),
        }),
        _ => CodexBackend::OpenAi,
    };
    CodexConfig { model, backend }
}

/// Generates a random token using UUID v4.
fn generate_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Attempts to read the token from the OS keyring.
fn load_keyring_token() -> Option<String> {
    let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME) else {
        return None;
    };
    let Ok(token) = entry.get_password() else {
        return None;
    };
    if token.is_empty() {
        return None;
    }
    tracing::info!(target: "webshell", "Token loaded from OS keyring");
    Some(token)
}

/// Persists a token to the OS keyring. Silently skips on failure.
fn save_keyring_token(token: &str) -> bool {
    let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME) else {
        return false;
    };
    match entry.set_password(token) {
        Ok(()) => {
            tracing::info!(target: "webshell", "Token persisted to OS keyring");
            true
        }
        Err(e) => {
            tracing::warn!(target: "webshell", "Failed to persist token to keyring: {e}");
            false
        }
    }
}

/// Removes the persisted auth token from both the token file and the OS keyring.
///
/// Called by `DELETE /api/auth/session`. Best-effort — logs failures but does
/// not propagate errors so a partially-cleared state still returns 204.
pub(crate) fn remove_persisted_token() {
    // Remove token file.
    if let Some(path) = token_file_path() {
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!(target: "webshell", "Failed to remove token file: {e}");
            } else {
                tracing::info!(target: "webshell", "Token file removed: {}", path.display());
            }
        }
    }
    // Remove keyring entry (silently ignore "not found").
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME) {
        match entry.delete_credential() {
            Ok(()) => tracing::info!(target: "webshell", "Keyring token entry removed"),
            Err(keyring::Error::NoEntry) => {} // already gone
            Err(e) => tracing::warn!(target: "webshell", "Failed to remove keyring entry: {e}"),
        }
    }
}

/// Reads an existing token from the file, or generates and persists a new one.
fn load_or_create_token(token_path: &PathBuf) -> Result<String, std::io::Error> {
    // Try to read existing token
    if let Ok(token) = std::fs::read_to_string(token_path) {
        let trimmed = token.trim().to_owned();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    // Generate new token and persist it
    let token = generate_token();
    if let Some(parent) = token_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(token_path, &token)?;

    // Set file permissions to 0600 (owner read/write only) on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(token_path, perms)?;
    }

    Ok(token)
}

/// Attempts to load or create a persisted token file.
/// Returns `None` if home directory is unavailable.
fn load_or_create_persisted_token() -> Option<(String, TokenSource)> {
    let path = token_file_path()?;
    match load_or_create_token(&path) {
        Ok(t) => {
            tracing::info!(target: "webshell", "Token loaded from {}", path.display());
            Some((t, TokenSource::File))
        }
        Err(e) => {
            tracing::warn!(
                target: "webshell",
                "Failed to load/create token file: {e} — generating ephemeral token"
            );
            Some((generate_token(), TokenSource::Ephemeral))
        }
    }
}

/// Persisted setup config — written by the GUI setup flow, read at startup.
///
/// Stored at `~/.lightarchitects/webshell/setup.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupConfig {
    /// Selected agent binary.
    pub agent: AgentKind,
    /// Backend identifier (`"anthropic"`, `"ollama-launch"`, `"ollama"`, `"openai"`).
    pub backend: String,
    /// Selected model name, if overriding the backend default.
    pub model: Option<String>,
    /// Ollama base URL (only set for `ollama` / `ollama-launch` backends).
    pub ollama_base_url: Option<String>,
    /// `true` when an API key is stored in the OS keyring (key NOT stored here).
    pub api_key_stored: bool,
}

/// Returns the canonical setup config path: `~/.lightarchitects/webshell/setup.json`.
fn setup_config_path() -> Option<std::path::PathBuf> {
    lightarchitects::core::paths::root().map(|root| root.join("webshell").join("setup.json"))
}

impl Config {
    /// Resolves configuration from CLI args, env vars, keyring, and token file.
    ///
    /// Token resolution order:
    ///   1. `LIGHTARCHITECTS_WEBSHELL_TOKEN` env var
    ///   2. OS keyring (macOS Keychain / Linux Secret Service)
    ///   3. `~/.lightarchitects/webshell/.token` (auto-generated on first run)
    ///   4. Fresh random token (ephemeral, persisted to keyring + file on best-effort)
    ///
    /// # Errors
    ///
    /// - [`ConfigError::InvalidCwd`] if no `--cwd` was provided and the
    ///   current working directory cannot be read.
    pub fn resolve(cli: Cli) -> Result<Self, ConfigError> {
        Self::resolve_with_token(cli, None)
    }

    /// Resolves configuration with an optional explicit token override.
    ///
    /// When `token_override` is `Some`, it takes priority over env var,
    /// keyring, and file — treated as [`TokenSource::EnvVar`] since it is
    /// an explicit programmatic override (same semantics as setting the
    /// env var, but without the `unsafe` env manipulation).
    ///
    /// This is the primary entry point for integration tests that need to
    /// inject a deterministic token.
    ///
    /// # Errors
    ///
    /// - [`ConfigError::InvalidCwd`] if no `--cwd` was provided and the
    ///   current working directory cannot be read.
    pub fn resolve_with_token(
        cli: Cli,
        token_override: Option<String>,
    ) -> Result<Self, ConfigError> {
        let (token, token_source) = token_override
            .filter(|t| !t.is_empty())
            .map(|t| (t, TokenSource::EnvVar))
            .or_else(|| {
                std::env::var(TOKEN_ENV)
                    .ok()
                    .filter(|t| !t.is_empty())
                    .map(|t| (t, TokenSource::EnvVar))
            })
            .or_else(|| load_keyring_token().map(|t| (t, TokenSource::Keyring)))
            .or_else(load_or_create_persisted_token)
            .unwrap_or_else(|| {
                let t = generate_token();
                // Best-effort: persist the ephemeral token so it survives restarts
                save_keyring_token(&t);
                if let Some(path) = token_file_path() {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(&path, &t);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ =
                            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                    }
                }
                (t, TokenSource::Ephemeral)
            });

        // If token came from file or ephemeral, try to promote it to keyring
        if token_source == TokenSource::File || token_source == TokenSource::Ephemeral {
            save_keyring_token(&token);
        }

        let cwd = match cli.cwd {
            Some(ref p) => p.clone(),
            None => std::env::current_dir().map_err(ConfigError::InvalidCwd)?,
        };

        let agent = resolve_agent_session(&cli);
        let claude_agent_template = cli.claude_agent.clone();
        let container_mode = crate::container::ContainerMode::from_env();

        Ok(Self {
            port: cli.port,
            host_cmd: cli.host_cmd,
            cwd,
            token,
            token_source,
            agent,
            claude_agent_template,
            container_mode,
        })
    }

    /// Loads the persisted setup config from disk.
    ///
    /// Returns `None` if the file does not exist, is unreadable, or fails to parse.
    #[must_use]
    pub fn load_setup() -> Option<SetupConfig> {
        let path = setup_config_path()?;
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str::<SetupConfig>(&content).ok()
    }

    /// Persists the setup config to disk with 0600 permissions.
    ///
    /// # Errors
    ///
    /// Returns an IO error if the directory cannot be created or the file cannot be written.
    pub fn save_setup(cfg: &SetupConfig) -> Result<(), std::io::Error> {
        let Some(path) = setup_config_path() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "home directory unavailable",
            ));
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(cfg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, json)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// Returns `true` if a valid setup config exists on disk.
    #[must_use]
    pub fn is_setup_complete() -> bool {
        Self::load_setup().is_some()
    }

    /// Deletes the persisted setup config from disk.
    ///
    /// # Errors
    ///
    /// Returns an IO error if the file exists but cannot be removed.
    pub fn delete_setup() -> Result<(), std::io::Error> {
        let Some(path) = setup_config_path() else {
            return Ok(());
        };
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn cli_with(port: u16) -> Cli {
        Cli {
            port,
            host_cmd: OsString::from("claude"),
            cwd: Some(PathBuf::from("/tmp")),
            agent: AgentKind::Lightarchitects,
            backend: ClaudeBackendKind::Anthropic,
            ollama_base_url: None,
            ollama_model: None,
            ollama_key: None,
            claude_agent: None,
            lightarchitects_cli_binary: None,
        }
    }

    #[test]
    fn resolve_produces_non_empty_token() {
        let cfg = Config::resolve(cli_with(8733)).unwrap();
        assert!(!cfg.token.is_empty(), "token must not be empty");
        assert_eq!(cfg.port, 8733);
    }

    #[test]
    fn resolve_preserves_host_cmd_and_cwd() {
        let cli = Cli {
            port: 8733,
            host_cmd: OsString::from("/custom/lightarchitects-cli"),
            cwd: Some(PathBuf::from("/tmp/session")),
            agent: AgentKind::Lightarchitects,
            backend: ClaudeBackendKind::Anthropic,
            ollama_base_url: None,
            ollama_model: None,
            ollama_key: None,
            claude_agent: None,
            lightarchitects_cli_binary: None,
        };
        let cfg = Config::resolve(cli).unwrap();
        assert_eq!(cfg.host_cmd, OsString::from("/custom/lightarchitects-cli"));
        assert_eq!(cfg.cwd, PathBuf::from("/tmp/session"));
    }

    #[test]
    fn claude_agent_template_flows_cli_to_config() {
        let mut cli = cli_with(8733);
        cli.claude_agent = Some("corso".to_owned());
        let cfg = Config::resolve(cli).unwrap();
        assert_eq!(cfg.claude_agent_template.as_deref(), Some("corso"));
    }

    #[test]
    fn claude_agent_template_default_is_none() {
        let cfg = Config::resolve(cli_with(8733)).unwrap();
        assert!(cfg.claude_agent_template.is_none());
    }

    #[test]
    fn token_source_is_set() {
        let cfg = Config::resolve(cli_with(8733)).unwrap();
        // Token source must be one of the valid variants
        assert!(matches!(
            cfg.token_source,
            TokenSource::EnvVar | TokenSource::Keyring | TokenSource::File | TokenSource::Ephemeral
        ));
    }

    #[test]
    #[allow(clippy::panic)]
    fn default_agent_is_claude_code_with_anthropic() {
        let cfg = Config::resolve(cli_with(8733)).unwrap();
        assert_eq!(cfg.agent.kind(), AgentKind::Lightarchitects);
        let AgentSession::Lightarchitects(backend) = &cfg.agent else {
            panic!("expected Lightarchitects session");
        };
        assert_eq!(backend.kind(), ClaudeBackendKind::Anthropic);
    }

    #[test]
    fn ollama_default_config_values() {
        let oc = OllamaConfig::default();
        assert_eq!(oc.base_url, DEFAULT_OLLAMA_BASE_URL);
        assert_eq!(oc.model, DEFAULT_OLLAMA_MODEL);
        assert_eq!(oc.auth_token, "ollama");
    }

    #[test]
    fn ollama_launch_config_defaults() {
        let lc = OllamaLaunchConfig::default();
        // Must match what `ollama launch` actually injects (verified empirically).
        assert_eq!(lc.base_url, "http://127.0.0.1:11434");
        assert_eq!(lc.model, DEFAULT_OLLAMA_MODEL);
    }

    #[test]
    fn claude_backend_kind_mirrors_variant() {
        assert_eq!(
            ClaudeBackend::Anthropic.kind(),
            ClaudeBackendKind::Anthropic
        );
        assert_eq!(
            ClaudeBackend::OllamaLaunch(OllamaLaunchConfig::default()).kind(),
            ClaudeBackendKind::OllamaLaunch
        );
        assert_eq!(
            ClaudeBackend::Ollama(OllamaConfig::default()).kind(),
            ClaudeBackendKind::Ollama
        );
    }

    #[test]
    fn codex_config_defaults_to_openai_o3() {
        let cfg = CodexConfig::default();
        assert_eq!(cfg.model, "o3");
        assert_eq!(cfg.backend, CodexBackend::OpenAi);
    }

    #[test]
    fn codex_ollama_launch_config_round_trips() {
        let cfg = CodexConfig {
            model: "gpt-oss:120b-cloud".to_owned(),
            backend: CodexBackend::OllamaLaunch(OllamaLaunchConfig {
                model: "gpt-oss:120b-cloud".to_owned(),
                base_url: "http://127.0.0.1:11434".to_owned(),
            }),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: CodexConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model, cfg.model);
        assert_eq!(back.backend, cfg.backend);
    }

    #[test]
    fn agent_session_kind_mirrors_outer_variant() {
        assert_eq!(
            AgentSession::Lightarchitects(ClaudeBackend::Anthropic).kind(),
            AgentKind::Lightarchitects
        );
        assert_eq!(
            AgentSession::Codex(CodexConfig::default()).kind(),
            AgentKind::Codex
        );
    }

    #[test]
    fn agent_session_serializes_with_outer_discriminator() {
        let sess = AgentSession::Lightarchitects(ClaudeBackend::Anthropic);
        let json = serde_json::to_string(&sess).unwrap();
        // Outer tag (agent) identifies the kind; inner tag (kind) identifies the backend.
        assert!(
            json.contains(r#""agent":"lightarchitects""#),
            "outer agent tag missing: {json}"
        );
        assert!(
            json.contains(r#""kind":"anthropic""#),
            "inner backend tag missing: {json}"
        );
    }

    #[test]
    fn lightarchitects_cli_native_config_defaults_to_lightarchitects_cli_binary() {
        let cfg = LightarchitectsNativeConfig::default();
        assert_eq!(cfg.binary, "lightarchitects-cli");
    }

    #[test]
    fn agent_session_native_kind_roundtrips() {
        let sess = AgentSession::LightarchitectsNative(LightarchitectsNativeConfig::default());
        assert_eq!(sess.kind(), AgentKind::LightarchitectsNative);
        let json = serde_json::to_string(&sess).unwrap();
        assert!(
            json.contains(r#""agent":"lightarchitects_native""#),
            "native agent tag missing: {json}"
        );
    }
}
