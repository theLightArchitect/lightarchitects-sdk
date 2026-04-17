use crate::{AuthConfig, AuthError};
use std::path::Path;
use tracing::debug;

/// Reads the API key from environment variable or local file.
///
/// Priority: `LA_API_KEY` env var > `~/lightarchitects/soul/config/la-api-key` file
pub struct KeyReader;

impl KeyReader {
    /// Read the API key. Returns the raw key string.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::NoKeyFound`] if the `LA_API_KEY` env var is unset or empty
    /// and the key file is absent or empty.
    pub fn read(config: &AuthConfig) -> Result<String, AuthError> {
        // Priority 1: Environment variable
        if let Ok(key) = std::env::var("LA_API_KEY") {
            let key = key.trim().to_string();
            if !key.is_empty() {
                debug!("API key loaded from LA_API_KEY env var");
                return Ok(key);
            }
        }

        // Priority 2: Local file
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
