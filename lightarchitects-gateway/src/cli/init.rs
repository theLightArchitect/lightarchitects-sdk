//! `lightarchitects init` — interactive identity setup wizard.
//!
//! Resolves a canonical `user_id` from one of four providers and writes the
//! `[identity]` section to `~/.lightarchitects/config.toml`.
//!
//! # Flow
//!
//! 1. Detect existing config — prompt for overwrite if present.
//! 2. Enumerate available providers (GitHub, `git_config`, explicit, local).
//! 3. Let user pick a provider.
//! 4. Resolve the `user_id` from the chosen provider.
//! 5. Write `config.toml` with atomic rename + 0o600 permissions.
//! 6. Verify by reloading the config.

use std::path::PathBuf;

use inquire::{Select, Text};
use lightarchitects::auth::{IdentityConfig, IdentityProvider};

use crate::config::GatewayConfig;
use crate::error::GatewayError;

/// Options presented to the user in the provider-selection step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderOption {
    GitHub,
    GitConfig,
    Explicit,
    Local,
}

impl std::fmt::Display for ProviderOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitHub => write!(f, "GitHub (via gh CLI)"),
            Self::GitConfig => write!(f, "Git config email"),
            Self::Explicit => write!(f, "Explicit user ID"),
            Self::Local => write!(f, "Local / offline mode"),
        }
    }
}

/// Run the interactive identity wizard.
///
/// # Errors
///
/// Returns [`GatewayError`] on I/O failure or if the user aborts.
pub fn run(force: bool) -> Result<(), GatewayError> {
    let config_path = config_file_path()?;

    if config_path.exists() && !force {
        let existing = GatewayConfig::load_from(&config_path).unwrap_or_default();
        let current_id = &existing.identity.user_id;
        let msg = format!(
            "Identity already configured (user_id: {current_id}). \
             Re-initialize anyway?"
        );
        let confirm = inquire::Confirm::new(&msg)
            .with_default(false)
            .prompt()
            .map_err(|e| GatewayError::Internal(format!("prompt error: {e}")))?;
        if !confirm {
            println!("Aborted. Existing identity preserved.");
            return Ok(());
        }
    }

    let options = detect_provider_options();
    let choice = Select::new("Choose an identity provider:", options)
        .with_page_size(4)
        .prompt()
        .map_err(|e| GatewayError::Internal(format!("prompt error: {e}")))?;

    let provider = match choice {
        ProviderOption::GitHub => {
            let username = resolve_gh_username()
                .map_err(|e| GatewayError::Internal(format!("GitHub resolution failed: {e}")))?;
            IdentityProvider::GitHub { username }
        }
        ProviderOption::GitConfig => {
            let email = resolve_git_email().map_err(|e| {
                GatewayError::Internal(format!("git config resolution failed: {e}"))
            })?;
            IdentityProvider::GitConfig { email }
        }
        ProviderOption::Explicit => {
            let raw = Text::new("Enter your user ID:")
                .prompt()
                .map_err(|e| GatewayError::Internal(format!("prompt error: {e}")))?;
            IdentityProvider::Explicit {
                user_id: raw.trim().to_owned(),
            }
        }
        ProviderOption::Local => IdentityProvider::Local,
    };

    let identity = IdentityConfig::resolve(provider)
        .map_err(|e| GatewayError::Internal(format!("identity resolution failed: {e}")))?;

    write_identity_config(&config_path, &identity.user_id, &identity.provider)?;

    // Verify round-trip
    let reloaded = GatewayConfig::load_from(&config_path).unwrap_or_default();
    if reloaded.identity.user_id != identity.user_id {
        return Err(GatewayError::Internal(
            "Config round-trip mismatch: verify failed".into(),
        ));
    }

    println!("✓ Identity configured: user_id = {}", identity.user_id);
    println!("  Provider: {:?}", identity.provider);
    println!("  Config: {}", config_path.display());

    Ok(())
}

// ── Provider detection ──────────────────────────────────────────────────────

/// Return a list of available providers with live-status annotations.
fn detect_provider_options() -> Vec<ProviderOption> {
    let mut opts = vec![ProviderOption::Local];

    if resolve_git_email().is_ok() {
        opts.push(ProviderOption::GitConfig);
    }

    if resolve_gh_username().is_ok() {
        opts.push(ProviderOption::GitHub);
    }

    opts.push(ProviderOption::Explicit);
    opts
}

// ── Resolution helpers ───────────────────────────────────────────────────────

/// Try to get the GitHub username via `gh api /user`.
fn resolve_gh_username() -> Result<String, std::io::Error> {
    let output = std::process::Command::new("gh")
        .args(["api", "/user", "--jq", ".login"])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "gh exited with {}",
            output.status
        )));
    }
    let username = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if username.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "empty gh username",
        ));
    }
    Ok(username)
}

/// Try to get the global git email via `git config --global user.email`.
fn resolve_git_email() -> Result<String, std::io::Error> {
    let output = std::process::Command::new("git")
        .args(["config", "--global", "user.email"])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "git exited with {}",
            output.status
        )));
    }
    let email = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if email.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "empty git email",
        ));
    }
    Ok(email)
}

// ── Config I/O ───────────────────────────────────────────────────────────────

fn config_file_path() -> Result<PathBuf, GatewayError> {
    let home =
        dirs_next::home_dir().ok_or(GatewayError::Config(crate::error::ConfigError::NoHome))?;
    Ok(home.join(".lightarchitects").join("config.toml"))
}

/// Atomically write the `[identity]` section into `config.toml`.
///
/// Preserves all other sections by round-tripping through `toml::Value`.
/// Uses an advisory file lock to prevent TOCTOU from concurrent `lightarchitects init` runs.
fn write_identity_config(
    path: &std::path::Path,
    user_id: &str,
    provider: &IdentityProvider,
) -> Result<(), GatewayError> {
    use std::io::Write as _;

    // Advisory lock — prevents concurrent RMW from multiple init processes.
    let lock_path = path.with_extension("lock");
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(|e| GatewayError::Internal(format!("lock open error: {e}")))?;
    fs2::FileExt::lock_exclusive(&lock_file)
        .map_err(|e| GatewayError::Internal(format!("lock error: {e}")))?;
    let _guard = LockGuard(lock_file);

    // Read existing or start fresh
    let mut doc: toml::Value = if path.exists() {
        let text = std::fs::read_to_string(path)
            .map_err(|e| GatewayError::Internal(format!("read error: {e}")))?;
        toml::from_str(&text).unwrap_or_else(|_| toml::Value::Table(toml::map::Map::new()))
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let table = doc
        .as_table_mut()
        .ok_or_else(|| GatewayError::Internal("config.toml root is not a table".into()))?;

    // Preserve existing identity fields (vault_remote, github_keychain_handle).
    let mut id_table = table
        .get("identity")
        .and_then(toml::Value::as_table)
        .cloned()
        .unwrap_or_else(toml::map::Map::new);

    // Remove stale provider-specific keys so they don't leak across switches.
    id_table.retain(|k, _| k == "vault_remote" || k == "github_keychain_handle");

    // Serialize the provider enum into the table (flattens tag+fields).
    let provider_val = toml::Value::try_from(provider)
        .map_err(|e| GatewayError::Internal(format!("serialize error: {e}")))?;
    if let Some(pt) = provider_val.as_table() {
        for (k, v) in pt {
            id_table.insert(k.clone(), v.clone());
        }
    }
    id_table.insert("user_id".into(), toml::Value::String(user_id.to_owned()));
    table.insert("identity".into(), toml::Value::Table(id_table));

    let text = toml::to_string_pretty(table)
        .map_err(|e| GatewayError::Internal(format!("serialize error: {e}")))?;

    let tmp = path.with_extension("toml.tmp");
    {
        let mut file = std::fs::File::create(&tmp)
            .map_err(|e| GatewayError::Internal(format!("create error: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            file.set_permissions(perms)
                .map_err(|e| GatewayError::Internal(format!("chmod error: {e}")))?;
        }
        file.write_all(text.as_bytes())
            .map_err(|e| GatewayError::Internal(format!("write error: {e}")))?;
    }

    std::fs::rename(&tmp, path)
        .map_err(|e| GatewayError::Internal(format!("rename error: {e}")))?;

    Ok(())
}

// RAII guard: unlock advisory lock on drop.
struct LockGuard(std::fs::File);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}
