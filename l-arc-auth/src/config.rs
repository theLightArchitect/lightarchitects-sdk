use std::path::PathBuf;
use std::time::Duration;

/// Configuration for the Light Architects auth system.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Base URL for lightarchitects.io API
    pub api_base_url: String,

    /// Path to the local API key file
    pub key_file_path: PathBuf,

    /// Path to the local key cache file
    pub cache_file_path: PathBuf,

    /// Path to the local revocation list
    pub revoked_file_path: PathBuf,

    /// Cache TTL for validated keys (default: 1 hour)
    pub cache_ttl: Duration,

    /// Background refresh interval (default: 50 minutes)
    pub refresh_interval: Duration,

    /// Revocation polling interval (default: 5 minutes)
    pub revocation_poll_interval: Duration,

    /// Maximum grace period resets before hard-block (default: 3)
    pub max_grace_resets: u8,

    /// Auth login callback timeout (default: 60 seconds)
    pub login_timeout: Duration,
}

impl Default for AuthConfig {
    fn default() -> Self {
        let soul_config = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".soul")
            .join("config");

        Self {
            api_base_url: "https://lightarchitects.io".to_string(),
            key_file_path: soul_config.join("la-api-key"),
            cache_file_path: soul_config.join("la-key-cache.json"),
            revoked_file_path: soul_config.join("la-revoked"),
            cache_ttl: Duration::from_secs(3600), // 1 hour
            refresh_interval: Duration::from_secs(3000), // 50 minutes
            revocation_poll_interval: Duration::from_secs(300), // 5 minutes
            max_grace_resets: 3,
            login_timeout: Duration::from_secs(60),
        }
    }
}

impl AuthConfig {
    /// Create config with a custom API base URL (for testing or self-hosted).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.api_base_url = url.into();
        self
    }
}
