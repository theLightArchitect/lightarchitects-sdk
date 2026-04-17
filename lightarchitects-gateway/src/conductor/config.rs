//! Conductor configuration.
//!
//! Config resolves from `~/.lightarchitects/conductor.toml`. All relative paths in the
//! config are resolved against `~/.lightarchitects/` as the base directory.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The conductor config directory under the user's home.
const CONFIG_DIR: &str = ".lightarchitects";

/// The conductor config file name.
const CONFIG_FILE: &str = "conductor.toml";

/// Legacy config file name (pre-renaming, loaded as fallback).
const LEGACY_CONFIG_FILE: &str = "lvl8.toml";

/// Resolve the config path, falling back to the legacy `lvl8.toml` name.
///
/// If `conductor.toml` exists, it is used. If not but `lvl8.toml` exists, a
/// deprecation warning is logged and the legacy path is returned. Otherwise
/// the new path is returned (even if absent — `load` handles the missing case).
fn resolve_config_path(base: &Path) -> PathBuf {
    let new_path = base.join(CONFIG_FILE);
    if new_path.exists() {
        return new_path;
    }
    let old_path = base.join(LEGACY_CONFIG_FILE);
    if old_path.exists() {
        tracing::warn!(
            new = %new_path.display(),
            old = %old_path.display(),
            "legacy config found — rename to conductor.toml"
        );
        return old_path;
    }
    new_path
}

/// Top-level conductor configuration, parsed from `conductor.toml`.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Loop behaviour.
    #[serde(default)]
    pub conductor: ConductorConfig,
    /// Per-task resource budgets.
    #[serde(default)]
    pub budgets: BudgetConfig,
    /// Filesystem paths (relative paths resolve against `base_dir`).
    #[serde(default)]
    pub paths: PathsConfig,
    /// Security constraints.
    #[serde(default)]
    pub security: SecurityConfig,

    /// Resolved base directory (`~/.lightarchitects/`). Not in TOML — set after load.
    #[serde(skip)]
    pub base_dir: PathBuf,
}

/// Conductor loop settings.
#[derive(Debug, Deserialize)]
pub struct ConductorConfig {
    /// Max concurrent tasks (1 = serial Ralph Loop).
    #[serde(default = "default_wip")]
    pub wip_limit: usize,
    /// Seconds between queue checks when idle.
    #[serde(default = "default_poll")]
    pub poll_interval_secs: u64,
    /// Run discovery scripts before each poll when queue is empty.
    #[serde(default = "default_true")]
    pub auto_discover: bool,
    /// Seconds between heartbeat writes.
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,
}

impl Default for ConductorConfig {
    fn default() -> Self {
        Self {
            wip_limit: 1,
            poll_interval_secs: 30,
            auto_discover: true,
            heartbeat_interval_secs: 60,
        }
    }
}

/// Per-task budgets and kill criteria.
#[derive(Debug, Deserialize)]
pub struct BudgetConfig {
    /// Max wall-clock seconds per task.
    #[serde(default = "default_wall_time")]
    pub max_wall_time_secs: u64,
    /// Max retry iterations before marking a task as failed.
    #[serde(default = "default_retries")]
    pub max_retries: u32,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_wall_time_secs: 1800,
            max_retries: 3,
        }
    }
}

/// Filesystem paths. Relative paths are resolved against the base directory.
#[derive(Debug, Deserialize)]
pub struct PathsConfig {
    /// Root directory for all Light Architects projects.
    #[serde(default = "default_projects_root")]
    pub projects_root: PathBuf,
    /// Task queue file.
    #[serde(default = "default_queue")]
    pub queue: PathBuf,
    /// Completed task archive directory.
    #[serde(default = "default_archive")]
    pub archive: PathBuf,
    /// Discovery script directory.
    #[serde(default = "default_discovery")]
    pub discovery: PathBuf,
    /// Log directory for task output.
    #[serde(default = "default_logs")]
    pub logs: PathBuf,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            projects_root: default_projects_root(),
            queue: default_queue(),
            archive: default_archive(),
            discovery: default_discovery(),
            logs: default_logs(),
        }
    }
}

/// Security constraints for autonomous execution.
#[derive(Debug, Deserialize, Default)]
pub struct SecurityConfig {
    /// Project directories the conductor is allowed to execute in.
    /// Empty list = unrestricted (NOT recommended).
    #[serde(default)]
    pub allowed_projects: Vec<String>,
}

impl SecurityConfig {
    /// Check whether a project path is allowed by the scope gate.
    ///
    /// Returns `true` if:
    /// - `allowed_projects` is empty (unrestricted mode), OR
    /// - the project matches any entry in `allowed_projects`
    #[must_use]
    pub fn is_project_allowed(&self, project: &str) -> bool {
        if self.allowed_projects.is_empty() {
            return true;
        }
        self.allowed_projects
            .iter()
            .any(|allowed| project == allowed || project.starts_with(&format!("{allowed}/")))
    }
}

impl Config {
    /// Resolve the config file from `~/.lightarchitects/conductor.toml`.
    ///
    /// Falls back to defaults if the file is absent. All relative paths in the
    /// config are resolved against `~/.lightarchitects/`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn resolve() -> Result<Self, ConfigError> {
        let base_dir = dirs_home().join(CONFIG_DIR);
        let config_path = resolve_config_path(&base_dir);
        let mut config = Self::load(&config_path)?;
        config.base_dir = base_dir;
        config.resolve_paths();
        Ok(config)
    }

    /// Load from an explicit path (for testing or override).
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self {
                conductor: ConductorConfig::default(),
                budgets: BudgetConfig::default(),
                paths: PathsConfig::default(),
                security: SecurityConfig::default(),
                base_dir: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            });
        }
        let content = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
        let mut config: Self = toml::from_str(&content).map_err(ConfigError::Parse)?;
        config.base_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        Ok(config)
    }

    /// Resolve relative paths against `base_dir`.
    fn resolve_paths(&mut self) {
        let base = &self.base_dir;
        resolve_relative(&mut self.paths.queue, base);
        resolve_relative(&mut self.paths.archive, base);
        resolve_relative(&mut self.paths.discovery, base);
        resolve_relative(&mut self.paths.logs, base);
    }

    /// PID file path for daemon management.
    #[must_use]
    pub fn pid_path(&self) -> PathBuf {
        self.base_dir.join("conductor.pid")
    }

    /// Heartbeat file path.
    #[must_use]
    pub fn heartbeat_path(&self) -> PathBuf {
        self.base_dir.join("conductor.heartbeat")
    }

    /// Metrics export path.
    #[must_use]
    pub fn metrics_path(&self) -> PathBuf {
        self.base_dir.join("conductor.metrics.json")
    }
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read the config file.
    #[error("failed to read config: {0}")]
    Io(std::io::Error),
    /// Failed to parse TOML.
    #[error("failed to parse config: {0}")]
    Parse(toml::de::Error),
}

// ── Path resolution ─────────────────────────────────────────────────────────

/// If the path is relative, resolve it against the base directory.
fn resolve_relative(path: &mut PathBuf, base: &Path) {
    if path.is_relative() {
        *path = base.join(&*path);
    }
}

// ── Defaults ─────────────────────────────────────────────────────────────────

fn default_wip() -> usize {
    1
}
fn default_poll() -> u64 {
    30
}
fn default_true() -> bool {
    true
}
fn default_wall_time() -> u64 {
    1800
}
fn default_retries() -> u32 {
    3
}
fn default_heartbeat_interval() -> u64 {
    60
}

fn default_projects_root() -> PathBuf {
    dirs_home().join("Projects")
}
fn default_queue() -> PathBuf {
    PathBuf::from("tasks/queue.json")
}
fn default_archive() -> PathBuf {
    PathBuf::from("tasks/completed")
}
fn default_discovery() -> PathBuf {
    PathBuf::from("discovery")
}
fn default_logs() -> PathBuf {
    PathBuf::from("logs")
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/Users/kft"), PathBuf::from)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn resolve_uses_home_based_path() {
        let config = Config::resolve();
        assert!(config.is_ok());
        let config = config.expect("test config");
        assert!(config.base_dir.ends_with(CONFIG_DIR));
    }

    #[test]
    fn relative_paths_resolved_against_base() {
        let mut config = Config {
            conductor: ConductorConfig::default(),
            budgets: BudgetConfig::default(),
            paths: PathsConfig {
                projects_root: PathBuf::from("/Users/kft/Projects"),
                queue: PathBuf::from("tasks/queue.json"),
                archive: PathBuf::from("tasks/completed"),
                discovery: PathBuf::from("discovery"),
                logs: PathBuf::from("logs"),
            },
            security: SecurityConfig::default(),
            base_dir: PathBuf::from("/tmp/test-base"),
        };
        config.resolve_paths();
        assert_eq!(
            config.paths.queue,
            PathBuf::from("/tmp/test-base/tasks/queue.json")
        );
        assert_eq!(config.paths.logs, PathBuf::from("/tmp/test-base/logs"));
    }

    #[test]
    fn absolute_paths_not_modified() {
        let mut config = Config {
            conductor: ConductorConfig::default(),
            budgets: BudgetConfig::default(),
            paths: PathsConfig {
                projects_root: PathBuf::from("/Users/kft/Projects"),
                queue: PathBuf::from("/absolute/queue.json"),
                archive: PathBuf::from("/absolute/completed"),
                discovery: PathBuf::from("/absolute/discovery"),
                logs: PathBuf::from("/absolute/logs"),
            },
            security: SecurityConfig::default(),
            base_dir: PathBuf::from("/tmp/test-base"),
        };
        config.resolve_paths();
        assert_eq!(config.paths.queue, PathBuf::from("/absolute/queue.json"));
    }

    #[test]
    fn allowed_projects_scope_gate() {
        let sec = SecurityConfig {
            allowed_projects: vec!["SOUL/SOUL-DEV".into(), "CORSO/MCP/CORSO-DEV".into()],
        };
        assert!(sec.is_project_allowed("SOUL/SOUL-DEV"));
        assert!(sec.is_project_allowed("SOUL/SOUL-DEV/subcrate"));
        assert!(sec.is_project_allowed("CORSO/MCP/CORSO-DEV"));
        assert!(!sec.is_project_allowed("EVIL/MALWARE"));
    }

    #[test]
    fn empty_allowed_projects_permits_all() {
        let sec = SecurityConfig::default();
        assert!(sec.is_project_allowed("anything"));
    }

    #[test]
    fn load_missing_file_returns_defaults() {
        let config = Config::load(Path::new("/nonexistent/conductor.toml"));
        assert!(config.is_ok());
        let config = config.expect("defaults");
        assert_eq!(config.conductor.wip_limit, 1);
        assert_eq!(config.budgets.max_wall_time_secs, 1800);
        assert_eq!(config.budgets.max_retries, 3);
    }

    #[test]
    fn load_valid_toml() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("conductor.toml");
        std::fs::write(
            &path,
            r#"
[conductor]
wip_limit = 3
poll_interval_secs = 10

[budgets]
max_wall_time_secs = 900

[security]
allowed_projects = ["SOUL/SOUL-DEV"]
"#,
        )
        .expect("write");

        let config = Config::load(&path).expect("load");
        assert_eq!(config.conductor.wip_limit, 3);
        assert_eq!(config.conductor.poll_interval_secs, 10);
        assert_eq!(config.budgets.max_wall_time_secs, 900);
        assert_eq!(config.security.allowed_projects.len(), 1);
    }

    #[test]
    fn pid_heartbeat_metrics_paths() {
        let config = Config {
            conductor: ConductorConfig::default(),
            budgets: BudgetConfig::default(),
            paths: PathsConfig::default(),
            security: SecurityConfig::default(),
            base_dir: PathBuf::from("/home/test/.lightarchitects"),
        };
        assert_eq!(
            config.pid_path(),
            PathBuf::from("/home/test/.lightarchitects/conductor.pid")
        );
        assert_eq!(
            config.heartbeat_path(),
            PathBuf::from("/home/test/.lightarchitects/conductor.heartbeat")
        );
        assert_eq!(
            config.metrics_path(),
            PathBuf::from("/home/test/.lightarchitects/conductor.metrics.json")
        );
    }

    #[test]
    fn allowed_projects_prefix_match() {
        let sec = SecurityConfig {
            allowed_projects: vec!["SOUL/SOUL-DEV".into()],
        };
        // Exact match
        assert!(sec.is_project_allowed("SOUL/SOUL-DEV"));
        // Subpath match
        assert!(sec.is_project_allowed("SOUL/SOUL-DEV/soul-mcp"));
        // Partial prefix should NOT match (no slash boundary)
        assert!(!sec.is_project_allowed("SOUL/SOUL-DEV-FORK"));
    }
}
