//! API key resolution for HTTP providers.
//!
//! Release builds consult **only** the macOS Keychain — no environment variable
//! fallback. This enforces hardware-backed secret storage and closes the
//! env-var exfiltration vector identified in R-01 / SERAPH OA-12 audit item (a).
//!
//! Debug builds additionally accept `ANTHROPIC_API_KEY` / `VERTEX_API_KEY` as
//! a convenience for local development (avoids requiring a Keychain entry on
//! CI or non-macOS machines where the Keychain feature is not compiled in).

use secrecy::SecretString;

use crate::agent::ProviderError;
use crate::crypto::secrets::{KeychainStore, SecretStore};

/// Keychain service name used by all HTTP provider keys.
const SERVICE: &str = "lightarchitects";

/// Resolve the Anthropic API key.
///
/// Tries the macOS Keychain first (key `"anthropic-api-key"`, service
/// `"lightarchitects"`). In **release** builds this is the only source —
/// no environment variable fallback exists. In **debug** builds the
/// `ANTHROPIC_API_KEY` environment variable is accepted as a fallback.
///
/// # Errors
///
/// Returns [`ProviderError::AuthFailure`] when no key can be located.
#[allow(clippy::missing_errors_doc)]
pub fn resolve_anthropic_key() -> Result<SecretString, ProviderError> {
    let store = KeychainStore::with_service(SERVICE);
    if let Ok(Some(key)) = store.get("anthropic-api-key") {
        return Ok(key);
    }

    // Debug-only: env-var fallback for CI / non-macOS development.
    #[cfg(debug_assertions)]
    if let Ok(val) = std::env::var("ANTHROPIC_API_KEY") {
        if !val.is_empty() {
            return Ok(SecretString::from(val));
        }
    }

    Err(ProviderError::AuthFailure(
        "Anthropic API key not found; store in Keychain \
         (service=\"lightarchitects\", key=\"anthropic-api-key\")"
            .into(),
    ))
}

/// Resolve the Google AI Studio (Gemini) API key.
///
/// Follows the same Keychain-only policy as [`resolve_anthropic_key`].
///
/// # Naming note
///
/// Keychain key + env var are still named `vertex-api-key` / `VERTEX_API_KEY`
/// to preserve operator state from before the 2026-06-04 rename (the function
/// was previously `resolve_vertex_key`; the provider was `VertexHttpProvider`
/// but actually targeted `generativelanguage.googleapis.com` — Google AI Studio,
/// not production Vertex AI). A future migration may rename the keychain entry
/// once a real Vertex AI provider lands and the two need distinct credentials.
///
/// # Errors
///
/// Returns [`ProviderError::AuthFailure`] when no key can be located.
#[allow(clippy::missing_errors_doc)]
pub fn resolve_google_ai_studio_key() -> Result<SecretString, ProviderError> {
    let store = KeychainStore::with_service(SERVICE);
    if let Ok(Some(key)) = store.get("vertex-api-key") {
        return Ok(key);
    }

    #[cfg(debug_assertions)]
    if let Ok(val) = std::env::var("VERTEX_API_KEY") {
        if !val.is_empty() {
            return Ok(SecretString::from(val));
        }
    }

    Err(ProviderError::AuthFailure(
        "Google AI Studio (Gemini) API key not found; store in Keychain \
         (service=\"lightarchitects\", key=\"vertex-api-key\")"
            .into(),
    ))
}

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// Note: env-var mutation (std::env::set_var / remove_var) requires unsafe in
// Rust 1.81+ and is forbidden by this project's `-D unsafe-code` policy.
// The Keychain-only release invariant (SERAPH OA-12 audit item e) is enforced
// structurally: the `#[cfg(debug_assertions)]` block simply does not compile
// into release binaries, so no runtime test is required for that guarantee.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_anthropic_key_does_not_panic() {
        // Verifies the error path compiles and runs without panicking.
        // May return Ok (Keychain present on dev machine) or Err (CI, no Keychain).
        let _ = resolve_anthropic_key();
    }

    #[test]
    fn resolve_google_ai_studio_key_does_not_panic() {
        let _ = resolve_google_ai_studio_key();
    }
}
