use crate::core::SdkError;
use crate::core::auth::{AuthProvider, AuthStatus};

use crate::auth::{
    AuthConfig, AuthError, AuthTier, KeyCache, KeyReader, KeyValidator, RevocationWatcher,
};
use tracing::{error, info, warn};

/// Orchestrates the full 3-tier auth degradation flow.
///
/// Combines `KeyReader`, `KeyValidator`, and `RevocationWatcher` into a single
/// startup check. Returns the tier and cached key info on success.
///
/// ## Tiers
///
/// - **A (`NoKey`)**: No key found → BLOCK (server should refuse to start)
/// - **B (`GracePeriod`)**: Expired cache + endpoint down → WARN (limited resets)
/// - **C (Valid)**: Fresh cache or live validation → ALLOW
pub struct AuthGuard {
    config: AuthConfig,
    validator: KeyValidator,
    revocation: RevocationWatcher,
}

impl AuthGuard {
    /// Create a new [`AuthGuard`] with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Http`] if the HTTP client cannot be constructed.
    pub fn new(config: AuthConfig) -> Result<Self, AuthError> {
        let validator = KeyValidator::new(config.clone())?;
        let revocation = RevocationWatcher::new(config.clone());
        Ok(Self {
            config,
            validator,
            revocation,
        })
    }

    /// Run the full auth check. Call this at MCP server startup.
    ///
    /// Returns `(AuthTier, KeyCache)` on success (tiers B and C),
    /// or `AuthError` on hard failure (tier A or exhausted grace).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::NoKeyFound`] if no API key is present.
    /// Returns [`AuthError::KeyRevoked`] if the key is on the revocation list.
    /// Returns [`AuthError::ValidationFailed`] if the server rejects the key.
    /// Returns [`AuthError::GraceExhausted`] if grace period resets are exhausted.
    pub async fn check(&self) -> Result<(AuthTier, KeyCache), AuthError> {
        // Step 1: Read the key
        let key = match KeyReader::read(&self.config) {
            Ok(k) => k,
            Err(e) => {
                error!("No API key found: {e}");
                return Err(e);
            }
        };

        // Step 2: Check revocation list (local, fast)
        let key_prefix = if key.len() >= 8 { &key[..8] } else { &key };
        if self.revocation.is_revoked(key_prefix) {
            error!("API key has been revoked (prefix: {key_prefix})");
            return Err(AuthError::KeyRevoked);
        }

        // Step 3: Validate (uses cache, calls endpoint if needed, handles grace)
        let (tier, cache) = self.validator.validate(&key).await?;

        match tier {
            AuthTier::Valid => {
                info!(
                    "Auth OK — tier: {}, user: {}, expires: {}",
                    cache.tier, cache.user_id, cache.expires_at
                );
            }
            AuthTier::GracePeriod { resets_remaining } => {
                warn!(
                    "Auth DEGRADED — grace period active ({resets_remaining} resets remaining). \
                     Validation endpoint unreachable."
                );
            }
            AuthTier::NoKey => {
                // Shouldn't reach here (caught in Step 1), but handle defensively
                error!("No API key — server should not start");
            }
        }

        Ok((tier, cache))
    }

    /// Start background tasks: key refresh + revocation polling.
    ///
    /// Call after a successful `check()`. Returns join handles that can be
    /// aborted on shutdown.
    pub fn spawn_background_tasks(
        self,
    ) -> (tokio::task::JoinHandle<()>, tokio::task::JoinHandle<()>) {
        let Self {
            config,
            validator: _,
            revocation,
        } = self;
        let refresh_handle = Self::spawn_key_refresh(config.clone());
        let revocation_handle = revocation.spawn_background_poll();
        (refresh_handle, revocation_handle)
    }

    fn spawn_key_refresh(config: AuthConfig) -> tokio::task::JoinHandle<()> {
        let interval = config.refresh_interval;

        tokio::spawn(async move {
            let validator = match KeyValidator::new(config.clone()) {
                Ok(v) => v,
                Err(e) => {
                    warn!("Background key refresh — failed to create validator: {e}");
                    return;
                }
            };
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // skip first immediate tick

            loop {
                ticker.tick().await;
                match KeyReader::read(&config) {
                    Ok(key) => match validator.validate(&key).await {
                        Ok((tier, _cache)) => {
                            info!("Background key refresh: {tier:?}");
                        }
                        Err(e) => {
                            warn!("Background key refresh failed: {e}");
                        }
                    },
                    Err(e) => {
                        warn!("Background key refresh — key read failed: {e}");
                    }
                }
            }
        })
    }

    /// Print auth status to stdout. Used by `{binary} auth status`.
    pub async fn print_status(config: &AuthConfig) {
        println!("Light Architects Auth Status");
        println!("────────────────────────────");

        // Key presence
        if let Ok(key) = KeyReader::read(config) {
            let prefix = if key.len() >= 8 { &key[..8] } else { &key };
            println!("Key:       found (prefix: {prefix}...)");
            println!("Key file:  {}", config.key_file_path.display());

            // Check cache
            let validator = match KeyValidator::new(config.clone()) {
                Ok(v) => v,
                Err(e) => {
                    println!("Status:    ERROR — failed to initialise validator: {e}");
                    return;
                }
            };
            match validator.validate(&key).await {
                Ok((tier, cache)) => {
                    println!("Tier:      {}", cache.tier);
                    println!("User ID:   {}", cache.user_id);
                    println!("Validated: {}", cache.validated_at);
                    println!("Expires:   {}", cache.expires_at);
                    match tier {
                        AuthTier::Valid => println!("Status:    VALID"),
                        AuthTier::GracePeriod { resets_remaining } => {
                            println!(
                                "Status:    GRACE PERIOD ({resets_remaining} resets remaining)"
                            );
                        }
                        AuthTier::NoKey => println!("Status:    NO KEY"),
                    }
                }
                Err(e) => {
                    println!("Status:    ERROR — {e}");
                }
            }
        } else {
            println!("Key:       NOT FOUND");
            println!("Key file:  {}", config.key_file_path.display());
            println!("Status:    NOT AUTHENTICATED");
            println!();
            println!("Run `{{binary}} auth login` to authenticate.");
        }
    }

    /// Remove local auth state. Used by `{binary} auth logout`.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Io`] if the key file, cache file, or revocation file cannot be removed.
    pub fn logout(config: &AuthConfig) -> Result<(), AuthError> {
        KeyReader::remove(config)?;

        // Also clear cache and revocation list
        let validator = KeyValidator::new(config.clone())?;
        validator.clear_cache()?;

        let watcher = RevocationWatcher::new(config.clone());
        watcher.clear()?;

        info!("Logged out — key, cache, and revocation list cleared");
        println!("Logged out successfully.");
        Ok(())
    }
}

// ── AuthProvider impl ─────────────────────────────────────────────────────────

/// `AuthGuard` implements [`AuthProvider`] from `lightarchitects-core`.
///
/// Maps the three-tier degradation model onto the SDK's two-outcome model:
///
/// | `AuthTier`     | `AuthStatus` returned                |
/// |----------------|--------------------------------------|
/// | `Valid`        | `AuthStatus::Valid`                  |
/// | `GracePeriod`  | `AuthStatus::Degraded { message }`   |
/// | `NoKey`        | `Err(SdkError::Auth(...))`           |
///
/// Key revocation and validation failure also map to `Err(SdkError::Auth)`.
impl AuthProvider for AuthGuard {
    async fn check_connect(&self) -> Result<AuthStatus, SdkError> {
        match self.check().await {
            Ok((AuthTier::Valid, cache)) => {
                info!(
                    user_id = %cache.user_id,
                    tier = %cache.tier,
                    expires = %cache.expires_at,
                    "auth check passed"
                );
                Ok(AuthStatus::Valid)
            }
            Ok((AuthTier::GracePeriod { resets_remaining }, cache)) => {
                warn!(
                    user_id = %cache.user_id,
                    resets_remaining,
                    "auth degraded — validation endpoint unreachable, grace period active"
                );
                Ok(AuthStatus::Degraded {
                    message: format!(
                        "validation endpoint unreachable — grace period active \
                         ({resets_remaining} resets remaining)"
                    ),
                })
            }
            Ok((AuthTier::NoKey, _)) => Err(SdkError::Auth("no API key found".to_owned())),
            Err(AuthError::NoKeyFound { path }) => {
                Err(SdkError::Auth(format!("no API key found (checked {path})")))
            }
            Err(AuthError::KeyRevoked) => {
                Err(SdkError::Auth("API key has been revoked".to_owned()))
            }
            Err(AuthError::GraceExhausted { .. }) => Err(SdkError::Auth(
                "auth grace period exhausted — re-authenticate to continue".to_owned(),
            )),
            Err(e) => Err(SdkError::Auth(e.to_string())),
        }
    }
}
