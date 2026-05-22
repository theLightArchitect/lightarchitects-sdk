//! GitHub Device Flow credential provider (OA-9).
//!
//! Uses RFC 8628 device authorization grant — the operator visits a GitHub
//! URL and enters a code shown in the webshell, then the server polls until
//! the token is granted.  No browser redirect required.

use anyhow::Result;

use super::{CredentialFlow, ProviderCredentialProvider, keychain};

/// macOS Keychain service name (OA-12).
pub const KEYCHAIN_SERVICE: &str = "la-github-credential";

/// Device code request endpoint (RFC 8628 §3.1).
pub const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";

/// Token poll endpoint (RFC 8628 §3.4).
pub const POLL_URL: &str = "https://github.com/login/oauth/access_token";

/// GitHub Device Flow provider.
pub struct GitHubCredentialProvider;

impl ProviderCredentialProvider for GitHubCredentialProvider {
    fn provider_id(&self) -> &'static str {
        "github"
    }

    fn credential_flow(&self) -> Result<CredentialFlow> {
        Ok(CredentialFlow::DeviceFlow {
            device_code_url: DEVICE_CODE_URL,
            poll_url: POLL_URL,
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
    fn provider_id_is_github() {
        assert_eq!(GitHubCredentialProvider.provider_id(), "github");
    }

    #[test]
    fn credential_flow_is_device_flow() {
        let flow = GitHubCredentialProvider.credential_flow().unwrap();
        assert!(
            matches!(flow, CredentialFlow::DeviceFlow { .. }),
            "GitHub must use DeviceFlow"
        );
    }

    #[test]
    fn device_code_url_is_github() {
        let flow = GitHubCredentialProvider.credential_flow().unwrap();
        if let CredentialFlow::DeviceFlow {
            device_code_url, ..
        } = flow
        {
            assert_eq!(device_code_url, "https://github.com/login/device/code");
        }
    }

    #[test]
    fn poll_url_is_github_oauth() {
        let flow = GitHubCredentialProvider.credential_flow().unwrap();
        if let CredentialFlow::DeviceFlow { poll_url, .. } = flow {
            assert_eq!(poll_url, "https://github.com/login/oauth/access_token");
        }
    }
}
