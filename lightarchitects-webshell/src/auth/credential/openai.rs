//! `OpenAI` API key credential provider (OA-8).
//!
//! The operator pastes a project API key; it is stored in the macOS Keychain.
//! No OAuth flow — static key issued by `platform.openai.com`.

use anyhow::Result;

use super::{CredentialFlow, ProviderCredentialProvider, keychain};

/// macOS Keychain service name (OA-12).
pub const KEYCHAIN_SERVICE: &str = "la-openai-credential";

/// `OpenAI` API key provider.
pub struct OpenAiCredentialProvider;

impl ProviderCredentialProvider for OpenAiCredentialProvider {
    fn provider_id(&self) -> &'static str {
        "openai"
    }

    fn credential_flow(&self) -> Result<CredentialFlow> {
        Ok(CredentialFlow::ApiKey {
            prompt: "Paste your OpenAI API key (sk-proj-…):",
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
    fn provider_id_is_openai() {
        assert_eq!(OpenAiCredentialProvider.provider_id(), "openai");
    }

    #[test]
    fn credential_flow_is_api_key() {
        let flow = OpenAiCredentialProvider.credential_flow().unwrap();
        assert!(
            matches!(flow, CredentialFlow::ApiKey { .. }),
            "OpenAI must use ApiKey flow"
        );
    }
}
