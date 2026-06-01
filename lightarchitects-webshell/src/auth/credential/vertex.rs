//! Google Vertex AI credential provider (OA-10).
//!
//! Google Vertex AI requires a service account JSON credential (not a plain
//! API key) and a GCP project ID.  Both are stored in the macOS Keychain as
//! separate entries:
//!
//! - `la-vertex-credential` — the full service account JSON blob
//! - `la-vertex-project`    — the GCP project ID string
//!
//! The JSON content is stored as-is; the `LiteLLM` `vertex_credentials` field
//! accepts either a file path or a raw JSON string.

use anyhow::Result;

use super::{CredentialFlow, ProviderCredentialProvider, keychain};

/// Keychain service name for the service account JSON (OA-12).
pub const KEYCHAIN_SERVICE: &str = "la-vertex-credential";

/// Keychain service name for the GCP project ID.
pub const KEYCHAIN_PROJECT_SERVICE: &str = "la-vertex-project";

/// Google Vertex AI credential provider (service account JSON).
pub struct GoogleVertexCredentialProvider;

impl ProviderCredentialProvider for GoogleVertexCredentialProvider {
    fn provider_id(&self) -> &'static str {
        "google-vertex"
    }

    fn credential_flow(&self) -> Result<CredentialFlow> {
        Ok(CredentialFlow::ApiKey {
            prompt: "Paste your Google service account JSON (or file path):",
        })
    }

    fn store_credential(&self, secret: &str) -> Result<()> {
        keychain::keychain_set(KEYCHAIN_SERVICE, secret)
    }

    fn load_credential(&self) -> Result<Option<String>> {
        keychain::keychain_get(KEYCHAIN_SERVICE)
    }

    fn revoke_credential(&self) -> Result<()> {
        keychain::keychain_delete(KEYCHAIN_SERVICE)?;
        // Best-effort: remove the project ID entry too. Ignore NotFound.
        let _ = keychain::keychain_delete(KEYCHAIN_PROJECT_SERVICE);
        Ok(())
    }
}

/// Stores the GCP project ID in the Keychain.
///
/// Called separately from `store_credential` — the project ID is provided
/// in the `POST /api/auth/credential/google-vertex/key` request body alongside
/// the service account JSON.
///
/// # Errors
///
/// Returns an error if the Keychain subprocess fails.
pub fn store_project_id(project_id: &str) -> Result<()> {
    keychain::keychain_set(KEYCHAIN_PROJECT_SERVICE, project_id)
}

/// Loads the GCP project ID from the Keychain.
///
/// Returns `Ok(None)` when no project ID is stored.
///
/// # Errors
///
/// Returns an error if the Keychain subprocess fails unexpectedly.
pub fn load_project_id() -> Result<Option<String>> {
    keychain::keychain_get(KEYCHAIN_PROJECT_SERVICE)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn provider_id_is_google_vertex() {
        assert_eq!(
            GoogleVertexCredentialProvider.provider_id(),
            "google-vertex"
        );
    }

    #[test]
    fn credential_flow_is_api_key() {
        let flow = GoogleVertexCredentialProvider.credential_flow().unwrap();
        assert!(
            matches!(flow, CredentialFlow::ApiKey { .. }),
            "Vertex must use ApiKey flow (service account JSON)"
        );
    }
}
