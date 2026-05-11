//! # lightarchitects-auth
//!
//! Light Architects API key authentication crate.
//! Zero soul dependency — imported by all MCP server binaries.
//!
//! ## Key Components
//!
//! - [`KeyReader`]: Reads API key from env var or file
//! - [`KeyValidator`]: Validates key against lightarchitects.ai with caching
//! - [`RevocationWatcher`]: Polls for revoked keys
//! - [`AuthConfig`]: Configuration for the auth system
//! - `auth_login`: Browser-based PKCE auth flow (feature: `cli`)

mod auth_guard;
mod config;
mod error;
mod identity_resolver;
mod key_reader;
mod key_validator;
mod revocation;

#[cfg(feature = "cli")]
mod cli;
#[cfg(feature = "cli")]
mod login;

pub use auth_guard::AuthGuard;
pub use config::{AuthConfig, IdentityConfig, IdentityProvider};
pub use error::AuthError;
pub use identity_resolver::{resolve_identity, resolve_user_id_from_cache, user_id_or_local};
pub use key_reader::KeyReader;
pub use key_validator::{KeyCache, KeyValidator, ValidationResult};
pub use revocation::RevocationWatcher;

#[cfg(feature = "cli")]
pub use cli::AuthCommand;
#[cfg(feature = "cli")]
pub use login::auth_login;

/// Degradation tier for the auth system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthTier {
    /// Tier A: No key found — BLOCK (refuse to start)
    NoKey,
    /// Tier B: Expired cache + validation endpoint unreachable — WARN with grace period
    GracePeriod {
        /// How many more grace period extensions are available before the key is blocked.
        resets_remaining: u8,
    },
    /// Tier C: Valid cached key — ALLOW (normal operation)
    Valid,
}

/// Subscription tier returned by the validation endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionTier {
    /// Free tier.
    Free,
    /// Pro tier.
    Pro,
}

impl std::fmt::Display for SubscriptionTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Free => write!(f, "free"),
            Self::Pro => write!(f, "pro"),
        }
    }
}
