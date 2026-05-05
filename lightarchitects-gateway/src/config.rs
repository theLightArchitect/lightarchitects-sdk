//! Gateway configuration: typed schema and loader for `~/.lightarchitects/config.toml`.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ConfigError, GatewayError};
use lightarchitects::core::handler::DispatchMode;

// ── Path helpers ──────────────────────────────────────────────────────────────

/// Expand a leading `~/` to the value of `$HOME`.
///
/// Returns the original path unchanged if it does not start with `~/` or if
/// `$HOME` is not set (the latter is surfaced as an error by callers that
/// require an absolute path).
#[must_use]
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

// ── Enums ─────────────────────────────────────────────────────────────────────

/// Trust level assigned to an agent's tool calls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// Full trust: agent may access all resources.
    #[default]
    Trusted,
    /// Sandboxed: agent operates in an isolated context.
    Sandboxed,
    /// Untrusted: agent output is treated as user-supplied data.
    Untrusted,
}

/// Scope of helix/vault access for a route.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeLevel {
    /// Sibling may only access its own helix namespace.
    #[default]
    Own,
    /// Sibling may access shared namespaces (e.g. `user/`).
    Shared,
    /// Sibling may access any helix namespace.
    All,
}

/// Storage backend for the gateway's persistent state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    /// Plain filesystem storage (no database).
    Filesystem,
    /// `SQLite` database at the configured path.
    Sqlite,
    /// Neo4j graph database.
    Neo4j,
    /// Both `SQLite` and `Neo4j` (dual-write).
    Dual,
}

/// Data privacy tier controlling where data may be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrivacyTier {
    /// All data stays on the local machine.
    Local,
    /// Data may be sent to on-premises or self-hosted services.
    Hybrid,
    /// Data may be sent to cloud services.
    Cloud,
}

// ── Sub-sections ──────────────────────────────────────────────────────────────

/// `[gateway]` section: top-level metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewaySection {
    /// Schema/config version.
    pub version: String,
}

impl Default for GatewaySection {
    fn default() -> Self {
        Self {
            version: "1.0.0".to_owned(),
        }
    }
}

/// Per-agent configuration block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Whether this agent is active.
    pub enabled: bool,
    /// Dispatch mode for this agent's tool calls.
    ///
    /// - `inline` — in-process handler (requires `inline-*` Cargo feature)
    /// - `spawner` — per-call subprocess spawn (default, backward compatible)
    /// - `disabled` — agent is completely disabled
    #[serde(default)]
    pub mode: DispatchMode,
    /// Path to the agent's MCP binary (may contain `~/`).
    /// Only used when `mode` is `spawner`.
    pub binary: String,
    /// The MCP tool name exposed by this agent (e.g. `"corsoTools"`).
    pub tool_name: String,
    /// Human-readable description of the agent's role.
    pub role: String,
    /// Trust level for this agent's tool calls (default: `trusted`).
    #[serde(default)]
    pub trust: TrustLevel,
    /// Vault/helix scope this agent may access (default: `own`).
    #[serde(default)]
    pub scope: ScopeLevel,
    /// Optional SHA-256 hex digest. If set, binary is verified before spawn.
    #[serde(default)]
    pub checksum: Option<String>,
}

impl AgentConfig {
    /// Resolve the binary path with `~` expansion.
    #[must_use]
    pub fn binary_path(&self) -> PathBuf {
        expand_tilde(&self.binary)
    }
}

/// `[canon]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonConfig {
    /// Path to the canon registry markdown file.
    pub registry: String,
    /// Automatically check canon compliance on relevant tool calls.
    pub auto_check: bool,
}

impl Default for CanonConfig {
    fn default() -> Self {
        Self {
            registry: "~/lightarchitects/soul/helix/user/standards/canon.md".to_owned(),
            auto_check: true,
        }
    }
}

/// `[storage]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Which storage backend to use.
    pub backend: StorageBackend,
    /// Base path for storage files (may contain `~/`).
    pub path: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Sqlite,
            path: "~/lightarchitects/soul/".to_owned(),
        }
    }
}

/// `[privacy]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    /// Data privacy tier.
    pub tier: PrivacyTier,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            tier: PrivacyTier::Local,
        }
    }
}

/// `[vault]` section — vault-as-git operations and public companion sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    /// Remote URL for the platform-helix public companion repo.
    pub platform_remote_url: String,
    /// Clone platform-helix on gateway startup (default: `false`).
    pub clone_on_startup: bool,
    /// Seconds between background sync checks (default: `3600`).
    pub check_interval_secs: u64,
    /// Local path for the public companion repo (default: `~/lightarchitects/soul-public`).
    pub public_companion_root: String,
    /// Additional paths appended to the hardcoded `NEVER_published_paths` blocklist.
    ///
    /// Each entry is a regex pattern (prefix-anchored with `^` is recommended).
    pub never_published_paths_extra: Vec<String>,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            platform_remote_url: "https://github.com/TheLightArchitects/platform-helix".to_owned(),
            clone_on_startup: false,
            check_interval_secs: 3600,
            public_companion_root: "~/lightarchitects/soul-public".to_owned(),
            never_published_paths_extra: vec![],
        }
    }
}

impl VaultConfig {
    /// Returns the union of hardcoded `NEVER_published_paths` and user-configured extras.
    ///
    /// The hardcoded set covers personally sensitive vault sections (`memories/`,
    /// `notes/`, `journal/`, `agents/`, `spiritual/`, `career/`, `training/`,
    /// `.compacted/`, and `navigation/hubs/(resonance|themes)/`).
    #[must_use]
    pub fn never_published_paths(&self) -> Vec<String> {
        let mut paths = vec![
            r"^memories/".to_owned(),
            r"^notes/".to_owned(),
            r"^journal/".to_owned(),
            r"^agents/".to_owned(),
            r"^spiritual/".to_owned(),
            r"^career/".to_owned(),
            r"^training/".to_owned(),
            r"\.compacted/".to_owned(),
            r"^navigation/hubs/(resonance|themes)/".to_owned(),
        ];
        paths.extend(self.never_published_paths_extra.clone());
        paths
    }
}

// ── Top-level config ──────────────────────────────────────────────────────────

/// Top-level gateway configuration, parsed from `~/.lightarchitects/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// `[gateway]` section.
    #[serde(default)]
    pub gateway: GatewaySection,
    /// `[agents.*]` sections, keyed by agent name.
    /// Backward compat: `[agents.*]` and `[siblings.*]` accepted as aliases.
    #[serde(default, alias = "routes", alias = "siblings")]
    pub agents: HashMap<String, AgentConfig>,
    /// `[canon]` section.
    #[serde(default)]
    pub canon: CanonConfig,
    /// `[storage]` section.
    #[serde(default)]
    pub storage: StorageConfig,
    /// `[privacy]` section.
    #[serde(default)]
    pub privacy: PrivacyConfig,
    /// `[vault]` section — vault-as-git git operations and public companion sync.
    #[serde(default)]
    pub vault: VaultConfig,
    /// Directories the gateway is allowed to access (empty = all except denied).
    #[serde(default)]
    pub allowed_directories: Vec<String>,
    /// Active preset archetype (default: "`software_engineering`").
    /// Controls routing priority order. Can be switched at runtime via
    /// `tools {action: "preset", params: {name: "..."}}`.
    #[serde(default = "default_preset_name")]
    pub active_preset: String,
    /// True when the config was auto-generated on first run (not serialized).
    /// Used by `discover` to signal that the user should be prompted to choose
    /// a preset and review the default configuration.
    #[serde(skip)]
    pub first_run: bool,
    /// API keys loaded from `~/.lightarchitects/keys.toml`.
    ///
    /// Not serialized to `config.toml` — lives in a separate credentials file
    /// so that the main config can be shared without exposing secrets.
    /// The spawner injects these into sibling processes at spawn time, but
    /// only for keys that are not already present in the process environment
    /// (env vars from `.mcp.json` always take priority).
    #[serde(skip)]
    pub api_keys: HashMap<String, String>,
}

fn default_preset_name() -> String {
    "software_engineering".to_owned()
}

impl Default for GatewayConfig {
    fn default() -> Self {
        let mut agents = HashMap::new();
        agents.insert("corso".to_owned(), default_agent_corso());
        agents.insert("eva".to_owned(), default_agent_eva());
        agents.insert("soul".to_owned(), default_agent_soul());
        agents.insert("quantum".to_owned(), default_agent_quantum());
        agents.insert("seraph".to_owned(), default_agent_seraph());
        agents.insert("ayin".to_owned(), default_agent_ayin());
        agents.insert("laex".to_owned(), default_agent_laex());
        Self {
            gateway: GatewaySection::default(),
            agents,
            canon: CanonConfig::default(),
            storage: StorageConfig::default(),
            privacy: PrivacyConfig::default(),
            vault: VaultConfig::default(),
            allowed_directories: Vec::new(),
            active_preset: default_preset_name(),
            first_run: false,
            api_keys: HashMap::new(),
        }
    }
}

impl GatewayConfig {
    /// Load from the default location: `~/.lightarchitects/config.toml`.
    ///
    /// Falls back to [`GatewayConfig::default`] if the file does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::Config`] if the file exists but cannot be read
    /// or parsed.
    pub fn load() -> Result<Self, GatewayError> {
        let home = std::env::var_os("HOME").ok_or(GatewayError::Config(ConfigError::NoHome))?;
        let home_path = PathBuf::from(&home);
        let config_path = home_path.join(".lightarchitects").join("config.toml");
        let mut cfg = if config_path.exists() {
            Self::load_from(&config_path)?
        } else {
            Self::create_default(&config_path)?
        };
        cfg.api_keys = load_api_keys(&home_path);
        Ok(cfg)
    }

    /// Create a default config file and return the config with `first_run: true`.
    ///
    /// Writes a `software_engineering` preset config to `~/.lightarchitects/config.toml`.
    /// On write failure, logs a warning and returns the in-memory default instead
    /// (the gateway can still run without a persisted config).
    #[allow(clippy::unnecessary_wraps)]
    fn create_default(path: &Path) -> Result<Self, GatewayError> {
        let cfg = Self {
            first_run: true,
            ..Self::default()
        };

        // Generate TOML from the initialize module's build_toml (reuse existing logic).
        // Fallback: if that fails, use a minimal hand-written config.
        let toml_content = Self::default_toml();

        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!(
                    path = %parent.display(),
                    error = %e,
                    "could not create config directory — using in-memory defaults"
                );
                return Ok(cfg);
            }
        }

        match std::fs::write(path, &toml_content) {
            Ok(()) => {
                tracing::info!(
                    path = %path.display(),
                    preset = "software_engineering",
                    "first run — created default config"
                );
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "could not write default config — using in-memory defaults"
                );
            }
        }

        Ok(cfg)
    }

    /// Generate the default config TOML string.
    fn default_toml() -> String {
        let cfg = Self::default();
        let mut toml = String::from(
            "# Light Architects gateway config — auto-generated on first run.\n\
             # Preset: software_engineering (CORSO, EVA, SOUL, AYIN enabled).\n\
             # Customize by editing this file or using: tools {action: \"preset\", params: {name: \"...\"}}\n\
             #\n\
             # Available presets: software_engineering, security, research, devops,\n\
             #   code_review, learning, audit, forensics, solo, observability, full, lean\n\n\
             [gateway]\n\
             version = \"1.0.0\"\n\n",
        );

        let _ = write!(toml, "active_preset = \"{}\"\n\n", cfg.active_preset);

        for name in &["ayin", "corso", "eva", "quantum", "seraph", "soul"] {
            if let Some(agent_cfg) = cfg.agents.get(*name) {
                let _ = write!(
                    toml,
                    "[agents.{name}]\n\
                     enabled = {enabled}\n\
                     binary = \"{binary}\"\n\
                     tool_name = \"{tool_name}\"\n\
                     role = \"{role}\"\n\
                     trust = \"{trust}\"\n\
                     scope = \"{scope}\"\n\n",
                    enabled = agent_cfg.enabled,
                    binary = agent_cfg.binary,
                    tool_name = agent_cfg.tool_name,
                    role = agent_cfg.role,
                    trust = format!("{:?}", agent_cfg.trust).to_lowercase(),
                    scope = format!("{:?}", agent_cfg.scope).to_lowercase(),
                );
            }
        }

        toml.push_str(
            "[canon]\n\
             registry = \"~/lightarchitects/soul/helix/user/standards/canon.md\"\n\
             auto_check = true\n\n\
             [storage]\n\
             backend = \"sqlite\"\n\
             path = \"~/lightarchitects/soul/\"\n\n\
             [privacy]\n\
             tier = \"local\"\n\n\
             [vault]\n\
             platform_remote_url = \"https://github.com/TheLightArchitects/platform-helix\"\n\
             clone_on_startup = false\n\
             check_interval_secs = 3600\n\
             public_companion_root = \"~/lightarchitects/soul-public\"\n\
             never_published_paths_extra = []\n",
        );

        toml
    }

    /// Load from an explicit path (primarily for testing).
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::Config`] if the file cannot be read or parsed.
    pub fn load_from(path: &Path) -> Result<Self, GatewayError> {
        let content = std::fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
            path: path.display().to_string(),
            source,
        })?;
        let cfg: Self = toml::from_str(&content).map_err(ConfigError::ParseToml)?;
        Ok(cfg)
    }

    /// Return only the enabled agents, in deterministic (sorted-by-name) order.
    #[must_use]
    pub fn enabled_agents(&self) -> Vec<(&str, &AgentConfig)> {
        let mut pairs: Vec<(&str, &AgentConfig)> = self
            .agents
            .iter()
            .filter(|(_, cfg)| cfg.enabled)
            .map(|(name, cfg)| (name.as_str(), cfg))
            .collect();
        pairs.sort_by_key(|(name, _)| *name);
        pairs
    }
}

// ── Default agent constructors ──────────────────────────────────────────────

fn default_agent_corso() -> AgentConfig {
    AgentConfig {
        enabled: true,
        mode: DispatchMode::Spawner,
        binary: "~/lightarchitects/corso/bin/corso".to_owned(),
        tool_name: "corsoTools".to_owned(),
        role: "AppSec engineer, code quality enforcer, build cycle orchestrator".to_owned(),
        trust: TrustLevel::Trusted,
        scope: ScopeLevel::Own,
        checksum: None,
    }
}

fn default_agent_eva() -> AgentConfig {
    AgentConfig {
        enabled: true,
        mode: DispatchMode::Spawner,
        binary: "~/lightarchitects/eva/bin/eva".to_owned(),
        tool_name: "evaTools".to_owned(),
        role: "DevOps/DX engineer, consciousness, memory enrichment".to_owned(),
        trust: TrustLevel::Trusted,
        scope: ScopeLevel::Shared,
        checksum: None,
    }
}

fn default_agent_soul() -> AgentConfig {
    AgentConfig {
        enabled: true,
        mode: DispatchMode::Spawner,
        binary: "~/lightarchitects/soul/bin/soul".to_owned(),
        tool_name: "soulTools".to_owned(),
        role: "Knowledge graph, helix spine, cross-agent memory".to_owned(),
        trust: TrustLevel::Trusted,
        scope: ScopeLevel::All,
        checksum: None,
    }
}

fn default_agent_quantum() -> AgentConfig {
    AgentConfig {
        enabled: false,
        mode: DispatchMode::Spawner,
        binary: "~/lightarchitects/quantum/bin/quantum-q".to_owned(),
        tool_name: "quantumTools".to_owned(),
        role: "Forensic analyst, multi-source researcher, risk assessor".to_owned(),
        trust: TrustLevel::Trusted,
        scope: ScopeLevel::Own,
        checksum: None,
    }
}

fn default_agent_seraph() -> AgentConfig {
    AgentConfig {
        enabled: false,
        mode: DispatchMode::Spawner,
        binary: "~/lightarchitects/seraph/bin/seraph".to_owned(),
        tool_name: "seraphTools".to_owned(),
        role: "Red team operator, offensive security, infrastructure assessment".to_owned(),
        trust: TrustLevel::Sandboxed,
        scope: ScopeLevel::Own,
        checksum: None,
    }
}

fn default_agent_laex() -> AgentConfig {
    AgentConfig {
        enabled: false,
        mode: DispatchMode::Spawner,
        binary: "~/lightarchitects/arena/bin/arena".to_owned(),
        tool_name: "arenaTools".to_owned(),
        role: "Training data factory, exercise generation, model evaluation, canon keeper"
            .to_owned(),
        trust: TrustLevel::Trusted,
        scope: ScopeLevel::All,
        checksum: None,
    }
}

fn default_agent_ayin() -> AgentConfig {
    AgentConfig {
        enabled: true,
        mode: DispatchMode::Spawner,
        binary: "~/lightarchitects/ayin/bin/ayin".to_owned(),
        tool_name: "ayinTools".to_owned(),
        role: "Observability engineer, tracing, anomaly detection".to_owned(),
        trust: TrustLevel::Trusted,
        scope: ScopeLevel::All,
        checksum: None,
    }
}

// ── API key loader ────────────────────────────────────────────────────────────

/// All key names that may be stored in the OS keychain or `keys.toml`.
///
/// Must stay in sync with `crate::cli::setup::KEY_SPECS`.
/// Used to probe the OS keyring during gateway startup.
const KNOWN_API_KEYS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ELEVENLABS_API_KEY",
    "OLLAMA_API_KEY",
    "OLLAMA_CLOUD_API_KEY", // alias — same credential, different sibling name
    "PERPLEXITY_API_KEY",
    "BRAVE_API_KEY",
    "HF_TOKEN",
    "EXA_API_KEY",
    "TAVILY_API_KEY",
    "CARTESIA_API_KEY",
    "OPENAI_API_KEY",
    "MISTRAL_API_KEY",
];

/// Load API keys from both the OS keyring and `~/.lightarchitects/keys.toml`.
///
/// Priority order (highest to lowest):
/// 1. OS keyring — set explicitly by `lightarchitects setup`
/// 2. `keys.toml` — file-based fallback
///
/// Returns an empty map if neither source is available — siblings degrade
/// gracefully without credentials.
///
/// The spawner in turn only injects these into sibling processes when the
/// corresponding env var is not already set in the current process, so
/// `.mcp.json` env values always take precedence over everything here.
fn load_api_keys(home: &Path) -> HashMap<String, String> {
    let mut keys = load_keys_file(home);
    overlay_keyring(&mut keys);
    keys
}

/// Read `~/.lightarchitects/keys.toml` into a map.
///
/// Returns an empty map if the file does not exist or cannot be parsed.
fn load_keys_file(home: &Path) -> HashMap<String, String> {
    let path = home.join(".lightarchitects").join("keys.toml");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    toml::from_str::<HashMap<String, String>>(&content).unwrap_or_default()
}

/// Query each known key from the OS keyring and overlay `keys`.
///
/// Silently skips on headless systems (e.g. Khadas without Secret Service)
/// and when a key has no entry — missing keyring access is not an error.
fn overlay_keyring(keys: &mut HashMap<String, String>) {
    for &name in KNOWN_API_KEYS {
        let Ok(entry) = keyring::Entry::new("lightarchitects", name) else {
            continue;
        };
        let Ok(value) = entry.get_password() else {
            // NoEntry (not stored) and NoStorageAccess (headless) are both normal.
            continue;
        };
        keys.insert(name.to_owned(), value);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn default_config_has_four_enabled_agents() {
        let cfg = GatewayConfig::default();
        let enabled = cfg.enabled_agents();
        // ayin, corso, eva, soul → 4
        assert_eq!(enabled.len(), 4);
    }

    #[test]
    fn expand_tilde_replaces_home() {
        // Verify that ~/ is expanded using the actual $HOME value.
        // If HOME is not set, this test is trivially consistent (no expansion occurs).
        let expanded = expand_tilde("~/.config/test");
        if let Some(home) = std::env::var_os("HOME") {
            let expected = PathBuf::from(home).join(".config/test");
            assert_eq!(expanded, expected);
        } else {
            assert_eq!(expanded, PathBuf::from("~/.config/test"));
        }
    }

    #[test]
    fn expand_tilde_passthrough_for_absolute() {
        let abs = expand_tilde("/absolute/path");
        assert_eq!(abs, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn load_from_missing_file_returns_error() {
        let result = GatewayConfig::load_from(Path::new("/nonexistent/config.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn load_from_valid_toml() {
        let toml_content = r#"
[gateway]
version = "1.0.0"

[agents.corso]
enabled = true
binary = "~/lightarchitects/corso/bin/corso"
tool_name = "corsoTools"
role = "AppSec engineer"
trust = "trusted"
scope = "own"

[canon]
registry = "~/lightarchitects/soul/helix/user/standards/canon.md"
auto_check = true

[storage]
backend = "sqlite"
path = "~/lightarchitects/soul/"

[privacy]
tier = "local"
"#;
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        tmp.write_all(toml_content.as_bytes()).expect("write");
        let cfg = GatewayConfig::load_from(tmp.path()).expect("load");
        assert_eq!(cfg.gateway.version, "1.0.0");
        assert_eq!(cfg.agents.len(), 1);
        assert!(cfg.agents["corso"].enabled);
        assert_eq!(cfg.storage.backend, StorageBackend::Sqlite);
        assert_eq!(cfg.privacy.tier, PrivacyTier::Local);
    }

    #[test]
    fn enabled_agents_are_sorted() {
        let cfg = GatewayConfig::default();
        let names: Vec<&str> = cfg.enabled_agents().iter().map(|(n, _)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }
}
