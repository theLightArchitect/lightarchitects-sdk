use crate::auth::{AuthConfig, AuthError};
use std::path::Path;
use tracing::debug;

/// Reads the LA API key via a three-tier priority chain.
///
/// | Priority | Source | Always active? |
/// |----------|--------|----------------|
/// | 1 | `LA_API_KEY` environment variable | Yes |
/// | 2 | macOS Keychain (`lightarchitects` / `la-api-key`) | `keychain` feature + macOS only |
/// | 3 | Key file at `config.key_file_path` | Yes |
pub struct KeyReader;

impl KeyReader {
    /// Read the API key. Returns the raw key string.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::NoKeyFound`] if all priority sources are exhausted.
    pub fn read(config: &AuthConfig) -> Result<String, AuthError> {
        // Priority 1: Environment variable — always checked first.
        if let Ok(key) = std::env::var("LA_API_KEY") {
            let key = key.trim().to_string();
            if !key.is_empty() {
                debug!("API key loaded from LA_API_KEY env var");
                return Ok(key);
            }
        }

        // Priority 2: macOS Keychain — only when `keychain` feature is enabled.
        // Falls through silently on miss or error so Priority 3 always gets a chance.
        #[cfg(all(target_os = "macos", feature = "keychain"))]
        {
            use crate::crypto::secrets::{KeychainStore, SecretStore};
            use secrecy::ExposeSecret as _;

            let store = KeychainStore::with_service("lightarchitects");
            match store.get("la-api-key") {
                Ok(Some(secret)) => {
                    let key = secret.expose_secret().trim().to_string();
                    if !key.is_empty() {
                        debug!("API key loaded from macOS Keychain (lightarchitects/la-api-key)");
                        return Ok(key);
                    }
                    debug!("Keychain entry lightarchitects/la-api-key is present but empty");
                }
                Ok(None) => {
                    debug!("Keychain miss: lightarchitects/la-api-key not found");
                }
                Err(e) => {
                    debug!("Keychain read error for lightarchitects/la-api-key: {e}");
                }
            }
        }

        // Priority 3: Key file.
        Self::read_from_file(&config.key_file_path)
    }

    fn read_from_file(path: &Path) -> Result<String, AuthError> {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                let key = contents.trim().to_string();
                if key.is_empty() {
                    return Err(AuthError::NoKeyFound {
                        path: path.display().to_string(),
                    });
                }
                debug!("API key loaded from {}", path.display());
                Ok(key)
            }
            Err(_) => Err(AuthError::NoKeyFound {
                path: path.display().to_string(),
            }),
        }
    }

    /// Save an API key to the local file.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Io`] if the parent directory cannot be created or the file cannot be written.
    pub fn save(config: &AuthConfig, key: &str) -> Result<(), AuthError> {
        if let Some(parent) = config.key_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write with restrictive permissions (owner read/write only)
        std::fs::write(&config.key_file_path, key)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&config.key_file_path, perms)?;
        }

        debug!("API key saved to {}", config.key_file_path.display());
        Ok(())
    }

    /// Remove the local API key file.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Io`] if the file exists but cannot be removed.
    pub fn remove(config: &AuthConfig) -> Result<(), AuthError> {
        if config.key_file_path.exists() {
            std::fs::remove_file(&config.key_file_path)?;
            debug!("API key removed from {}", config.key_file_path.display());
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unsafe_code)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn isolated(dir: &TempDir) -> AuthConfig {
        AuthConfig {
            key_file_path: dir.path().join("la-api-key"),
            cache_file_path: dir.path().join("la-key-cache.json"),
            revoked_file_path: dir.path().join("la-revoked"),
            ..Default::default()
        }
    }

    #[test]
    fn read_from_file_returns_trimmed_key() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        std::fs::write(&cfg.key_file_path, "  la-test-key-abc\n").unwrap();
        assert_eq!(
            KeyReader::read_from_file(&cfg.key_file_path).unwrap(),
            "la-test-key-abc"
        );
    }

    #[test]
    fn missing_file_returns_no_key_found() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        // No file written — must return NoKeyFound
        assert!(matches!(
            KeyReader::read(&cfg),
            Err(AuthError::NoKeyFound { .. })
        ));
    }

    #[test]
    fn whitespace_only_file_returns_no_key_found() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        std::fs::write(&cfg.key_file_path, "   \n  \n").unwrap();
        assert!(matches!(
            KeyReader::read(&cfg),
            Err(AuthError::NoKeyFound { .. })
        ));
    }

    #[test]
    fn save_creates_nested_dirs_and_writes_key() {
        let dir = TempDir::new().unwrap();
        let cfg = AuthConfig {
            key_file_path: dir.path().join("nested/dir/la-api-key"),
            cache_file_path: dir.path().join("la-key-cache.json"),
            revoked_file_path: dir.path().join("la-revoked"),
            ..Default::default()
        };
        KeyReader::save(&cfg, "save-test-key").unwrap();
        let contents = std::fs::read_to_string(&cfg.key_file_path).unwrap();
        assert_eq!(contents.trim(), "save-test-key");
    }

    #[test]
    fn save_then_read_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        KeyReader::save(&cfg, "round-trip-key").unwrap();
        assert_eq!(KeyReader::read(&cfg).unwrap(), "round-trip-key");
    }

    #[test]
    fn remove_deletes_existing_file() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        std::fs::write(&cfg.key_file_path, "some-key").unwrap();
        assert!(cfg.key_file_path.exists());
        KeyReader::remove(&cfg).unwrap();
        assert!(!cfg.key_file_path.exists());
    }

    #[test]
    fn remove_is_noop_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        KeyReader::remove(&cfg).unwrap(); // no file — should not error
    }

    // ── Priority chain tests ──────────────────────────────────────────────────
    // These tests validate the three-tier priority without requiring a live
    // Keychain (Priority 2 is cfg-gated on target_os = "macos" + keychain feat).

    #[test]
    fn priority_1_env_var_wins_when_file_also_present() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        std::fs::write(&cfg.key_file_path, "file-key").unwrap();
        // SAFETY: test-only env mutation; test is not parallel-sensitive here
        // because LA_API_KEY is cleared at the end of the function.
        unsafe { std::env::set_var("LA_API_KEY", "env-key") };
        let result = KeyReader::read(&cfg);
        unsafe { std::env::remove_var("LA_API_KEY") };
        assert_eq!(result.unwrap(), "env-key");
    }

    #[test]
    fn priority_1_env_var_trims_whitespace() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        unsafe { std::env::set_var("LA_API_KEY", "  padded-key\n") };
        let result = KeyReader::read(&cfg);
        unsafe { std::env::remove_var("LA_API_KEY") };
        assert_eq!(result.unwrap(), "padded-key");
    }

    #[test]
    fn priority_1_empty_env_falls_through_to_file() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        std::fs::write(&cfg.key_file_path, "file-key").unwrap();
        unsafe { std::env::set_var("LA_API_KEY", "") };
        let result = KeyReader::read(&cfg);
        unsafe { std::env::remove_var("LA_API_KEY") };
        // Empty env var → falls through to Priority 3 (file)
        assert_eq!(result.unwrap(), "file-key");
    }

    #[test]
    fn priority_1_whitespace_env_falls_through_to_file() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        std::fs::write(&cfg.key_file_path, "file-key-ws").unwrap();
        unsafe { std::env::set_var("LA_API_KEY", "   ") };
        let result = KeyReader::read(&cfg);
        unsafe { std::env::remove_var("LA_API_KEY") };
        assert_eq!(result.unwrap(), "file-key-ws");
    }

    #[test]
    fn priority_3_file_used_when_no_env_and_no_keychain() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        // Ensure env absent (remove if accidentally set in test environment)
        unsafe { std::env::remove_var("LA_API_KEY") };
        std::fs::write(&cfg.key_file_path, "file-only-key").unwrap();
        assert_eq!(KeyReader::read(&cfg).unwrap(), "file-only-key");
    }

    #[test]
    fn all_sources_absent_returns_no_key_found() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        unsafe { std::env::remove_var("LA_API_KEY") };
        // No file, no keychain entry — should error
        assert!(matches!(
            KeyReader::read(&cfg),
            Err(AuthError::NoKeyFound { .. })
        ));
    }

    #[test]
    fn priority_chain_order_is_p1_then_p3_without_keychain() {
        // Without keychain feature: chain is P1 → P3 with no P2 intervening.
        // Verify both work in order.
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir);
        std::fs::write(&cfg.key_file_path, "chain-file-key").unwrap();

        unsafe { std::env::remove_var("LA_API_KEY") };
        assert_eq!(KeyReader::read(&cfg).unwrap(), "chain-file-key"); // P3

        unsafe { std::env::set_var("LA_API_KEY", "chain-env-key") };
        assert_eq!(KeyReader::read(&cfg).unwrap(), "chain-env-key"); // P1
        unsafe { std::env::remove_var("LA_API_KEY") };
    }
}
