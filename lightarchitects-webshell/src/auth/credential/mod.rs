//! Provider credential substrate.
//!
//! Defines the [`ProviderCredentialProvider`] trait, core types, and
//! re-exports for the credential route handlers.
//!
//! ## Module layout
//!
//! | Module | Contents |
//! |--------|----------|
//! | [`flow`] | `CredentialFlow` enum — acquisition protocol per provider |
//! | [`pkce`] | PKCE challenge generation (OA-1) |
//! | [`keychain`] | macOS Keychain subprocess helpers (OA-3, OA-12) |
//! | [`google`] | `GoogleCredentialProvider` — OAuth PKCE redirect |
//! | [`github`] | `GitHubCredentialProvider` — RFC 8628 Device Flow |
//! | [`anthropic`] | `AnthropicCredentialProvider` — API key |
//! | [`openai`] | `OpenAiCredentialProvider` — API key |
//! | [`mistral`] | `MistralCredentialProvider` — API key |
//! | [`ollama`] | `OllamaCredentialProvider` — CLI subprocess |
//! | [`routes`] | Axum handlers registered in `build_app` |

pub mod anthropic;
pub mod flow;
pub mod github;
pub mod google;
pub mod keychain;
pub mod mistral;
pub mod ollama;
pub mod openai;
pub mod pkce;
pub mod routes;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Instant;

pub use flow::CredentialFlow;

/// Connection state of a provider credential.
///
/// Stored in `AppState::credential_store` as an in-memory cache.
/// The Keychain is the authoritative source; this cache avoids a subprocess
/// call on every status request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialState {
    /// No credential stored or the Keychain entry was deleted.
    NotConnected,
    /// OAuth flow or subprocess handshake in progress.
    Authenticating,
    /// Credential present in the Keychain and verified.
    Connected,
    /// Operator explicitly signed out; credential removed.
    SignedOut,
    /// Refresh token present but access token renewal failed.
    RefreshFailed,
}

/// In-flight OAuth CSRF state (OA-2).
///
/// Stored in `AppState::oauth_states` keyed by the `state` UUID parameter.
/// Separate from `auth_nonces` — different TTL (120 s vs 60 s) and a
/// provider-specific payload that `auth_nonces` does not have.
///
/// Entries are consumed on the first valid callback and evicted at TTL.
#[derive(Debug, Clone)]
pub struct OAuthPendingState {
    /// Provider identifier (e.g. `"google"`).
    pub provider_id: String,
    /// PKCE code verifier corresponding to the challenge in the auth URL.
    pub code_verifier: String,
    /// Redirect URI sent in the auth request (OA-5 — validated at callback).
    pub redirect_uri: String,
    /// Wall-clock expiry (120-second TTL from creation).
    pub expires_at: Instant,
}

/// Trait implemented by every credential provider.
///
/// Providers are stateless structs; all mutable state lives in
/// `AppState::oauth_states` (in-flight) and the macOS Keychain (persisted).
pub trait ProviderCredentialProvider: Send + Sync {
    /// Unique provider identifier (e.g. `"google"`, `"github"`).
    ///
    /// Used as the key in `AppState::credential_store` and as the
    /// `{provider}` path segment in credential routes.
    fn provider_id(&self) -> &'static str;

    /// Returns the acquisition flow for this provider.
    ///
    /// For `OAuthRedirect` providers, returns the base endpoints and scopes.
    /// The route handler generates the PKCE pair and state UUID separately.
    ///
    /// # Errors
    ///
    /// Returns an error if the flow cannot be constructed (e.g. missing config).
    fn credential_flow(&self) -> Result<CredentialFlow>;

    /// Persists `secret` to the macOS Keychain (OA-3, OA-12).
    ///
    /// # Errors
    ///
    /// Returns an error if the Keychain subprocess fails.
    fn store_credential(&self, secret: &str) -> Result<()>;

    /// Loads the stored credential from the Keychain.
    ///
    /// Returns `Ok(None)` when no credential is stored.
    ///
    /// # Errors
    ///
    /// Returns an error if the Keychain subprocess fails unexpectedly.
    fn load_credential(&self) -> Result<Option<String>>;

    /// Removes the stored credential from the Keychain (sign-out).
    ///
    /// Returns `Ok(())` if the credential was already absent.
    ///
    /// # Errors
    ///
    /// Returns an error if the Keychain subprocess fails.
    fn revoke_credential(&self) -> Result<()>;
}
