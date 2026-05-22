//! Mistral API key credential provider (OA-10).
//!
//! The operator pastes an API key; it is stored in the macOS Keychain.
//! No OAuth flow — static key issued by `console.mistral.ai`.

use anyhow::Result;

use super::{CredentialFlow, ProviderCredentialProvider, keychain};

/// macOS Keychain service name (OA-12).
pub const KEYCHAIN_SERVICE: &str = "la-mistral-credential";

/// Mistral API key provider.
pub struct MistralCredentialProvider;

impl ProviderCredentialProvider for MistralCredentialProvider {
    fn provider_id(&self) -> &'static str {
        "mistral"
    }

    fn credential_flow(&self) -> Result<CredentialFlow> {
        Ok(CredentialFlow::ApiKey {
            prompt: "Paste your Mistral API key:",
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
    fn provider_id_is_mistral() {
        assert_eq!(MistralCredentialProvider.provider_id(), "mistral");
    }

    #[test]
    fn credential_flow_is_api_key() {
        let flow = MistralCredentialProvider.credential_flow().unwrap();
        assert!(
            matches!(flow, CredentialFlow::ApiKey { .. }),
            "Mistral must use ApiKey flow"
        );
    }
}
