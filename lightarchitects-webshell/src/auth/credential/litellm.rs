//! `LiteLLM` proxy API key credential provider (OA-10).
//!
//! The operator pastes a `LiteLLM` proxy API key; it is stored in the macOS
//! Keychain.  The base URL and model name live in `AppState.litellm_config`
//! and are updated atomically by `POST /api/litellm/config`.

use anyhow::Result;

use super::{CredentialFlow, ProviderCredentialProvider, keychain};

/// macOS Keychain service name (OA-12).
pub const KEYCHAIN_SERVICE: &str = "la-litellm-credential";

/// `LiteLLM` proxy API key credential provider.
pub struct LitellmCredentialProvider;

impl ProviderCredentialProvider for LitellmCredentialProvider {
    fn provider_id(&self) -> &'static str {
        "litellm"
    }

    fn credential_flow(&self) -> Result<CredentialFlow> {
        Ok(CredentialFlow::ApiKey {
            prompt: "Paste your LiteLLM proxy API key:",
        })
    }

    fn store_credential(&self, secret: &str) -> Result<()> {
        keychain::keychain_set(KEYCHAIN_SERVICE, secret)
    }

    fn load_credential(&self) -> Result<Option<String>> {
        keychain::keychain_get(KEYCHAIN_SERVICE)
    }

    fn revoke_credential(&self) -> Result<()> {
        keychain::keychain_delete(KEYCHAIN_SERVICE)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn provider_id_is_litellm() {
        assert_eq!(LitellmCredentialProvider.provider_id(), "litellm");
    }

    #[test]
    fn credential_flow_is_api_key() {
        let flow = LitellmCredentialProvider.credential_flow().unwrap();
        assert!(
            matches!(flow, CredentialFlow::ApiKey { .. }),
            "LiteLLM must use ApiKey flow"
        );
    }
}
