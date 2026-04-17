use crate::auth::{AuthConfig, AuthError, AuthTier, SubscriptionTier};
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::auth::{AuthTier, SubscriptionTier};
    use tempfile::TempDir;

    fn isolated(dir: &TempDir, api_url: &str) -> AuthConfig {
        AuthConfig {
            api_base_url: api_url.to_string(),
            key_file_path: dir.path().join("la-api-key"),
            cache_file_path: dir.path().join("la-key-cache.json"),
            revoked_file_path: dir.path().join("la-revoked"),
            max_grace_resets: 2,
            cache_ttl: std::time::Duration::from_secs(3600),
            refresh_interval: std::time::Duration::from_secs(3000),
            revocation_poll_interval: std::time::Duration::from_secs(300),
            login_timeout: std::time::Duration::from_secs(60),
        }
    }

    fn make_cache(key: &str, hours_until_expiry: i64) -> KeyCache {
        KeyCache {
            key_hash: KeyValidator::hash_key(key),
            tier: SubscriptionTier::Pro,
            user_id: "test-user".to_string(),
            validated_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(hours_until_expiry),
            grace_resets: 0,
        }
    }

    fn write_cache_file(config: &AuthConfig, cache: &KeyCache) {
        let json = serde_json::to_string_pretty(cache).unwrap();
        std::fs::write(&config.cache_file_path, json).unwrap();
    }

    #[test]
    fn hash_key_is_deterministic() {
        assert_eq!(
            KeyValidator::hash_key("la-abc123"),
            KeyValidator::hash_key("la-abc123")
        );
    }

    #[test]
    fn hash_key_differs_for_different_inputs() {
        assert_ne!(
            KeyValidator::hash_key("key-a"),
            KeyValidator::hash_key("key-b")
        );
    }

    #[test]
    fn hash_key_is_64_hex_chars() {
        let h = KeyValidator::hash_key("test");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn clear_cache_removes_file() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, "http://127.0.0.1:1");
        let cache = make_cache("k", 1);
        write_cache_file(&cfg, &cache);
        assert!(cfg.cache_file_path.exists());
        KeyValidator::new(cfg.clone()).clear_cache().unwrap();
        assert!(!cfg.cache_file_path.exists());
    }

    #[test]
    fn clear_cache_noop_when_absent() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, "http://127.0.0.1:1");
        KeyValidator::new(cfg).clear_cache().unwrap();
    }

    #[test]
    fn cache_write_read_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, "http://127.0.0.1:1");
        let validator = KeyValidator::new(cfg);
        let cache = make_cache("round-trip", 1);
        validator.write_cache(&cache).unwrap();
        let read = validator.read_cache().unwrap().unwrap();
        assert_eq!(read.key_hash, cache.key_hash);
        assert_eq!(read.user_id, "test-user");
    }

    #[tokio::test]
    async fn validate_hits_fresh_cache_without_network() {
        let dir = TempDir::new().unwrap();
        // Port 1 is unreachable — if we hit the network the test errors immediately
        let cfg = isolated(&dir, "http://127.0.0.1:1");
        let key = "la-cached-key";
        write_cache_file(&cfg, &make_cache(key, 1)); // fresh cache

        let (tier, cache) = KeyValidator::new(cfg).validate(key).await.unwrap();
        assert_eq!(tier, AuthTier::Valid);
        assert_eq!(cache.user_id, "test-user");
    }

    #[tokio::test]
    async fn validate_refreshes_expired_cache_via_endpoint() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/validate-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"valid":true,"tier":"pro","user_id":"fresh-user"}"#)
            .create_async()
            .await;

        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, &server.url());
        let key = "la-expired-key";
        write_cache_file(&cfg, &make_cache(key, -1)); // expired (-1 hour)

        let (tier, cache) = KeyValidator::new(cfg).validate(key).await.unwrap();
        assert_eq!(tier, AuthTier::Valid);
        assert_eq!(cache.user_id, "fresh-user");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn validate_returns_err_for_invalid_key() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/api/validate-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"valid":false,"error":"Invalid API key"}"#)
            .create_async()
            .await;

        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, &server.url());
        let result = KeyValidator::new(cfg).validate("bad-key").await;
        assert!(matches!(result, Err(AuthError::ValidationFailed(_))));
    }

    #[tokio::test]
    async fn grace_period_granted_when_endpoint_returns_non_json() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/api/validate-key")
            .with_status(503)
            .with_body("Service Unavailable")
            .create_async()
            .await;

        let dir = TempDir::new().unwrap();
        let key = "la-grace-key";
        let mut cfg = isolated(&dir, &server.url());
        cfg.max_grace_resets = 2;
        // Write an expired cache — grace period requires existing cache
        write_cache_file(&cfg, &make_cache(key, -1));

        let (tier, _) = KeyValidator::new(cfg).validate(key).await.unwrap();
        // grace_resets was 0, incremented to 1, resets_remaining = 2 - 1 = 1
        assert!(matches!(
            tier,
            AuthTier::GracePeriod {
                resets_remaining: 1
            }
        ));
    }

    #[tokio::test]
    async fn grace_exhausted_when_max_resets_consumed() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/api/validate-key")
            .with_status(503)
            .with_body("Service Unavailable")
            .create_async()
            .await;

        let dir = TempDir::new().unwrap();
        let key = "la-exhausted-key";
        let mut cfg = isolated(&dir, &server.url());
        cfg.max_grace_resets = 1;
        // Cache already consumed all allowed grace resets
        let cache = KeyCache {
            key_hash: KeyValidator::hash_key(key),
            tier: SubscriptionTier::Free,
            user_id: "user".to_string(),
            validated_at: Utc::now() - chrono::Duration::hours(3),
            expires_at: Utc::now() - chrono::Duration::hours(1),
            grace_resets: 1, // == max_grace_resets → exhausted
        };
        write_cache_file(&cfg, &cache);

        let result = KeyValidator::new(cfg).validate(key).await;
        assert!(matches!(result, Err(AuthError::GraceExhausted { max: 1 })));
    }

    #[tokio::test]
    async fn no_grace_without_any_cache() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/api/validate-key")
            .with_status(503)
            .with_body("Service Unavailable")
            .create_async()
            .await;

        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, &server.url());
        // No cache file exists — grace period requires prior cache
        let result = KeyValidator::new(cfg).validate("brand-new-key").await;
        // Must fail, but NOT with GraceExhausted (no prior validation to grace)
        assert!(result.is_err());
        assert!(!matches!(result, Err(AuthError::GraceExhausted { .. })));
    }
}
