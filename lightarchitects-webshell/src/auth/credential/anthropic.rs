//! Anthropic API key credential provider (OA-7, OA-8).
//!
//! The operator pastes an API key; it is stored in the macOS Keychain.
//! No OAuth flow — static key issued by `console.anthropic.com`.

use anyhow::Result;

use super::{CredentialFlow, ProviderCredentialProvider, keychain};

/// macOS Keychain service name (OA-12).
pub const KEYCHAIN_SERVICE: &str = "la-anthropic-credential";

/// Anthropic API key provider.
pub struct AnthropicCredentialProvider;

impl ProviderCredentialProvider for AnthropicCredentialProvider {
    fn provider_id(&self) -> &'static str {
        "anthropic"
    }

    fn credential_flow(&self) -> Result<CredentialFlow> {
        Ok(CredentialFlow::ApiKey {
            prompt: "Paste your Anthropic API key (sk-ant-…):",
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
    fn provider_id_is_anthropic() {
        assert_eq!(AnthropicCredentialProvider.provider_id(), "anthropic");
    }

    #[test]
    fn credential_flow_is_api_key() {
        let flow = AnthropicCredentialProvider.credential_flow().unwrap();
        assert!(
            matches!(flow, CredentialFlow::ApiKey { .. }),
            "Anthropic must use ApiKey flow"
        );
    }
}
