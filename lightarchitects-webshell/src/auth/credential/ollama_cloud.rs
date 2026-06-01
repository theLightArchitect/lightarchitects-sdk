//! Ollama Cloud API key credential provider (OA-10).
//!
//! The operator pastes an Ollama Cloud Bearer token; it is stored in the
//! macOS Keychain.  Ollama Cloud uses Bearer auth, not `Authorization: Bearer`
//! — the token is forwarded as-is in the `LiteLLM` model `api_key` field.

use anyhow::Result;

use super::{CredentialFlow, ProviderCredentialProvider, keychain};

/// macOS Keychain service name (OA-12).
pub const KEYCHAIN_SERVICE: &str = "la-ollama-cloud-credential";

/// Ollama Cloud API key (Bearer token) credential provider.
pub struct OllamaCloudCredentialProvider;

impl ProviderCredentialProvider for OllamaCloudCredentialProvider {
    fn provider_id(&self) -> &'static str {
        "ollama-cloud"
    }

    fn credential_flow(&self) -> Result<CredentialFlow> {
        Ok(CredentialFlow::ApiKey {
            prompt: "Paste your Ollama Cloud Bearer token:",
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
    fn provider_id_is_ollama_cloud() {
        assert_eq!(OllamaCloudCredentialProvider.provider_id(), "ollama-cloud");
    }

    #[test]
    fn credential_flow_is_api_key() {
        let flow = OllamaCloudCredentialProvider.credential_flow().unwrap();
        assert!(
            matches!(flow, CredentialFlow::ApiKey { .. }),
            "Ollama Cloud must use ApiKey flow"
        );
    }
}
