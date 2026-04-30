//! Anthropic Claude CLI credential provider.
//!
//! SENSITIVE: canonical service / file / env names are scoped to this
//! file. They are contract strings with the target CLI — do not modify
//! without updating the corresponding references in the Claude Code
//! source (see `utils/secureStorage/macOsKeychainHelpers.ts`).
//!
//! **Logging policy**: these constants MUST NOT appear in `tracing`
//! output or user-visible diagnostics. The crate's `Debug` impls redact
//! them; if you add new log sites touching this module, use
//! [`ProviderId`] not the canonical strings.
//!
//! Detection precedence mirrors Claude Code's `auth.ts`:
//!
//! 1. `ANTHROPIC_AUTH_TOKEN`
//! 2. `CLAUDE_CODE_OAUTH_TOKEN`
//! 3. `ANTHROPIC_API_KEY`
//! 4. macOS Keychain — service `"Claude Code-credentials"`, account `$USER`
//! 5. File `${CLAUDE_CONFIG_DIR ?? ~/.claude}/.credentials.json`

use async_trait::async_trait;
use std::path::PathBuf;

use crate::credentials::platform::probe_keychain;
use crate::credentials::registry::CliCredentialProvider;
use crate::credentials::types::{Detection, Locator, ProbeError, ProviderId};

/// Stable opaque identifier for this provider. Do not change — persisted
/// in caches and UI state across SDK releases.
pub(crate) const PROVIDER_ID: ProviderId = ProviderId([
    0xa1, 0x3c, 0x7f, 0x92, 0x46, 0xd8, 0x20, 0x5e, 0x04, 0x9b, 0xcc, 0x81, 0x7d, 0x33, 0xe1, 0x68,
]);

/// Public re-export for callers that want to pattern-match on provider id.
pub const ID: ProviderId = PROVIDER_ID;

// Canonical contract strings — scoped to this module, never logged.
const SERVICE_NAME: &str = "Claude Code-credentials";
const FILE_NAME: &str = ".credentials.json";
const DEFAULT_DIR: &str = ".claude";

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(DEFAULT_DIR)))
}

fn username() -> String {
    std::env::var("USER").unwrap_or_else(|_| "claude-code-user".to_owned())
}

const ENV_PRECEDENCE: &[&str] = &[
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "ANTHROPIC_API_KEY",
];

fn env_hit() -> Option<&'static str> {
    ENV_PRECEDENCE
        .iter()
        .copied()
        .find(|v| std::env::var(v).map(|s| !s.is_empty()).unwrap_or(false))
}

/// Anthropic Claude CLI provider.
pub(crate) struct AnthropicCliProvider;

#[async_trait]
impl CliCredentialProvider for AnthropicCliProvider {
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
        let user = username();
        match probe_keychain(SERVICE_NAME, &user).await {
            Ok(true) => {
                return Ok(Detection {
                    provider_id: PROVIDER_ID,
                    available: true,
                    locator: Locator::Keychain,
                });
            }
            Ok(false) => {}
            Err(_) => {
                return Err(ProbeError::Keychain {
                    provider_id: PROVIDER_ID,
                });
            }
        }
        if let Some(dir) = config_dir() {
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
        "Claude Code"
    }

    #[cfg(feature = "credentials-detailed-locator")]
    async fn probe_detailed(
        &self,
    ) -> Result<crate::credentials::types::DetailedLocator, ProbeError> {
        use crate::credentials::types::DetailedLocator;
        if let Some(var) = env_hit() {
            return Ok(DetailedLocator::Env { var });
        }
        let user = username();
        match probe_keychain(SERVICE_NAME, &user).await {
            Ok(true) => {
                return Ok(DetailedLocator::Keychain {
                    service: SERVICE_NAME.to_owned(),
                    account: user,
                });
            }
            Ok(false) => {}
            Err(_) => {
                return Err(ProbeError::Keychain {
                    provider_id: PROVIDER_ID,
                });
            }
        }
        if let Some(dir) = config_dir() {
            let path = dir.join(FILE_NAME);
            if path.exists() {
                return Ok(DetailedLocator::File { path });
            }
        }
        Ok(DetailedLocator::Absent)
    }
}
