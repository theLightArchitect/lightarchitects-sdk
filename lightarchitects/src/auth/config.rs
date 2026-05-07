use std::path::PathBuf;
use std::time::Duration;

use crate::auth::AuthError;
use crate::core::paths;

/// Configuration for the Light Architects auth system.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Base URL for lightarchitects.ai API
    pub api_base_url: String,

    /// Path to the local API key file
    pub key_file_path: PathBuf,

    /// Path to the local key cache file
    pub cache_file_path: PathBuf,

    /// Path to the local revocation list
    pub revoked_file_path: PathBuf,

    /// Cache TTL for validated keys (default: 1 hour)
    pub cache_ttl: Duration,

    /// Background refresh interval (default: 50 minutes)
    pub refresh_interval: Duration,

    /// Revocation polling interval (default: 5 minutes)
    pub revocation_poll_interval: Duration,

    /// Maximum grace period resets before hard-block (default: 3)
    pub max_grace_resets: u8,

    /// Auth login callback timeout (default: 60 seconds)
    pub login_timeout: Duration,
}

impl Default for AuthConfig {
    fn default() -> Self {
        let soul_config = paths::soul_or_fallback().join("config");

        Self {
            api_base_url: "https://lightarchitects.ai".to_string(),
            key_file_path: soul_config.join("la-api-key"),
            cache_file_path: soul_config.join("la-key-cache.json"),
            revoked_file_path: soul_config.join("la-revoked"),
            cache_ttl: Duration::from_secs(3600), // 1 hour
            refresh_interval: Duration::from_secs(3000), // 50 minutes
            revocation_poll_interval: Duration::from_secs(300), // 5 minutes
            max_grace_resets: 3,
            login_timeout: Duration::from_secs(60),
        }
    }
}

impl AuthConfig {
    /// Create config with a custom API base URL (for testing or self-hosted).
    #[must_use]
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.api_base_url = url.into();
        self
    }
}

// ── IdentityProvider ──────────────────────────────────────────────────────────

/// Where the canonical `user_id` originates.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "provider")]
pub enum IdentityProvider {
    /// Resolved from `gh api /user`.
    GitHub {
        /// GitHub username returned by `gh api /user`.
        username: String,
    },
    /// Resolved from `git config --global user.email`.
    GitConfig {
        /// Email address from `git config --global user.email`.
        email: String,
    },
    /// User-supplied literal.
    Explicit {
        /// User-supplied identifier (slugified before use).
        user_id: String,
    },
    /// Offline / single-user mode (default).
    #[default]
    Local,
}

impl IdentityProvider {
    /// Derive the canonical `user_id` from the provider.
    #[must_use]
    pub fn user_id(&self) -> String {
        match self {
            Self::GitHub { username } => slugify(username),
            Self::GitConfig { email } => slugify(email.split('@').next().unwrap_or(email)),
            Self::Explicit { user_id } => slugify(user_id),
            Self::Local => "local".to_string(),
        }
    }
}

/// Resolved identity configuration, written to `~/.lightarchitects/config.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IdentityConfig {
    /// Which provider was used to resolve identity.
    pub provider: IdentityProvider,
    /// Canonical `user_id` (derived from provider).
    pub user_id: String,
    /// Optional public companion repo remote (SSH or credential-helper URL only).
    pub vault_remote: Option<String>,
    /// Keychain handle referencing the GitHub PAT (never the literal token).
    pub github_keychain_handle: Option<String>,
}

impl IdentityConfig {
    /// Resolve identity from a chosen provider.
    ///
    /// Derives the canonical `user_id` from the provider's `user_id()` method.
    /// This operation is infallible — the [`Result`] type is retained for API
    /// compatibility and future validation gates (e.g., slug collision check).
    ///
    /// # Errors
    ///
    /// Currently never errors. The [`Result`] type is reserved for future
    /// validation gates (e.g., slug uniqueness or reserved-word checks).
    pub fn resolve(provider: IdentityProvider) -> Result<Self, AuthError> {
        let user_id = provider.user_id();
        Ok(Self {
            provider,
            user_id,
            vault_remote: None,
            github_keychain_handle: None,
        })
    }

    /// Validate that a vault remote URL does NOT contain embedded credentials (S8).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidVaultRemote`] if credentials are detected.
    pub fn validate_vault_remote(url: &str) -> Result<(), AuthError> {
        // Reject https://user:pass@host patterns
        static EMBEDDED_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            // Hardcoded pattern — compilation failure is a programming error, not runtime.
            #[allow(clippy::expect_used)]
            regex::Regex::new(r"(?i)https?://[^@/]+:[^@/]+@").expect("valid hardcoded regex")
        });
        if EMBEDDED_RE.is_match(url) {
            return Err(AuthError::InvalidVaultRemote(url.to_string()));
        }
        Ok(())
    }

    /// Build a slug-safe user identifier from arbitrary input.
    ///
    /// Lowercase alphanumeric + hyphens only. Max 64 chars.
    #[must_use]
    pub fn slugify(input: &str) -> String {
        crate::auth::config::slugify(input)
    }
}

fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.to_lowercase().chars() {
        if (ch.is_ascii_alphanumeric() || ch == '-') && out.len() < 64 {
            out.push(ch);
        } else if !out.ends_with('-') && out.len() < 64 {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}
