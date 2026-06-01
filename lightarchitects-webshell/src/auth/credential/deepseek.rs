//! `DeepSeek` API key credential provider (OA-10).
//!
//! The operator pastes a `DeepSeek` API key; it is stored in the macOS Keychain.
//! No OAuth flow — static key issued at `platform.deepseek.com`.

use anyhow::Result;

use super::{CredentialFlow, ProviderCredentialProvider, keychain};

/// macOS Keychain service name (OA-12).
pub const KEYCHAIN_SERVICE: &str = "la-deepseek-credential";

/// `DeepSeek` API key credential provider.
pub struct DeepSeekCredentialProvider;

impl ProviderCredentialProvider for DeepSeekCredentialProvider {
    fn provider_id(&self) -> &'static str {
        "deepseek"
    }

    fn credential_flow(&self) -> Result<CredentialFlow> {
        Ok(CredentialFlow::ApiKey {
            prompt: "Paste your DeepSeek API key (sk-…):",
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
    fn provider_id_is_deepseek() {
        assert_eq!(DeepSeekCredentialProvider.provider_id(), "deepseek");
    }

    #[test]
    fn credential_flow_is_api_key() {
        let flow = DeepSeekCredentialProvider.credential_flow().unwrap();
        assert!(
            matches!(flow, CredentialFlow::ApiKey { .. }),
            "DeepSeek must use ApiKey flow"
        );
    }
}
