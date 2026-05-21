//! Server-side GitHub PAT storage — Phase 4 security hardening.
//!
//! Reads the token from the macOS Keychain (`service = "lightarchitects-webshell"`,
//! `account = "github-pat"`) and falls back to the `GITHUB_PAT` environment variable.
//! The token is NEVER stored in `localStorage` or returned to the frontend.
//!
//! # Security invariants
//!
//! - `GitHubToken` wraps `secrecy::SecretString` so the value is zeroed on drop.
//! - The `Debug` impl emits `GitHubToken(*****)` — safe for logging.
//! - `github_proxy` accesses the token via `expose_secret()` only at the HTTP call site.

use secrecy::{ExposeSecret, SecretString};
use tracing::{debug, warn};

/// A GitHub Personal Access Token held server-side.
///
/// Zeroed on drop via [`secrecy::SecretString`].
pub struct GitHubToken(SecretString);

impl GitHubToken {
    /// Expose the raw token value — call only at the HTTP call site.
    pub fn as_str(&self) -> &str {
        self.0.expose_secret()
    }

    /// Construct a token from a raw string — test use only.
    #[cfg(test)]
    pub fn new(raw: String) -> Self {
        Self(SecretString::new(raw.into()))
    }
}

impl std::fmt::Debug for GitHubToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("GitHubToken").field(&"*****").finish()
    }
}

/// Load the GitHub PAT, preferring Keychain over environment variable.
///
/// Returns `None` when neither source is configured; `github_proxy` will
/// skip CI check-run fetches gracefully in that case.
pub fn load_github_pat() -> Option<GitHubToken> {
    // 1. Try macOS Keychain
    #[cfg(target_os = "macos")]
    {
        use security_framework::passwords::get_generic_password;
        match get_generic_password("lightarchitects-webshell", "github-pat") {
            Ok(bytes) => {
                if let Ok(s) = String::from_utf8(bytes) {
                    if !s.trim().is_empty() {
                        debug!("github PAT loaded from Keychain");
                        return Some(GitHubToken(SecretString::new(s.trim().to_string().into())));
                    }
                }
            }
            Err(e) => debug!("Keychain lookup failed (no PAT configured?): {e}"),
        }
    }

    // 2. Fallback: GITHUB_PAT env var
    if let Ok(pat) = std::env::var("GITHUB_PAT") {
        if !pat.trim().is_empty() {
            warn!("github PAT loaded from environment — consider moving to Keychain");
            return Some(GitHubToken(SecretString::new(
                pat.trim().to_string().into(),
            )));
        }
    }

    None
}
