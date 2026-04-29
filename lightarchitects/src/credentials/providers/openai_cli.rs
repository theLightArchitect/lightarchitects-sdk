//! `OpenAI` Codex CLI credential provider.
//!
//! SENSITIVE: canonical service / file / env names are scoped to this
//! file. They are contract strings with the target CLI — do not modify
//! without updating the corresponding references in codex-rs (see
//! `codex-rs/login/src/auth/storage.rs`).
//!
//! **Logging policy**: these constants MUST NOT appear in `tracing`
//! output or user-visible diagnostics. The crate's `Debug` impls redact
//! them; if you add new log sites touching this module, use
//! [`ProviderId`] not the canonical strings.
//!
//! Default storage mode in Codex (as of codex-rs `storage.rs:33`) is
//! [`AuthCredentialsStoreMode::File`] — `$CODEX_HOME/auth.json`. Keyring
//! (service `"Codex Auth"`) is opt-in via `cli_auth_credentials_store =
//! "keyring"` in `config.toml`. We probe the file first, then the keyring
//! only if the config toggles it on.
//!
//! Detection precedence:
//!
//! 1. `OPENAI_API_KEY`
//! 2. `CODEX_API_KEY`
//! 3. File `${CODEX_HOME ?? ~/.codex}/auth.json`
//! 4. Keyring (only if `config.toml` enables it) — service `"Codex Auth"`,
//!    account `"cli|{sha256(canonical(codex_home))[:16]}"`

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use crate::credentials::platform::probe_keychain;
use crate::credentials::registry::CliCredentialProvider;
use crate::credentials::types::{Detection, Locator, ProbeError, ProviderId};

/// Stable opaque identifier.
pub(crate) const PROVIDER_ID: ProviderId = ProviderId([
    0x5f, 0x72, 0xa4, 0x18, 0xbd, 0x09, 0xc6, 0x31, 0x8e, 0x4d, 0x12, 0x7a, 0x95, 0xe8, 0x20, 0xb7,
]);

/// Public re-export.
pub const ID: ProviderId = PROVIDER_ID;

// Canonical contract strings — scoped to this module, never logged.
const SERVICE_NAME: &str = "Codex Auth";
const FILE_NAME: &str = "auth.json";
const DEFAULT_DIR: &str = ".codex";
const CONFIG_FILE_NAME: &str = "config.toml";

fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(DEFAULT_DIR)))
}

const ENV_PRECEDENCE: &[&str] = &["OPENAI_API_KEY", "CODEX_API_KEY"];

fn env_hit() -> Option<&'static str> {
    ENV_PRECEDENCE
        .iter()
        .copied()
        .find(|v| std::env::var(v).map(|s| !s.is_empty()).unwrap_or(false))
}

/// Compute the Codex keyring account identifier: `cli|<sha256[:16]>`.
fn keyring_account(dir: &std::path::Path) -> String {
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let hex = hex::encode(digest);
    let truncated = &hex[..hex.len().min(16)];
    format!("cli|{truncated}")
}

/// Return `true` if the Codex config opts into keyring storage.
fn config_uses_keyring(dir: &std::path::Path) -> bool {
    let cfg_path = dir.join(CONFIG_FILE_NAME);
    let Ok(contents) = std::fs::read_to_string(&cfg_path) else {
        return false;
    };
    let Ok(val) = contents.parse::<toml::Value>() else {
        return false;
    };
    val.get("cli_auth_credentials_store")
        .and_then(|v| v.as_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("keyring") || s.eq_ignore_ascii_case("auto"))
}

/// `OpenAI` Codex CLI provider.
pub(crate) struct OpenAiCliProvider;

#[async_trait]
impl CliCredentialProvider for OpenAiCliProvider {
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
        let Some(dir) = codex_home() else {
            return Ok(Detection {
                provider_id: PROVIDER_ID,
                available: false,
                locator: Locator::Absent,
            });
        };
        let file_path = dir.join(FILE_NAME);
        if file_path.exists() {
            return Ok(Detection {
                provider_id: PROVIDER_ID,
                available: true,
                locator: Locator::File,
            });
        }
        if config_uses_keyring(&dir) {
            let account = keyring_account(&dir);
            match probe_keychain(SERVICE_NAME, &account).await {
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
        }
        Ok(Detection {
            provider_id: PROVIDER_ID,
            available: false,
            locator: Locator::Absent,
        })
    }

    #[cfg(feature = "credentials-display-names")]
    fn display_name(&self) -> &'static str {
        "OpenAI Codex"
    }

    #[cfg(feature = "credentials-detailed-locator")]
    async fn probe_detailed(
        &self,
    ) -> Result<crate::credentials::types::DetailedLocator, ProbeError> {
        use crate::credentials::types::DetailedLocator;
        if let Some(var) = env_hit() {
            return Ok(DetailedLocator::Env { var });
        }
        let Some(dir) = codex_home() else {
            return Ok(DetailedLocator::Absent);
        };
        let file_path = dir.join(FILE_NAME);
        if file_path.exists() {
            return Ok(DetailedLocator::File { path: file_path });
        }
        if config_uses_keyring(&dir) {
            let account = keyring_account(&dir);
            match probe_keychain(SERVICE_NAME, &account).await {
                Ok(true) => {
                    return Ok(DetailedLocator::Keychain {
                        service: SERVICE_NAME.to_owned(),
                        account,
                    });
                }
                Ok(false) => {}
                Err(_) => {
                    return Err(ProbeError::Keychain {
                        provider_id: PROVIDER_ID,
                    });
                }
            }
        }
        Ok(DetailedLocator::Absent)
    }
}
