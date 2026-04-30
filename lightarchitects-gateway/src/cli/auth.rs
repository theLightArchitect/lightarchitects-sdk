//! `lightarchitects auth` — authentication subcommands for the gateway binary.
//!
//! # Subcommands
//!
//! | Subcommand | Action |
//! |------------|--------|
//! | `login`    | Browser-based PKCE flow; saves key to local file |
//! | `logout`   | Removes local API key and cached auth state |
//! | `status`   | Prints current key validity and cache state |
//!
//! # Usage
//!
//! ```text
//! lightarchitects auth login
//! lightarchitects auth logout
//! lightarchitects auth status
//! ```

use crate::error::GatewayError;
use lightarchitects::auth::{AuthConfig, AuthError, AuthGuard, KeyReader};

/// Execute an `auth` subcommand.
///
/// Parses the leading element of `args` as the subcommand name, then
/// dispatches to the appropriate handler.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] when no subcommand is given.
/// Propagates [`AuthError`] variants (mapped to [`GatewayError::File`]) for
/// login/logout/status failures.
pub async fn execute(args: &[String]) -> Result<(), GatewayError> {
    let auth_config = AuthConfig::default();
    match args.first().map(String::as_str) {
        Some("login") => {
            let pkce = !args.contains(&"--device".to_string());
            let device = args.contains(&"--device".to_string());
            cmd_login(pkce, device, &auth_config)
                .await
                .map_err(|e| GatewayError::File(format!("auth login: {e}")))?;
        }
        Some("logout") => {
            cmd_logout(&auth_config)
                .map_err(|e| GatewayError::File(format!("auth logout: {e}")))?;
        }
        Some("status") => {
            cmd_status(&auth_config).await;
        }
        Some(sub) => {
            eprintln!(
                "Unknown auth subcommand: {sub}\n\n\
                 Usage:\n  \
                   lightarchitects auth login    # browser PKCE flow\n  \
                   lightarchitects auth logout   # remove local credentials\n  \
                   lightarchitects auth status   # show current auth state"
            );
            return Err(GatewayError::UnknownTool(format!("auth {sub}")));
        }
        None => {
            eprintln!(
                "Usage:\n  \
                   lightarchitects auth login\n  \
                   lightarchitects auth logout\n  \
                   lightarchitects auth status"
            );
            return Err(GatewayError::MissingParam("auth subcommand"));
        }
    }
    Ok(())
}

/// Run the browser-based PKCE login flow.
///
/// `pkce` selects the standard PKCE flow (default). `device` selects RFC 8628
/// device-code flow — not yet implemented; emits an informational message.
///
/// # Errors
///
/// Returns [`AuthError::LoginFailed`] or [`AuthError::LoginTimeout`] on failure.
async fn cmd_login(pkce: bool, device: bool, config: &AuthConfig) -> Result<(), AuthError> {
    if device && !pkce {
        eprintln!("Device-code flow (RFC 8628) is not yet implemented.");
        eprintln!("Use `lightarchitects auth login` (standard PKCE) instead.");
        return Ok(());
    }
    match lightarchitects::auth::auth_login(config).await {
        Ok(key) => {
            let prefix_end = key.len().min(8);
            println!("Authenticated successfully.");
            println!("Key prefix: {}...", &key[..prefix_end]);
            println!("Saved to:   {}", config.key_file_path.display());
        }
        Err(e) => {
            eprintln!("Authentication failed: {e}");
            return Err(e);
        }
    }
    Ok(())
}

/// Remove local API key and cached auth state.
///
/// Does not revoke the key server-side — only clears local storage.
///
/// # Errors
///
/// Returns [`AuthError::Io`] if the key file cannot be removed.
fn cmd_logout(config: &AuthConfig) -> Result<(), AuthError> {
    AuthGuard::logout(config)?;
    println!("Logged out. Local credentials removed.");
    println!("Key file: {}", config.key_file_path.display());
    Ok(())
}

/// Print current authentication status.
///
/// Reads the key via [`KeyReader`] and reports validity tier. Timestamp
/// arithmetic for expiry display uses `checked_sub` to prevent underflow.
async fn cmd_status(config: &AuthConfig) {
    // Surface whether a key is loadable at all before delegating to full status.
    if KeyReader::read(config).is_err() {
        println!("No API key found. Run `lightarchitects auth login` to authenticate.");
        return;
    }
    AuthGuard::print_status(config).await;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn isolated_config(dir: &TempDir) -> AuthConfig {
        AuthConfig {
            key_file_path: dir.path().join("la-api-key"),
            cache_file_path: dir.path().join("la-key-cache.json"),
            revoked_file_path: dir.path().join("la-revoked"),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn execute_with_no_args_returns_missing_param() {
        let result = execute(&[]).await;
        assert!(matches!(result, Err(GatewayError::MissingParam(_))));
    }

    #[tokio::test]
    async fn execute_with_unknown_subcommand_returns_unknown_tool() {
        let result = execute(&["invalid".to_string()]).await;
        assert!(matches!(result, Err(GatewayError::UnknownTool(_))));
    }

    #[test]
    fn cmd_logout_with_no_key_file_is_noop() {
        let dir = TempDir::new().unwrap();
        let config = isolated_config(&dir);
        // No key file — logout should not error
        cmd_logout(&config).unwrap();
    }

    #[test]
    fn cmd_logout_removes_existing_key() {
        let dir = TempDir::new().unwrap();
        let config = isolated_config(&dir);
        std::fs::write(&config.key_file_path, "la-test-key-abc").unwrap();
        assert!(config.key_file_path.exists());
        cmd_logout(&config).unwrap();
        assert!(!config.key_file_path.exists());
    }

    #[tokio::test]
    async fn cmd_status_with_no_key_does_not_panic() {
        let dir = TempDir::new().unwrap();
        let config = isolated_config(&dir);
        // Must not panic — just prints "No API key found"
        cmd_status(&config).await;
    }

    #[tokio::test]
    async fn cmd_status_with_key_present_does_not_panic() {
        let dir = TempDir::new().unwrap();
        let config = isolated_config(&dir);
        std::fs::write(&config.key_file_path, "la-fake-key-for-status-test").unwrap();
        // Must not panic — calls AuthGuard::print_status which does async validation
        cmd_status(&config).await;
    }
}
