//! Arena gateway configuration.
//!
//! Loads from environment variables with sensible defaults for local development.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret, SecretString};

/// Read a path from an env var, falling back to a default.
fn env_or_path(var: &str, default: &Path) -> PathBuf {
    std::env::var(var)
        .map(PathBuf::from)
        .unwrap_or_else(|_| default.to_path_buf())
}

/// Agent backend selection — controls how sibling agent processes are spawned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentBackendKind {
    /// Spawn as native OS processes (default, Khadas production).
    Native,
    /// Spawn as Docker containers named `larc-agent-{sibling}`.
    Docker,
    /// Spawn via macOS `sandbox-exec` with a minimal profile.
    Sandbox,
}

impl AgentBackendKind {
    /// Parse from the `ARENA_AGENT_BACKEND` env var.
    fn from_env() -> Self {
        match std::env::var("ARENA_AGENT_BACKEND")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "docker" => Self::Docker,
            "sandbox" => Self::Sandbox,
            _ => Self::Native,
        }
    }
}

/// Light Architects Arena gateway configuration.
///
/// Debug impl intentionally omitted — contains pepper path.
#[derive(Clone)]
pub struct Config {
    /// Listen address (default: 127.0.0.1:3800).
    pub listen_addr: SocketAddr,
    /// MCP binary paths for each sibling.
    pub siblings: SiblingBinaries,
    /// CORS allowed origins.
    pub cors_origins: Vec<String>,
    /// Database path for auth keys + session store.
    pub db_path: PathBuf,
    /// HMAC pepper for API key hashing (from `ARENA_PEPPER`).
    /// Stored as `SecretString` — zeroized on drop, never logged.
    pub pepper: Option<SecretString>,
    /// Rate limit window in seconds (default: 60).
    pub rate_limit_window_secs: u64,
    /// Default rate limit per key per window (default: 60).
    pub rate_limit_default: u32,
    /// Discord bot token for Gateway WebSocket presence.
    /// Stored as `SecretString` — zeroized on drop, never logged.
    pub discord_bot_token: Option<SecretString>,
    /// Telegram bot token for supervisor alerting.
    /// Stored as `SecretString` — zeroized on drop, never logged.
    pub telegram_bot_token: Option<SecretString>,
    /// Telegram chat ID for alerts (e.g., Squad Chat topic).
    pub telegram_chat_id: Option<String>,
    /// Consecutive failures before alerting (default: 3).
    pub alert_threshold: u32,
    /// Path to routines.json for scheduled dispatch.
    pub routines_path: PathBuf,
    /// Arena data directory (default: `~/.arena`).
    pub data_dir: PathBuf,
    /// Light Architects Genesis HuggingFace Inference Endpoint URL.
    /// Reads `EXODUS_ENDPOINT_URL` (preferred) or `LARC_ENDPOINT_URL` (legacy).
    pub exodus_endpoint_url: Option<String>,
    /// HuggingFace token for Exodus endpoint auth.
    /// Reads `EXODUS_HF_TOKEN` (preferred) or `LARC_HF_TOKEN` (legacy).
    /// Stored as `SecretString` — zeroized on drop, never logged.
    pub exodus_hf_token: Option<SecretString>,
    /// Agent backend — how sibling agents are spawned (native/docker/sandbox).
    pub agent_backend: AgentBackendKind,
    /// Docker image for DockerBackend (from `ARENA_DOCKER_IMAGE`).
    pub docker_image: String,
    /// Ollama base URL, overridable for container environments.
    pub ollama_host: String,
    /// Helix output directory for auto-conversation transcripts.
    pub helix_output_dir: PathBuf,
    /// Significance spike threshold for canon-evaluation trigger.
    pub significance_spike_threshold: f64,
}

/// Paths to MCP binary executables.
#[derive(Debug, Clone)]
pub struct SiblingBinaries {
    pub corso: PathBuf,
    pub eva: PathBuf,
    pub soul: PathBuf,
    pub quantum: PathBuf,
    pub seraph: PathBuf,
    /// Exodus (LÆX) model — placeholder, no binary yet (Ollama Cloud inference TBD).
    pub exodus: Option<PathBuf>,
}

/// Valid sibling names — used for input validation.
pub const VALID_SIBLINGS: &[&str] = &["corso", "eva", "soul", "quantum", "seraph", "exodus"];

impl SiblingBinaries {
    /// Resolve binary paths — env vars override, then fall back to home defaults.
    ///
    /// Container-friendly: set `CORSO_BIN`, `EVA_BIN`, etc. to override.
    fn from_home() -> Result<Self, Box<dyn std::error::Error>> {
        let home = dirs_next::home_dir()
            .ok_or("Cannot determine home directory — set sibling paths via env vars")?;
        Ok(Self {
            corso: env_or_path("CORSO_BIN", &home.join(".corso/bin/corso")),
            eva: env_or_path("EVA_BIN", &home.join(".eva/bin/eva")),
            soul: env_or_path("SOUL_BIN", &home.join(".soul/.config/bin/soul")),
            quantum: env_or_path("QUANTUM_BIN", &home.join(".quantum/bin/quantum-q")),
            seraph: env_or_path("SERAPH_BIN", &home.join(".seraph/bin/seraph")),
            exodus: std::env::var("EXODUS_BIN")
                .or_else(|_| std::env::var("LARC_BIN"))
                .ok()
                .map(PathBuf::from),
        })
    }
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Supports both legacy `IRONCLAW_*` and new `ARENA_*` env var prefixes.
    /// `ARENA_*` takes precedence when both are set.
    ///
    /// # Errors
    /// Returns error if address parsing fails or home directory is unavailable.
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let host = env_with_fallback("ARENA_HOST", "IRONCLAW_HOST", "127.0.0.1");
        let port = env_with_fallback("ARENA_PORT", "IRONCLAW_PORT", "3800").parse::<u16>()?;

        let listen_addr: SocketAddr = format!("{host}:{port}").parse()?;

        let cors_origins = env_with_fallback(
            "ARENA_CORS_ORIGINS",
            "IRONCLAW_CORS_ORIGINS",
            "https://lightarchitects.io,https://lightarchitects.ai,https://api.lightarchitects.ai",
        )
        .split(',')
        .map(|s| s.trim().to_owned())
        .collect();

        let db_path = std::env::var("ARENA_DB")
            .or_else(|_| std::env::var("IRONCLAW_DB"))
            .map_or_else(|_| PathBuf::from("./data/arena.db"), PathBuf::from);

        let pepper = std::env::var("ARENA_PEPPER")
            .or_else(|_| std::env::var("IRONCLAW_PEPPER"))
            .ok()
            .map(SecretString::from);

        let rate_limit_window_secs =
            env_with_fallback("ARENA_RATE_WINDOW", "IRONCLAW_RATE_WINDOW", "60")
                .parse()
                .unwrap_or(60);

        let rate_limit_default = env_with_fallback("ARENA_RATE_LIMIT", "IRONCLAW_RATE_LIMIT", "60")
            .parse()
            .unwrap_or(60);

        let discord_bot_token = std::env::var("DISCORD_BOT_TOKEN")
            .ok()
            .map(SecretString::from);
        let telegram_bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
            .ok()
            .map(SecretString::from);
        let telegram_chat_id = std::env::var("TELEGRAM_CHAT_ID").ok();

        let alert_threshold =
            env_with_fallback("ARENA_ALERT_THRESHOLD", "IRONCLAW_ALERT_THRESHOLD", "3")
                .parse()
                .unwrap_or(3);

        let home = dirs_next::home_dir().ok_or("Cannot determine home directory")?;
        let data_dir = std::env::var("ARENA_DATA_DIR")
            .or_else(|_| std::env::var("IRONCLAW_DATA_DIR"))
            .map_or_else(|_| home.join(".arena"), PathBuf::from);
        let routines_path = data_dir.join("routines.json");

        let helix_output_dir = std::env::var("ARENA_HELIX_OUTPUT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".soul/helix/chat/transcripts"));

        let ollama_host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "localhost:11434".into());

        let docker_image = std::env::var("ARENA_DOCKER_IMAGE")
            .unwrap_or_else(|_| "lightarchitects/gateway:latest".into());

        let significance_spike_threshold = std::env::var("ARENA_SPIKE_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8.0_f64);

        Ok(Self {
            listen_addr,
            siblings: SiblingBinaries::from_home()?,
            cors_origins,
            db_path,
            pepper,
            rate_limit_window_secs,
            rate_limit_default,
            discord_bot_token,
            telegram_bot_token,
            telegram_chat_id,
            alert_threshold,
            routines_path,
            data_dir,
            exodus_endpoint_url: std::env::var("EXODUS_ENDPOINT_URL")
                .or_else(|_| std::env::var("LARC_ENDPOINT_URL"))
                .ok(),
            exodus_hf_token: std::env::var("EXODUS_HF_TOKEN")
                .or_else(|_| std::env::var("LARC_HF_TOKEN"))
                .ok()
                .map(SecretString::from),
            agent_backend: AgentBackendKind::from_env(),
            docker_image,
            ollama_host,
            helix_output_dir,
            significance_spike_threshold,
        })
    }
}

/// Read an env var with fallback to a legacy name, then a default value.
fn env_with_fallback(primary: &str, legacy: &str, default: &str) -> String {
    std::env::var(primary)
        .or_else(|_| std::env::var(legacy))
        .unwrap_or_else(|_| default.into())
}

// Manual Debug to redact secrets
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("listen_addr", &self.listen_addr)
            .field("siblings", &self.siblings)
            .field("cors_origins", &self.cors_origins)
            .field("db_path", &self.db_path)
            .field("pepper", &self.pepper.as_ref().map(|_| "[REDACTED]"))
            .field("rate_limit_window_secs", &self.rate_limit_window_secs)
            .field("rate_limit_default", &self.rate_limit_default)
            .field(
                "discord_bot_token",
                &self.discord_bot_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "telegram_bot_token",
                &self.telegram_bot_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("telegram_chat_id", &self.telegram_chat_id)
            .field("alert_threshold", &self.alert_threshold)
            .field("routines_path", &self.routines_path)
            .field("data_dir", &self.data_dir)
            .field(
                "exodus_endpoint_url",
                &self.exodus_endpoint_url.as_ref().map(|u| {
                    let preview: String = u.chars().take(30).collect();
                    format!("{preview}...")
                }),
            )
            .field(
                "exodus_hf_token",
                &self.exodus_hf_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("agent_backend", &self.agent_backend)
            .field("docker_image", &self.docker_image)
            .field("ollama_host", &self.ollama_host)
            .field("helix_output_dir", &self.helix_output_dir)
            .field(
                "significance_spike_threshold",
                &self.significance_spike_threshold,
            )
            .finish()
    }
}
