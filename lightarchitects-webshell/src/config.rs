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
///
/// Phase 1: `ClaudeCode` is the only variant. `Codex` (`OpenAI`) is reserved
/// for Phase 2 and will be added without refactoring existing call sites —
/// each agent kind has its own agent-specific backend enum, so invalid
/// combinations are unrepresentable at the type level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    /// Anthropic's Claude Code CLI (binary: `claude`).
    ClaudeCode,
    // Codex (OpenAI) — reserved for Phase 2.
}

impl Default for AgentKind {
    fn default() -> Self {
        Self::ClaudeCode
    }
}

/// Backend routing choice for the Claude Code agent.
///
/// Controls which LLM answers API calls from the `claude` CLI:
/// - `Anthropic` (default): no env overrides — Claude uses OAuth subscription
///   or `ANTHROPIC_API_KEY` and talks to `api.anthropic.com`.
/// - `Ollama`: injects `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` +
///   `ANTHROPIC_MODEL` before exec, pointing Claude at a local Ollama daemon
///   that exposes the Anthropic Messages API (cloud models accessed via
///   `:cloud` model-name suffix, e.g. `qwen3-coder:480b-cloud`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeBackendKind {
    /// Use Claude Code's built-in Anthropic auth (subscription or API key).
    Anthropic,
    /// Route Claude through a local Ollama daemon (Anthropic-compat endpoint).
    Ollama,
}

impl Default for ClaudeBackendKind {
    fn default() -> Self {
        Self::Anthropic
    }
}

/// Ollama routing configuration — loaded from disk + CLI, merged per-build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Anthropic-compat base URL (default: `http://localhost:11434`).
    pub base_url: String,
    /// Model name; cloud models use `:cloud` suffix (e.g. `qwen3-coder:480b-cloud`).
    pub model: String,
    /// Auth token. For local-only use, `"ollama"` is the documented placeholder.
    /// For cloud models this should be the user's Ollama Cloud API key.
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

/// Fully-resolved Claude backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClaudeBackend {
    /// Let Claude Code handle auth via its normal OAuth/API-key flow.
    Anthropic,
    /// Route Claude through a local Ollama daemon.
    Ollama(OllamaConfig),
}

impl Default for ClaudeBackend {
    fn default() -> Self {
        Self::Anthropic
    }
}

impl ClaudeBackend {
    /// Return the kind (for display / serialization) without the embedded config.
    #[must_use]
    pub const fn kind(&self) -> ClaudeBackendKind {
        match self {
            Self::Anthropic => ClaudeBackendKind::Anthropic,
            Self::Ollama(_) => ClaudeBackendKind::Ollama,
        }
    }
}

/// Complete agent session — which CLI to spawn + how it routes its LLM calls.
///
/// The outer discriminator is the [`AgentKind`]; each kind's variant carries
/// its own agent-specific backend enum. This structure makes invalid
/// combinations (e.g., Claude paired with an OpenAI-only backend) unrepresentable
/// and leaves a clean extension point for Phase 2's `Codex(CodexBackend)` variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "agent", rename_all = "snake_case")]
pub enum AgentSession {
    /// Claude Code CLI with a Claude-specific backend.
    ClaudeCode(ClaudeBackend),
    // Codex(CodexBackend) — reserved for Phase 2.
}

impl Default for AgentSession {
    fn default() -> Self {
        Self::ClaudeCode(ClaudeBackend::default())
    }
}

impl AgentSession {
    /// Return the agent kind (for display / serialization).
    #[must_use]
    pub const fn kind(&self) -> AgentKind {
        match self {
            Self::ClaudeCode(_) => AgentKind::ClaudeCode,
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

    /// Which agent CLI to spawn. Phase 1: only `claude-code` is wired.
    /// New per-build sessions can override this via `POST /api/builds`.
    #[arg(long, value_enum, default_value_t = AgentKind::ClaudeCode)]
    pub agent: AgentKind,

    /// Default Claude backend applied when a build does not specify its own
    /// (ignored unless `--agent=claude-code`, which is currently the only option).
    #[arg(long, value_enum, default_value_t = ClaudeBackendKind::Anthropic)]
    pub backend: ClaudeBackendKind,

    /// Override Ollama's base URL (only applies when --backend=ollama).
    #[arg(long)]
    pub ollama_base_url: Option<String>,

    /// Override the Ollama model (only applies when --backend=ollama).
    #[arg(long)]
    pub ollama_model: Option<String>,

    /// Override the Ollama auth token (only applies when --backend=ollama).
    /// Redacted in all logs.
    #[arg(long)]
    pub ollama_key: Option<String>,

    /// Name of a Claude Code agent template to launch (e.g., `corso`, `eva`,
    /// `soul`, `quantum`, `seraph`). Maps to `claude --agent <name>` on spawn.
    /// Only applies when `--agent=claude-code` (currently the only option).
    ///
    /// Singleton templates are registered in Claude's agent registry
    /// (`~/.claude/agents/` or via `--agents` JSON). The webshell does not
    /// register them; it only passes the name through.
    #[arg(long)]
    pub claude_agent: Option<String>,
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
            agent: AgentKind::ClaudeCode,
            backend: ClaudeBackendKind::Anthropic,
            ollama_base_url: None,
            ollama_model: None,
            ollama_key: None,
            claude_agent: None,
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
    /// Optional Claude Code agent template name (e.g., `corso`, `eva`, `soul`).
    ///
    /// Applied as `claude --agent <name>` only when `agent` is `ClaudeCode`.
    /// Codex sessions ignore this field (Phase 2 will introduce a
    /// `codex_agent_template` parallel field if that direction makes sense).
    ///
    /// Per-build `POST /api/builds` requests may override this; the `Config`
    /// value is just the default for builds that don't specify.
    pub claude_agent_template: Option<String>,
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
/// When `--backend=ollama`, loads any stored `ollama.json`, overlays CLI
/// overrides, and persists back if the CLI provided any override — so a
/// user who once ran `--ollama-key=xxx --backend=ollama` gets the key
/// remembered for subsequent plain `--backend=ollama` runs.
fn resolve_claude_backend(cli: &Cli) -> ClaudeBackend {
    match cli.backend {
        ClaudeBackendKind::Anthropic => ClaudeBackend::Anthropic,
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

/// Resolve the effective [`AgentSession`] (kind + backend) from CLI flags.
///
/// Phase 1 wires only `AgentKind::ClaudeCode`; when Codex lands in Phase 2,
/// match on `cli.agent` here and select the appropriate backend resolver.
fn resolve_agent_session(cli: &Cli) -> AgentSession {
    match cli.agent {
        AgentKind::ClaudeCode => AgentSession::ClaudeCode(resolve_claude_backend(cli)),
    }
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

        Ok(Self {
            port: cli.port,
            host_cmd: cli.host_cmd,
            cwd,
            token,
            token_source,
            agent,
            claude_agent_template,
        })
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
            agent: AgentKind::ClaudeCode,
            backend: ClaudeBackendKind::Anthropic,
            ollama_base_url: None,
            ollama_model: None,
            ollama_key: None,
            claude_agent: None,
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
            host_cmd: OsString::from("/custom/laex0"),
            cwd: Some(PathBuf::from("/tmp/session")),
            agent: AgentKind::ClaudeCode,
            backend: ClaudeBackendKind::Anthropic,
            ollama_base_url: None,
            ollama_model: None,
            ollama_key: None,
            claude_agent: None,
        };
        let cfg = Config::resolve(cli).unwrap();
        assert_eq!(cfg.host_cmd, OsString::from("/custom/laex0"));
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
    fn default_agent_is_claude_code_with_anthropic() {
        let cfg = Config::resolve(cli_with(8733)).unwrap();
        assert_eq!(cfg.agent.kind(), AgentKind::ClaudeCode);
        let AgentSession::ClaudeCode(backend) = &cfg.agent;
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
    fn claude_backend_kind_mirrors_variant() {
        let anthropic: ClaudeBackend = ClaudeBackend::Anthropic;
        let ollama: ClaudeBackend = ClaudeBackend::Ollama(OllamaConfig::default());
        assert_eq!(anthropic.kind(), ClaudeBackendKind::Anthropic);
        assert_eq!(ollama.kind(), ClaudeBackendKind::Ollama);
    }

    #[test]
    fn agent_session_kind_mirrors_outer_variant() {
        let sess: AgentSession = AgentSession::ClaudeCode(ClaudeBackend::Anthropic);
        assert_eq!(sess.kind(), AgentKind::ClaudeCode);
    }

    #[test]
    fn agent_session_serializes_with_outer_discriminator() {
        let sess = AgentSession::ClaudeCode(ClaudeBackend::Anthropic);
        let json = serde_json::to_string(&sess).unwrap();
        // Outer tag (agent) identifies the kind; inner tag (kind) identifies the backend.
        assert!(
            json.contains(r#""agent":"claude_code""#),
            "outer agent tag missing: {json}"
        );
        assert!(
            json.contains(r#""kind":"anthropic""#),
            "inner backend tag missing: {json}"
        );
    }
}
