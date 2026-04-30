//! Google Gemini CLI credential provider.
//!
//! SENSITIVE: canonical file / env names are scoped to this file. They
//! are contract strings with the target CLI.
//!
//! **Logging policy**: these constants MUST NOT appear in `tracing`
//! output or user-visible diagnostics. The crate's `Debug` impls redact
//! them; if you add new log sites touching this module, use
//! [`ProviderId`] not the canonical strings.
//!
//! gemini-cli.js stores OAuth credentials at `~/.gemini/oauth_creds.json`
//! after `gemini auth`. Env-var override via `GEMINI_API_KEY` /
//! `GOOGLE_API_KEY`. Service-account credentials are referenced via
//! `GOOGLE_APPLICATION_CREDENTIALS` pointing at a JSON file (existence-
//! only check — we never read its content).
//!
//! No current Keychain usage. If Google adds libsecret/Keychain storage
//! upstream, add a `Locator::Keychain` branch here without touching the
//! public API.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::credentials::registry::CliCredentialProvider;
use crate::credentials::types::{Detection, Locator, ProbeError, ProviderId};

/// Stable opaque identifier.
pub(crate) const PROVIDER_ID: ProviderId = ProviderId([
    0x7c, 0x04, 0xf1, 0x55, 0x3a, 0xbe, 0x91, 0x08, 0xd2, 0x76, 0xaa, 0x1e, 0x4f, 0xc9, 0x63, 0x2d,
]);

/// Public re-export.
pub const ID: ProviderId = PROVIDER_ID;

// Canonical contract strings — scoped to this module, never logged.
const FILE_NAME: &str = "oauth_creds.json";
const DEFAULT_DIR: &str = ".gemini";

fn gemini_home() -> Option<PathBuf> {
    std::env::var_os("GEMINI_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(DEFAULT_DIR)))
}

const ENV_PRECEDENCE: &[&str] = &[
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GOOGLE_APPLICATION_CREDENTIALS",
];

fn env_hit() -> Option<&'static str> {
    ENV_PRECEDENCE
        .iter()
        .copied()
        .find(|v| std::env::var(v).map(|s| !s.is_empty()).unwrap_or(false))
}

/// Google Gemini CLI provider.
pub(crate) struct GoogleCliProvider;

#[async_trait]
impl CliCredentialProvider for GoogleCliProvider {
    fn id(&self) -> ProviderId {
        PROVIDER_ID
    }

    async fn probe(&self) -> Result<Detection, ProbeError> {
        if env_hit().is_some() {
            return Ok(Detection {
                provider_id: PROVIDER_ID,
                available: true,
                locator: Locator::Env,
            });
        }
        if let Some(dir) = gemini_home() {
            let path = dir.join(FILE_NAME);
            if path.exists() {
                return Ok(Detection {
                    provider_id: PROVIDER_ID,
                    available: true,
                    locator: Locator::File,
                });
            }
        }
        Ok(Detection {
            provider_id: PROVIDER_ID,
            available: false,
            locator: Locator::Absent,
        })
    }

    #[cfg(feature = "credentials-display-names")]
    fn display_name(&self) -> &'static str {
        "Google Gemini"
    }

    #[cfg(feature = "credentials-detailed-locator")]
    async fn probe_detailed(
        &self,
    ) -> Result<crate::credentials::types::DetailedLocator, ProbeError> {
        use crate::credentials::types::DetailedLocator;
        if let Some(var) = env_hit() {
            return Ok(DetailedLocator::Env { var });
        }
        if let Some(dir) = gemini_home() {
            let path = dir.join(FILE_NAME);
            if path.exists() {
                return Ok(DetailedLocator::File { path });
            }
        }
        Ok(DetailedLocator::Absent)
    }
}
