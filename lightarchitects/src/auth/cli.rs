use crate::auth::{AuthConfig, auth_guard::AuthGuard, auth_login};
use clap::Subcommand;

/// Auth subcommands available to all MCP server binaries.
///
/// Usage: `{binary} auth login|status|logout`
#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Authenticate via browser (opens lightarchitects.io/auth/cli)
    Login,
    /// Show current authentication status
    Status,
    /// Remove local API key and cached state
    Logout,
}

impl AuthCommand {
    /// Execute the auth subcommand.
    ///
    /// # Errors
    ///
    /// Returns [`lightarchitects::auth::AuthError::LoginFailed`] or [`lightarchitects::auth::AuthError::LoginTimeout`] if `login` fails.
    /// Returns [`lightarchitects::auth::AuthError::Io`] if `logout` cannot remove local state.
    pub async fn run(&self, config: &AuthConfig) -> Result<(), crate::auth::AuthError> {
        match self {
            Self::Login => match auth_login(config).await {
                Ok(key) => {
                    let prefix = if key.len() >= 8 { &key[..8] } else { &key };
                    println!("Authenticated successfully!");
                    println!("Key prefix: {prefix}...");
                    println!("Saved to: {}", config.key_file_path.display());
                }
                Err(e) => {
                    eprintln!("Authentication failed: {e}");
                    return Err(e);
                }
            },
            Self::Status => {
                AuthGuard::print_status(config).await;
            }
            Self::Logout => {
                AuthGuard::logout(config)?;
            }
        }
        Ok(())
    }
}
