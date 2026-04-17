use crate::auth::{AuthConfig, AuthError};
use std::collections::HashSet;
use tracing::{debug, warn};

/// Watches for revoked API keys by polling the /api/revocations endpoint.
pub struct RevocationWatcher {
    config: AuthConfig,
    client: reqwest::Client,
}

impl RevocationWatcher {
    /// Create a new `RevocationWatcher` with the given configuration.
    pub fn new(config: AuthConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self { config, client }
    }

    /// Check if a key prefix is in the local revocation list.
    pub fn is_revoked(&self, key_prefix: &str) -> bool {
        match self.read_revoked_list() {
            Ok(list) => list.contains(key_prefix),
            Err(_) => false,
        }
    }

    /// Poll the revocations endpoint and update the local list.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Http`] if the request fails.
    /// Returns [`AuthError::Json`] if the response cannot be deserialized.
    /// Returns [`AuthError::Io`] if the local revocation file cannot be written.
    pub async fn poll(&self) -> Result<usize, AuthError> {
        let url = format!("{}/api/revocations", self.config.api_base_url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            debug!("Revocation poll returned {}", response.status());
            return Ok(0);
        }

        let data: RevocationResponse = response.json().await?;
        let new_count = data.revoked.len();

        if new_count > 0 {
            let mut existing = self.read_revoked_list().unwrap_or_default();
            for prefix in &data.revoked {
                existing.insert(prefix.clone());
            }
            self.write_revoked_list(&existing)?;
            debug!("Revocation list updated: {} new entries", new_count);
        }

        Ok(new_count)
    }

    /// Start a background polling task. Returns a handle that can be used to stop it.
    pub fn spawn_background_poll(self) -> tokio::task::JoinHandle<()> {
        let interval = self.config.revocation_poll_interval;

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // Skip first immediate tick

            loop {
                ticker.tick().await;
                if let Err(e) = self.poll().await {
                    warn!("Revocation poll failed: {e}");
                }
            }
        })
    }

    /// Remove the local revocation file.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Io`] if the file exists but cannot be removed.
    pub fn clear(&self) -> Result<(), AuthError> {
        let path = &self.config.revoked_file_path;
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    fn read_revoked_list(&self) -> Result<HashSet<String>, AuthError> {
        let path = &self.config.revoked_file_path;
        if !path.exists() {
            return Ok(HashSet::new());
        }
        let contents = std::fs::read_to_string(path)?;
        let list: HashSet<String> = contents
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        Ok(list)
    }

    fn write_revoked_list(&self, list: &HashSet<String>) -> Result<(), AuthError> {
        let path = &self.config.revoked_file_path;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents: String = list.iter().fold(String::new(), |mut acc, s| {
            acc.push_str(s);
            acc.push('\n');
            acc
        });
        std::fs::write(path, contents)?;
        Ok(())
    }
}

#[derive(serde::Deserialize)]
struct RevocationResponse {
    revoked: Vec<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn isolated(dir: &TempDir, api_url: &str) -> AuthConfig {
        AuthConfig {
            api_base_url: api_url.to_string(),
            key_file_path: dir.path().join("la-api-key"),
            cache_file_path: dir.path().join("la-key-cache.json"),
            revoked_file_path: dir.path().join("la-revoked"),
            cache_ttl: std::time::Duration::from_secs(3600),
            refresh_interval: std::time::Duration::from_secs(3000),
            revocation_poll_interval: std::time::Duration::from_secs(300),
            max_grace_resets: 3,
            login_timeout: std::time::Duration::from_secs(60),
        }
    }

    #[test]
    fn is_revoked_false_when_no_list_exists() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, "http://127.0.0.1:1");
        let watcher = RevocationWatcher::new(cfg);
        assert!(!watcher.is_revoked("la-abc123"));
    }

    #[test]
    fn is_revoked_true_for_listed_prefix() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, "http://127.0.0.1:1");
        std::fs::write(&cfg.revoked_file_path, "la-badke\nla-rogue1\n").unwrap();
        let watcher = RevocationWatcher::new(cfg);
        assert!(watcher.is_revoked("la-badke"));
        assert!(watcher.is_revoked("la-rogue1"));
    }

    #[test]
    fn is_revoked_false_for_unlisted_prefix() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, "http://127.0.0.1:1");
        std::fs::write(&cfg.revoked_file_path, "la-badke\n").unwrap();
        let watcher = RevocationWatcher::new(cfg);
        assert!(!watcher.is_revoked("la-goodk1")); // different prefix
    }

    #[test]
    fn clear_removes_revocation_file() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, "http://127.0.0.1:1");
        std::fs::write(&cfg.revoked_file_path, "la-badke\n").unwrap();
        assert!(cfg.revoked_file_path.exists());
        RevocationWatcher::new(cfg.clone()).clear().unwrap();
        assert!(!cfg.revoked_file_path.exists());
    }

    #[test]
    fn clear_is_noop_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, "http://127.0.0.1:1");
        RevocationWatcher::new(cfg).clear().unwrap();
    }

    #[test]
    fn revoked_list_round_trip_via_file() {
        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, "http://127.0.0.1:1");
        let watcher = RevocationWatcher::new(cfg);
        let list: std::collections::HashSet<String> = ["la-prefix1", "la-prefix2"]
            .into_iter()
            .map(String::from)
            .collect();
        watcher.write_revoked_list(&list).unwrap();
        let read = watcher.read_revoked_list().unwrap();
        assert_eq!(read, list);
    }

    #[tokio::test]
    async fn poll_updates_local_revocation_list() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/revocations")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"revoked":["la-badke1","la-badke2"]}"#)
            .create_async()
            .await;

        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, &server.url());
        let watcher = RevocationWatcher::new(cfg);
        let count = watcher.poll().await.unwrap();
        assert_eq!(count, 2);
        assert!(watcher.is_revoked("la-badke1"));
        assert!(watcher.is_revoked("la-badke2"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn poll_merges_with_existing_revocation_list() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/api/revocations")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"revoked":["la-new123"]}"#)
            .create_async()
            .await;

        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, &server.url());
        // Pre-populate with an existing entry
        std::fs::write(&cfg.revoked_file_path, "la-old456\n").unwrap();
        let watcher = RevocationWatcher::new(cfg);
        watcher.poll().await.unwrap();
        // Both entries must be present
        assert!(watcher.is_revoked("la-old456"));
        assert!(watcher.is_revoked("la-new123"));
    }

    #[tokio::test]
    async fn poll_non_success_status_returns_zero() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/api/revocations")
            .with_status(404)
            .create_async()
            .await;

        let dir = TempDir::new().unwrap();
        let cfg = isolated(&dir, &server.url());
        let count = RevocationWatcher::new(cfg).poll().await.unwrap();
        assert_eq!(count, 0);
    }
}
