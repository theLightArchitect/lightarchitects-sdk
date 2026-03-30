use crate::{AuthConfig, AuthError, AuthTier, SubscriptionTier};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::path::Path;
use tracing::{debug, warn};

/// Cached validation result stored locally.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeyCache {
    /// SHA-256 hash of the API key.
    pub key_hash: String,
    /// Subscription tier for this key.
    pub tier: SubscriptionTier,
    /// User ID associated with this key.
    pub user_id: String,
    /// When this cache entry was last validated.
    pub validated_at: DateTime<Utc>,
    /// When this cache entry expires.
    pub expires_at: DateTime<Utc>,
    /// Number of grace period resets consumed.
    pub grace_resets: u8,
}

/// Result from the `/api/validate-key` endpoint.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ValidationResult {
    /// Whether the key is valid.
    pub valid: bool,
    /// Subscription tier, present when `valid` is true.
    pub tier: Option<SubscriptionTier>,
    /// User ID, present when `valid` is true.
    pub user_id: Option<String>,
    /// Error message, present when `valid` is false.
    pub error: Option<String>,
}

/// Validates API keys against lightarchitects.io with local caching.
pub struct KeyValidator {
    config: AuthConfig,
    client: reqwest::Client,
}

impl KeyValidator {
    /// Create a new `KeyValidator` with the given configuration.
    pub fn new(config: AuthConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self { config, client }
    }

    /// Hash a key using SHA-256 (matches the server-side hashing).
    pub fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Validate a key, using cache when available.
    /// Returns the auth tier indicating how to proceed.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ValidationFailed`] if the server rejects the key.
    /// Returns [`AuthError::GraceExhausted`] if the endpoint is unreachable and grace resets are exhausted.
    /// Returns [`AuthError::Http`] or [`AuthError::Io`] on network or filesystem failures.
    pub async fn validate(&self, key: &str) -> Result<(AuthTier, KeyCache), AuthError> {
        let key_hash = Self::hash_key(key);

        // Check local cache first
        if let Some(cache) = self.read_cache()?
            && cache.key_hash == key_hash
        {
            if Utc::now() < cache.expires_at {
                debug!("Using cached validation (expires {})", cache.expires_at);
                return Ok((AuthTier::Valid, cache));
            }
            debug!("Cache expired, re-validating...");
        }

        // Call the validation endpoint
        match self.call_validate_endpoint(key).await {
            Ok(result) => {
                if result.valid {
                    let cache = KeyCache {
                        key_hash,
                        tier: result.tier.unwrap_or(SubscriptionTier::Free),
                        user_id: result.user_id.unwrap_or_default(),
                        validated_at: Utc::now(),
                        expires_at: Utc::now()
                            + chrono::Duration::from_std(self.config.cache_ttl)
                                .unwrap_or(chrono::Duration::hours(1)),
                        grace_resets: 0,
                    };
                    self.write_cache(&cache)?;
                    Ok((AuthTier::Valid, cache))
                } else {
                    Err(AuthError::ValidationFailed(
                        result
                            .error
                            .unwrap_or_else(|| "Invalid API key".to_string()),
                    ))
                }
            }
            Err(e) => {
                // Endpoint unreachable — check for grace period
                warn!("Validation endpoint unreachable: {e}");
                self.handle_grace_period(&key_hash, e)
            }
        }
    }

    async fn call_validate_endpoint(&self, key: &str) -> Result<ValidationResult, AuthError> {
        let url = format!("{}/api/validate-key", self.config.api_base_url);

        let response = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "key": key }))
            .send()
            .await?;

        let result: ValidationResult = response.json().await?;
        Ok(result)
    }

    fn handle_grace_period(
        &self,
        key_hash: &str,
        original_error: AuthError,
    ) -> Result<(AuthTier, KeyCache), AuthError> {
        // Read existing cache to check grace resets
        if let Some(mut cache) = self.read_cache().ok().flatten()
            && cache.key_hash == key_hash
        {
            if cache.grace_resets >= self.config.max_grace_resets {
                return Err(AuthError::GraceExhausted {
                    max: self.config.max_grace_resets,
                });
            }

            // Grant grace period: extend expiry by 1 hour, increment reset counter
            cache.grace_resets += 1;
            cache.expires_at = Utc::now() + chrono::Duration::hours(1);
            if let Err(e) = self.write_cache(&cache) {
                warn!("Failed to update grace period cache: {e}");
            }

            let resets_remaining = self.config.max_grace_resets - cache.grace_resets;
            warn!(
                "Grace period granted ({} resets remaining)",
                resets_remaining
            );
            return Ok((AuthTier::GracePeriod { resets_remaining }, cache));
        }

        // No cache at all — can't grant grace period
        Err(original_error)
    }

    fn read_cache(&self) -> Result<Option<KeyCache>, AuthError> {
        let path = &self.config.cache_file_path;
        if !path.exists() {
            return Ok(None);
        }
        let contents = std::fs::read_to_string(path)?;
        let cache: KeyCache = serde_json::from_str(&contents)?;
        Ok(Some(cache))
    }

    fn write_cache(&self, cache: &KeyCache) -> Result<(), AuthError> {
        let path = &self.config.cache_file_path;
        ensure_parent_dir(path)?;
        let json = serde_json::to_string_pretty(cache)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Remove the local cache file.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Io`] if the cache file exists but cannot be removed.
    pub fn clear_cache(&self) -> Result<(), AuthError> {
        let path = &self.config.cache_file_path;
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

fn ensure_parent_dir(path: &Path) -> Result<(), AuthError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}
