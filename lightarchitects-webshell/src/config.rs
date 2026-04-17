//! Webshell runtime configuration resolved from CLI args + environment.
//!
//! Token resolution order:
//!   1. `LIGHTARCHITECTS_WEBSHELL_TOKEN` env var (explicit override)
//!   2. OS keyring (macOS Keychain / Linux Secret Service)
//!   3. `~/.lightarchitects/webshell/.token` (auto-generated on first run)
//!   4. If none exists: generate a random UUID token and persist to keyring + file.

use std::{ffi::OsString, path::PathBuf};

use clap::Parser;

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
    lightarchitects_core::paths::root().map(|root| root.join("webshell").join(".token"))
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
            Some(p) => p,
            None => std::env::current_dir().map_err(ConfigError::InvalidCwd)?,
        };

        Ok(Self {
            port: cli.port,
            host_cmd: cli.host_cmd,
            cwd,
            token,
            token_source,
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
        };
        let cfg = Config::resolve(cli).unwrap();
        assert_eq!(cfg.host_cmd, OsString::from("/custom/laex0"));
        assert_eq!(cfg.cwd, PathBuf::from("/tmp/session"));
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
}
