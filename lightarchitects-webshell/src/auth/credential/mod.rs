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
//! | [`litellm`] | `LitellmCredentialProvider` — API key |
//! | [`mistral`] | `MistralCredentialProvider` — API key |
//! | [`ollama`] | `OllamaCredentialProvider` — CLI subprocess |
//! | [`ollama_cloud`] | `OllamaCloudCredentialProvider` — API key (Bearer) |
//! | [`deepseek`] | `DeepSeekCredentialProvider` — API key |
//! | [`vertex`] | `GoogleVertexCredentialProvider` — service account JSON |
//! | [`routes`] | Axum handlers registered in `build_app` |

pub mod anthropic;
pub mod deepseek;
pub mod flow;
pub mod github;
pub mod google;
pub mod keychain;
pub mod litellm;
pub mod mistral;
pub mod ollama;
pub mod ollama_cloud;
pub mod openai;
pub mod pkce;
pub mod routes;
pub mod vertex;

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

/// Auto-detect connected credential providers from process env + common dotenv files.
///
/// Walks well-known env-var names per provider and marks each provider as
/// [`CredentialState::Connected`] in the cache if any matching variable is set
/// and non-empty. Does **NOT** write detected secrets to the Keychain — that
/// remains an explicit operator action via `POST /api/auth/credential/{provider}/key`.
///
/// **Sources scanned (in this order; first non-empty wins per provider):**
///
/// 1. Process environment (`std::env::vars()`) — inherits the shell that launched
///    the webshell, so anything `.zshrc`/`.bashrc` exported is already covered here.
/// 2. `~/.env` — simple `KEY=value` or `export KEY=value` lines; quotes stripped.
/// 3. `$CWD/.env` — same parse rules; project-local override.
///
/// `~/.zshrc`/`~/.bashrc` are **NOT** scanned: shell-script parsing is brittle
/// (conditional logic, command substitution, profile chains). Operators must
/// `source` their rc before launching the webshell to get those values into env.
///
/// **Security**: never logs values — only the *name* of the detected variable
/// and the resulting provider. Skips the call entirely if `LIGHTARCHITECTS_DISABLE_AUTO_DETECT`
/// is set in env (escape hatch for operators who keep stale keys in their shell).
///
/// **Honesty caveat**: marking `Connected` from a present env var is a
/// best-effort signal — the key may be malformed or revoked. The first
/// actual provider call will fail-loud if it's bad; `Connected` here means
/// "credential material exists in operator's environment", not "verified valid".
pub fn auto_detect_from_env_and_files(store: &dashmap::DashMap<String, CredentialState>) {
    if std::env::var("LIGHTARCHITECTS_DISABLE_AUTO_DETECT").is_ok() {
        tracing::info!(target: "credential::auto_detect", "auto-detect disabled by env flag");
        return;
    }

    // provider → list of env-var names whose presence implies "connected".
    // Mirrors the 10 modules in src/auth/credential/ plus openrouter (which has
    // no dedicated credential module yet — surfaces only via env auto-detect).
    let provider_envs: &[(&str, &[&str])] = &[
        ("anthropic", &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"]),
        ("openai", &["OPENAI_API_KEY"]),
        ("mistral", &["MISTRAL_API_KEY"]),
        ("github", &["GITHUB_TOKEN", "GH_TOKEN"]),
        ("google", &["GOOGLE_API_KEY", "GEMINI_API_KEY"]),
        ("ollama", &["OLLAMA_API_KEY"]),
        ("ollama_cloud", &["OLLAMA_CLOUD_API_KEY"]),
        // Vertex uses a service-account JSON path, not a raw API key — the env
        // var here points at the credentials file rather than carrying secrets.
        ("vertex", &["GOOGLE_APPLICATION_CREDENTIALS"]),
        ("litellm", &["LITELLM_API_KEY", "LITELLM_PROXY_API_KEY"]),
        ("deepseek", &["DEEPSEEK_API_KEY"]),
        ("openrouter", &["OPENROUTER_API_KEY"]),
    ];

    // Pass 1: process env (covers everything the operator's shell has exported).
    for (provider, vars) in provider_envs {
        // Skip if cache already has this provider — keychain-backed status from
        // an earlier handler call wins over env detection.
        if store.contains_key(*provider) {
            continue;
        }
        for var in *vars {
            if let Ok(val) = std::env::var(var) {
                if !val.trim().is_empty() {
                    store.insert((*provider).to_owned(), CredentialState::Connected);
                    tracing::info!(
                        target: "credential::auto_detect",
                        provider = provider,
                        source = "process_env",
                        var_name = var,
                        "auto-detected provider credential"
                    );
                    break;
                }
            }
        }
    }

    // Pass 2: dotenv files. Only honor entries for providers NOT already cached.
    let home = std::env::var("HOME").unwrap_or_default();
    let mut candidate_files: Vec<std::path::PathBuf> = Vec::with_capacity(2);
    if !home.is_empty() {
        candidate_files.push(std::path::PathBuf::from(&home).join(".env"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_env = cwd.join(".env");
        if cwd_env != candidate_files.first().cloned().unwrap_or_default() {
            candidate_files.push(cwd_env);
        }
    }

    for path in candidate_files {
        if !path.is_file() {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        // Cap file size at 256 KB to bound parse work + avoid runaway memory.
        if contents.len() > 256 * 1024 {
            tracing::warn!(target: "credential::auto_detect", path = %path.display(), size = contents.len(), "skipping oversized .env file");
            continue;
        }
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let Some((raw_key, raw_val)) = line.split_once('=') else {
                continue;
            };
            let key = raw_key.trim();
            let val = raw_val
                .trim()
                .trim_start_matches(['"', '\''])
                .trim_end_matches(['"', '\'']);
            if val.is_empty() {
                continue;
            }
            for (provider, vars) in provider_envs {
                if store.contains_key(*provider) {
                    continue;
                }
                if vars.contains(&key) {
                    store.insert((*provider).to_owned(), CredentialState::Connected);
                    tracing::info!(
                        target: "credential::auto_detect",
                        provider = provider,
                        source = "dotenv",
                        path = %path.display(),
                        var_name = key,
                        "auto-detected provider credential"
                    );
                }
            }
        }
    }
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
