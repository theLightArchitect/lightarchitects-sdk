//! SSH session pool -- reuse SSH sessions across multiple calls to amortize
//! connection overhead.
//!
//! Feature-gated behind `feature = "ssh"`.
//!
//! # Design
//!
//! `SessionPool` maintains a bounded vector of pooled sessions protected by a
//! `tokio::sync::Mutex`. Sessions are created on-demand via a user-supplied
//! factory closure and returned to the pool after use. An idle timeout evicts
//! sessions that have not been used recently.
//!
//! # Limits
//!
//! `max_sessions` is capped at 8 to prevent resource exhaustion on the remote
//! host (Khadas Edge 2 Pro has limited SSH concurrency).

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::seraph::ssh::SshSession;
use crate::core::error::SdkError;

/// Hard upper bound for `max_sessions`.
const MAX_SESSIONS_CEILING: usize = 8;

/// Default maximum number of pooled sessions.
const DEFAULT_MAX_SESSIONS: usize = 4;

/// Default idle timeout in seconds (5 minutes).
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 300;

/// Default health-check interval in seconds (1 minute).
const DEFAULT_HEALTH_CHECK_INTERVAL_SECS: u64 = 60;

// ── PoolConfig ──────────────────────────────────────────────────────────────

/// Configuration for a [`SessionPool`].
///
/// Use the builder pattern to customise values:
///
/// ```rust
/// use crate::seraph::pool::PoolConfig;
///
/// let config = PoolConfig::new()
///     .with_max_sessions(6)
///     .with_idle_timeout_secs(120)
///     .with_health_check_interval_secs(30);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Maximum number of sessions the pool will hold (1..=8).
    pub max_sessions: usize,
    /// Seconds a session can sit idle before eviction.
    pub idle_timeout_secs: u64,
    /// Seconds between automatic health-check sweeps.
    pub health_check_interval_secs: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl PoolConfig {
    /// Create a config with default values.
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_sessions: DEFAULT_MAX_SESSIONS,
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
            health_check_interval_secs: DEFAULT_HEALTH_CHECK_INTERVAL_SECS,
        }
    }

    /// Set the maximum number of pooled sessions (clamped to 1..=8).
    ///
    /// Values above 8 are clamped to 8. Values of 0 are raised to 1.
    #[must_use]
    pub fn with_max_sessions(mut self, n: usize) -> Self {
        self.max_sessions = n.clamp(1, MAX_SESSIONS_CEILING);
        self
    }

    /// Set the idle timeout in seconds.
    #[must_use]
    pub fn with_idle_timeout_secs(mut self, secs: u64) -> Self {
        self.idle_timeout_secs = secs;
        self
    }

    /// Set the health-check interval in seconds.
    #[must_use]
    pub fn with_health_check_interval_secs(mut self, secs: u64) -> Self {
        self.health_check_interval_secs = secs;
        self
    }
}

// ── PooledSession ───────────────────────────────────────────────────────────

/// A session with bookkeeping metadata.
struct PooledSession {
    session: SshSession,
    last_used: Instant,
    #[allow(dead_code)]
    healthy: bool,
}

// ── SessionPool ─────────────────────────────────────────────────────────────

/// A bounded pool of [`SshSession`]s.
///
/// Sessions are created lazily via a factory closure. When all sessions are
/// checked out and the pool is at capacity, `acquire` returns an error rather
/// than blocking.
pub struct SessionPool {
    config: PoolConfig,
    sessions: tokio::sync::Mutex<Vec<PooledSession>>,
    factory: Box<dyn Fn() -> Result<SshSession, SdkError> + Send + Sync>,
    /// Number of sessions that have been acquired but not yet released.
    checked_out: std::sync::atomic::AtomicUsize,
}

impl SessionPool {
    /// Create a new pool with the given configuration and session factory.
    ///
    /// The factory is called each time a new session is needed (up to
    /// `config.max_sessions`).
    #[must_use]
    pub fn new(
        config: PoolConfig,
        session_factory: Box<dyn Fn() -> Result<SshSession, SdkError> + Send + Sync>,
    ) -> Self {
        Self {
            config,
            sessions: tokio::sync::Mutex::new(Vec::new()),
            factory: session_factory,
            checked_out: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Acquire a session from the pool.
    ///
    /// Returns an idle session if one is available, otherwise creates a new
    /// one via the factory (up to `max_sessions`). If the pool is exhausted,
    /// returns a config error.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if all sessions are in use.
    /// Returns any error propagated from the session factory.
    pub async fn acquire(&self) -> Result<SshSession, SdkError> {
        let mut pool = self.sessions.lock().await;

        // Try to return an idle session.
        if let Some(entry) = pool.pop() {
            self.checked_out
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(entry.session);
        }

        // No idle sessions -- can we create a new one?
        let in_use = self.checked_out.load(std::sync::atomic::Ordering::Relaxed);
        let total = pool.len().saturating_add(in_use);

        if total >= self.config.max_sessions {
            return Err(SdkError::Config(format!(
                "session pool exhausted (max: {})",
                self.config.max_sessions
            )));
        }

        // Create a new session via the factory.
        let session = (self.factory)()?;
        self.checked_out
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(session)
    }

    /// Return a session to the pool after use.
    pub async fn release(&self, session: SshSession) {
        self.checked_out
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        let mut pool = self.sessions.lock().await;
        pool.push(PooledSession {
            session,
            last_used: Instant::now(),
            healthy: true,
        });
    }

    /// Remove sessions that have been idle longer than `idle_timeout_secs`.
    ///
    /// Evicted sessions are returned so they can be dropped outside of any
    /// async context (tokio runtimes inside `SshSession` cannot be dropped
    /// from within `block_on`).
    pub async fn health_check(&self) -> Vec<SshSession> {
        let mut pool = self.sessions.lock().await;
        let timeout = std::time::Duration::from_secs(self.config.idle_timeout_secs);

        let mut evicted = Vec::new();
        let mut kept = Vec::new();
        for entry in pool.drain(..) {
            if entry.last_used.elapsed() < timeout {
                kept.push(entry);
            } else {
                evicted.push(entry.session);
            }
        }
        *pool = kept;

        if !evicted.is_empty() {
            tracing::warn!(
                removed = evicted.len(),
                "pool health_check: evicted idle sessions"
            );
        }
        evicted
    }

    /// Number of sessions currently tracked (idle + checked out).
    #[must_use]
    pub fn active_count(&self) -> usize {
        let idle = self.sessions.try_lock().map_or(0, |pool| pool.len());
        let in_use = self.checked_out.load(std::sync::atomic::Ordering::Relaxed);
        idle.saturating_add(in_use)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Helper: factory that creates real `SshSession`s pointing at /dev/null.
    fn test_factory() -> Box<dyn Fn() -> Result<SshSession, SdkError> + Send + Sync> {
        Box::new(|| SshSession::new("10.0.0.1", 22, "test", "/dev/null"))
    }

    /// Helper: factory that always fails.
    fn failing_factory() -> Box<dyn Fn() -> Result<SshSession, SdkError> + Send + Sync> {
        Box::new(|| Err(SdkError::Config("factory error".into())))
    }

    /// Helper: create a runtime for pool tests.
    fn pool_rt() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().unwrap()
    }

    #[test]
    fn pool_config_defaults() {
        let config = PoolConfig::new();
        assert_eq!(config.max_sessions, 4);
        assert_eq!(config.idle_timeout_secs, 300);
        assert_eq!(config.health_check_interval_secs, 60);
    }

    #[test]
    fn pool_config_builder() {
        let config = PoolConfig::new()
            .with_max_sessions(6)
            .with_idle_timeout_secs(120)
            .with_health_check_interval_secs(30);
        assert_eq!(config.max_sessions, 6);
        assert_eq!(config.idle_timeout_secs, 120);
        assert_eq!(config.health_check_interval_secs, 30);
    }

    #[test]
    fn pool_config_max_validation() {
        let config = PoolConfig::new().with_max_sessions(16);
        assert_eq!(config.max_sessions, 8);

        let config = PoolConfig::new().with_max_sessions(0);
        assert_eq!(config.max_sessions, 1);
    }

    #[test]
    fn pool_acquire_creates_session() {
        let rt = pool_rt();
        let pool = SessionPool::new(PoolConfig::new(), test_factory());
        let session = rt.block_on(pool.acquire());
        assert!(
            session.is_ok(),
            "acquire should succeed: {:?}",
            session.err()
        );
        assert_eq!(pool.active_count(), 1);
    }

    #[test]
    fn pool_release_and_reuse() {
        let rt = pool_rt();
        let factory_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls = factory_calls.clone();
        let factory: Box<dyn Fn() -> Result<SshSession, SdkError> + Send + Sync> =
            Box::new(move || {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                SshSession::new("10.0.0.1", 22, "test", "/dev/null")
            });

        let pool = SessionPool::new(PoolConfig::new().with_max_sessions(2), factory);

        let session = rt.block_on(pool.acquire()).unwrap();
        rt.block_on(pool.release(session));

        let _session = rt.block_on(pool.acquire()).unwrap();
        assert_eq!(
            factory_calls.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "factory should only be called once (session was reused)"
        );
    }

    #[test]
    fn pool_exhausted_returns_error() {
        let rt = pool_rt();
        let config = PoolConfig::new().with_max_sessions(1);
        let pool = SessionPool::new(config, test_factory());

        let _session = rt.block_on(pool.acquire()).unwrap();

        let result = rt.block_on(pool.acquire());
        assert!(result.is_err(), "should fail when pool is exhausted");
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("pool exhausted"),
            "error should mention 'pool exhausted': {err}"
        );
    }

    #[test]
    fn pool_health_check_removes_expired() {
        let rt = pool_rt();
        let config = PoolConfig::new()
            .with_max_sessions(4)
            .with_idle_timeout_secs(0);
        let pool = SessionPool::new(config, test_factory());

        let session = rt.block_on(pool.acquire()).unwrap();
        rt.block_on(pool.release(session));
        assert_eq!(pool.active_count(), 1);

        let evicted = rt.block_on(pool.health_check());
        drop(evicted);

        let idle = rt.block_on(pool.sessions.lock()).len();
        assert_eq!(idle, 0, "expired session should have been evicted");
    }

    #[test]
    fn pool_factory_error_propagates() {
        let rt = pool_rt();
        let pool = SessionPool::new(PoolConfig::new(), failing_factory());
        let result = rt.block_on(pool.acquire());
        assert!(result.is_err(), "factory error should propagate");
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("factory error"),
            "should contain factory error message: {err}"
        );
    }

    #[test]
    fn active_count_tracks_sessions() {
        let rt = pool_rt();
        let pool = SessionPool::new(PoolConfig::new().with_max_sessions(4), test_factory());

        assert_eq!(pool.active_count(), 0);

        let s1 = rt.block_on(pool.acquire()).unwrap();
        assert_eq!(pool.active_count(), 1);

        let s2 = rt.block_on(pool.acquire()).unwrap();
        assert_eq!(pool.active_count(), 2);

        rt.block_on(pool.release(s1));
        assert_eq!(pool.active_count(), 2);

        rt.block_on(pool.release(s2));
        assert_eq!(pool.active_count(), 2);
    }
}
