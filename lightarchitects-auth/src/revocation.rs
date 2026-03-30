use crate::{AuthConfig, AuthError};
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
