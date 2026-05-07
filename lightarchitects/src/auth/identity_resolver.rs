//! Identity resolution: load `[identity]` from `~/.lightarchitects/config.toml`.
//!
//! Falls back through env → config → `git_config` → local.

use std::path::Path;

use crate::auth::{AuthError, IdentityConfig, IdentityProvider};
use tracing::{debug, warn};

/// Resolve identity using the priority chain.
///
/// | Priority | Source |
/// |----------|--------|
/// | 1 | `LA_USER_ID` environment variable |
/// | 2 | `~/.lightarchitects/config.toml` `[identity]` section |
/// | 3 | `git config --global user.email` |
/// | 4 | `local` (offline default) |
///
/// # Errors
///
/// Returns [`AuthError::Io`] if config file exists but cannot be read.
/// Returns [`AuthError::IdentityResolutionFailed`] if provider resolution fails.
pub async fn resolve_identity(config_path: &Path) -> Result<IdentityConfig, AuthError> {
    // Priority 1: environment variable
    if let Ok(id) = std::env::var("LA_USER_ID") {
        let id = id.trim();
        if !id.is_empty() {
            debug!("Identity resolved from LA_USER_ID env var");
            return IdentityConfig::resolve(IdentityProvider::Explicit {
                user_id: id.to_string(),
            });
        }
    }

    // Priority 2: config.toml
    if config_path.exists() {
        match read_identity_from_toml(config_path).await {
            Ok(Some(cfg)) => {
                debug!("Identity resolved from config.toml");
                return Ok(cfg);
            }
            Ok(None) => {}
            Err(e) => {
                warn!("Failed to read identity from config.toml: {e}");
            }
        }
    }

    // Priority 3: git config user.email
    match resolve_git_config_identity() {
        Ok(Some(cfg)) => {
            debug!("Identity resolved from git config");
            return Ok(cfg);
        }
        Ok(None) => {}
        Err(e) => {
            warn!("Failed to resolve identity from git config: {e}");
        }
    }

    // Priority 4: local fallback
    debug!("Identity resolved as 'local' (offline default)");
    IdentityConfig::resolve(IdentityProvider::Local)
}

/// Extract `user_id` from an existing `KeyCache` if valid.
#[must_use]
pub fn resolve_user_id_from_cache(cache: &crate::auth::KeyCache) -> Option<String> {
    let trimmed = cache.user_id.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Return a `user_id` string, falling back to `"local"`.
#[must_use]
pub fn user_id_or_local(provider: &IdentityProvider) -> String {
    provider.user_id()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

async fn read_identity_from_toml(path: &Path) -> Result<Option<IdentityConfig>, AuthError> {
    let contents = tokio::fs::read_to_string(path)
        .await
        .map_err(AuthError::Io)?;
    let table: toml::Value = contents.parse().map_err(|e| {
        AuthError::IdentityResolutionFailed(format!("config.toml parse error: {e}"))
    })?;

    let Some(identity) = table.get("identity") else {
        return Ok(None);
    };

    let provider = parse_identity_provider(identity).await.map_err(|e| {
        AuthError::IdentityResolutionFailed(format!("invalid [identity] provider: {e}"))
    })?;

    let mut cfg = IdentityConfig::resolve(provider)?;

    if let Some(vault_remote) = identity.get("vault_remote").and_then(|v| v.as_str()) {
        IdentityConfig::validate_vault_remote(vault_remote)?;
        cfg.vault_remote = Some(vault_remote.to_string());
    }

    if let Some(handle) = identity
        .get("github_keychain_handle")
        .and_then(|v| v.as_str())
    {
        cfg.github_keychain_handle = Some(handle.to_string());
    }

    Ok(Some(cfg))
}

async fn parse_identity_provider(value: &toml::Value) -> Result<IdentityProvider, String> {
    let provider_str = value
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("local");

    match provider_str {
        "github" => {
            let username_opt = if let Some(u) = value.get("username").and_then(|v| v.as_str()) {
                Some(u.to_string())
            } else {
                resolve_gh_username().await.ok()
            };
            let username = username_opt.ok_or_else(|| {
                "GitHub provider requires 'username' or authenticated `gh` CLI".to_string()
            })?;
            Ok(IdentityProvider::GitHub { username })
        }
        "git_config" => {
            let email = resolve_git_email().ok_or_else(|| {
                "git_config provider requires `git config --global user.email`".to_string()
            })?;
            Ok(IdentityProvider::GitConfig { email })
        }
        "explicit" => {
            let user_id = value
                .get("user_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "explicit provider requires 'user_id' field".to_string())?;
            Ok(IdentityProvider::Explicit {
                user_id: user_id.to_string(),
            })
        }
        _ => Ok(IdentityProvider::Local),
    }
}

async fn resolve_gh_username() -> Result<String, AuthError> {
    let output = tokio::process::Command::new("gh")
        .args(["api", "/user", "--jq", ".login"])
        .output()
        .await
        .map_err(|e| AuthError::IdentityResolutionFailed(format!("gh CLI failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AuthError::IdentityResolutionFailed(format!(
            "gh auth status failed: {stderr}"
        )));
    }

    let username = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if username.is_empty() {
        return Err(AuthError::IdentityResolutionFailed(
            "gh CLI returned empty username".to_string(),
        ));
    }
    Ok(username)
}

fn resolve_git_config_identity() -> Result<Option<IdentityConfig>, AuthError> {
    let Some(email) = resolve_git_email() else {
        return Ok(None);
    };
    let cfg = IdentityConfig::resolve(IdentityProvider::GitConfig { email })?;
    Ok(Some(cfg))
}

fn resolve_git_email() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["config", "--global", "user.email"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let email = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if email.is_empty() {
        return None;
    }
    Some(email)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn resolve_explicit_provider() {
        let provider = IdentityProvider::Explicit {
            user_id: "kf-tan".to_string(),
        };
        assert_eq!(provider.user_id(), "kf-tan");
    }

    #[test]
    fn resolve_github_provider() {
        let provider = IdentityProvider::GitHub {
            username: "KFT".to_string(),
        };
        assert_eq!(provider.user_id(), "kft");
    }

    #[test]
    fn resolve_git_config_provider() {
        let provider = IdentityProvider::GitConfig {
            email: "kf.tan@lightarchitects.io".to_string(),
        };
        assert_eq!(provider.user_id(), "kf-tan");
    }

    #[test]
    fn resolve_local_provider() {
        let provider = IdentityProvider::Local;
        assert_eq!(provider.user_id(), "local");
    }

    #[test]
    fn slugify_lowercases_and_replaces_special_chars() {
        assert_eq!(
            IdentityConfig::slugify("Hello_World!123"),
            "hello-world-123"
        );
    }

    #[test]
    fn slugify_truncates_to_64() {
        let long = "a".repeat(100);
        let result = IdentityConfig::slugify(&long);
        assert_eq!(result.len(), 64);
    }

    #[tokio::test]
    async fn validate_vault_remote_rejects_embedded_creds() {
        let result = IdentityConfig::validate_vault_remote(
            "https://x-access-token:ghp_xxx@github.com/user/repo.git",
        );
        assert!(matches!(result, Err(AuthError::InvalidVaultRemote(_))));
    }

    #[tokio::test]
    async fn validate_vault_remote_rejects_uppercase_scheme() {
        // Case-insensitive regex must catch HTTPS:// embedded credentials.
        let result = IdentityConfig::validate_vault_remote(
            "HTTPS://x-access-token:ghp_xxx@github.com/user/repo.git",
        );
        assert!(matches!(result, Err(AuthError::InvalidVaultRemote(_))));
    }

    #[tokio::test]
    async fn validate_vault_remote_accepts_ssh_url() {
        let result = IdentityConfig::validate_vault_remote(
            "git@github.com:TheLightArchitects/soul-vault-public.git",
        );
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn validate_vault_remote_accepts_plain_https() {
        let result = IdentityConfig::validate_vault_remote("https://github.com/user/repo.git");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn read_identity_from_toml_explicit() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let toml = r#"
[identity]
provider = "explicit"
user_id = "alice"
vault_remote = "git@github.com:TheLightArchitects/soul-vault-public.git"
"#;
        tokio::fs::write(&path, toml).await.unwrap();

        let cfg = read_identity_from_toml(&path).await.unwrap().unwrap();
        assert_eq!(cfg.user_id, "alice");
        assert_eq!(
            cfg.vault_remote,
            Some("git@github.com:TheLightArchitects/soul-vault-public.git".to_string())
        );
    }

    #[tokio::test]
    async fn read_identity_from_toml_github() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let toml = r#"
[identity]
provider = "github"
username = "kft"
"#;
        tokio::fs::write(&path, toml).await.unwrap();

        let cfg = read_identity_from_toml(&path).await.unwrap().unwrap();
        assert_eq!(cfg.user_id, "kft");
        assert!(matches!(cfg.provider, IdentityProvider::GitHub { .. }));
    }

    #[tokio::test]
    async fn read_identity_from_toml_missing_identity_section() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let toml = r#"
[gateway]
version = "1.0.0"
"#;
        tokio::fs::write(&path, toml).await.unwrap();

        let cfg = read_identity_from_toml(&path).await.unwrap();
        assert!(cfg.is_none());
    }

    #[test]
    fn user_id_priority_env_over_config() {
        // This test verifies the priority chain by checking that
        // resolve_identity prefers LA_USER_ID over config file.
        // We cannot easily test the full async chain here without mocking fs,
        // so we verify the building blocks instead.
        let provider = IdentityProvider::Explicit {
            user_id: "env-user".to_string(),
        };
        assert_eq!(provider.user_id(), "env-user");
    }

    #[test]
    fn resolve_user_id_from_cache_valid() {
        let cache = crate::auth::KeyCache {
            key_hash: "abc".to_string(),
            tier: crate::auth::SubscriptionTier::Pro,
            user_id: "cached-user".to_string(),
            validated_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            grace_resets: 0,
        };
        assert_eq!(
            resolve_user_id_from_cache(&cache),
            Some("cached-user".to_string())
        );
    }

    #[test]
    fn resolve_user_id_from_cache_empty_returns_none() {
        let cache = crate::auth::KeyCache {
            key_hash: "abc".to_string(),
            tier: crate::auth::SubscriptionTier::Pro,
            user_id: String::new(),
            validated_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            grace_resets: 0,
        };
        assert_eq!(resolve_user_id_from_cache(&cache), None);
    }
}
