//! Client and retry configuration types.

use std::time::Duration;

use crate::core::constants::DEFAULT_TIMEOUT_SECS;
use crate::core::error::SdkError;

/// Configuration for an MCP client.
///
/// Construct with [`Config::builder`].
#[derive(Debug, Clone)]
pub struct Config {
    /// Path to the sibling MCP binary.
    pub binary_path: std::path::PathBuf,
    /// Per-call timeout. Defaults to [`DEFAULT_TIMEOUT_SECS`].
    pub timeout: Duration,
    /// Retry policy applied to transient transport errors.
    pub retry: RetryConfig,
}

impl Config {
    /// Create a [`ConfigBuilder`] for this type.
    #[must_use]
    pub fn builder(binary_path: impl Into<std::path::PathBuf>) -> ConfigBuilder {
        ConfigBuilder::new(binary_path)
    }
}

/// Builder for [`Config`].
#[derive(Debug)]
pub struct ConfigBuilder {
    binary_path: std::path::PathBuf,
    timeout: Duration,
    retry: RetryConfig,
}

impl ConfigBuilder {
    /// Create a new builder with the given binary path and default settings.
    #[must_use]
    pub fn new(binary_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            binary_path: binary_path.into(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            retry: RetryConfig::default(),
        }
    }

    /// Override the per-call timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the retry policy.
    #[must_use]
    pub fn retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    /// Consume the builder and produce a [`Config`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if `binary_path` does not point to an
    /// existing file, providing an early actionable error rather than a
    /// spawn failure later.
    pub fn build(self) -> Result<Config, SdkError> {
        // Use `is_file()` rather than `exists()` so that directory paths (e.g.
        // passing `~/lightarchitects/corso/bin/` instead of `~/lightarchitects/corso/bin/corso`) are caught
        // here as a config error rather than being deferred to spawn time where
        // they would burn the full retry budget before failing.
        if !self.binary_path.is_file() {
            return Err(SdkError::Config(format!(
                "binary not found (or is not a file): {}",
                self.binary_path.display()
            )));
        }
        Ok(Config {
            binary_path: self.binary_path,
            timeout: self.timeout,
            retry: self.retry,
        })
    }
}

/// Exponential back-off retry policy for transient transport errors.
///
/// Only [`crate::core::error::TransportError::Timeout`] and
/// [`crate::core::error::TransportError::Io`] are retried; tool errors are never
/// retried because they represent intentional remote failures.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (including the first). Defaults to `3`.
    pub max_attempts: u32,
    /// Base delay before the first retry; doubles on each subsequent attempt.
    /// Defaults to 500 ms.
    pub base_delay: Duration,
    /// Jitter factor in `[0.0, 1.0)`. Scaled by a random value and added to
    /// the computed delay to spread retries. Defaults to `0.75`.
    pub jitter: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            jitter: 0.75,
        }
    }
}

impl RetryConfig {
    /// Compute the delay before the given retry (zero-based retry count, not
    /// counting the initial attempt).
    ///
    /// Formula: `base_delay × 2^retry_count + jitter × rand_factor × base_delay`.
    ///
    /// `rand_factor` must be in `[0.0, 1.0)`.
    #[must_use]
    pub fn delay_for(&self, retry_count: u32, rand_factor: f64) -> Duration {
        let base_ms = u64::try_from(self.base_delay.as_millis()).unwrap_or(u64::MAX);
        let multiplier = 1u64.checked_shl(retry_count).unwrap_or(u64::MAX);
        let backoff_ms = base_ms.saturating_mul(multiplier);
        // Safe casts: `jitter` and `rand_factor` are in [0,1), product bounded by base_ms.
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation
        )]
        let jitter_ms = (self.jitter * rand_factor * base_ms as f64) as u64;
        Duration::from_millis(backoff_ms.saturating_add(jitter_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_retry_values() {
        let r = RetryConfig::default();
        assert_eq!(r.max_attempts, 3);
        assert_eq!(r.base_delay, Duration::from_millis(500));
        assert!((r.jitter - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn delay_increases_with_attempts() {
        let r = RetryConfig::default();
        let d0 = r.delay_for(0, 0.0);
        let d1 = r.delay_for(1, 0.0);
        let d2 = r.delay_for(2, 0.0);
        assert!(d1 > d0, "attempt 1 delay should exceed attempt 0");
        assert!(d2 > d1, "attempt 2 delay should exceed attempt 1");
    }

    #[test]
    fn jitter_adds_to_delay() {
        let r = RetryConfig::default();
        let no_jitter = r.delay_for(0, 0.0);
        let with_jitter = r.delay_for(0, 1.0);
        assert!(with_jitter > no_jitter, "jitter should increase delay");
    }

    #[test]
    fn builder_validates_binary_path() {
        let result = Config::builder("/nonexistent/binary/path").build();
        assert!(result.is_err());
        assert!(matches!(result, Err(SdkError::Config(_))));
    }
}
