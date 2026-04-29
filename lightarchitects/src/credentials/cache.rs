//! 30-second TTL cache wrapper.
//!
//! Mirrors Claude Code's `KEYCHAIN_CACHE_TTL_MS = 30_000` exactly. Caching
//! serves two purposes:
//!
//! 1. **Subprocess bounding** — a keychain `security(1)` spawn costs
//!    ~500ms on cold-path. Under 50+ concurrent callers (plausible when
//!    the webshell boots a fleet of MCP sessions) we'd otherwise thrash.
//! 2. **Unlock-prompt suppression** — macOS surfaces a keychain-unlock
//!    dialog on locked-keychain queries. A 30s window means the user
//!    sees at most one dialog per window, never a burst.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::credentials::types::Detection;

const TTL: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct Entry {
    detection: Detection,
    fetched_at: Instant,
}

/// Provider-scoped cache. Safe for concurrent use.
#[derive(Clone, Default)]
pub(crate) struct DetectionCache {
    inner: Arc<Mutex<Option<Entry>>>,
}

impl DetectionCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn get_or_refresh<F, Fut>(&self, refresh: F) -> Detection
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Detection>,
    {
        let mut guard = self.inner.lock().await;
        if let Some(e) = guard.as_ref()
            && e.fetched_at.elapsed() < TTL
        {
            return e.detection.clone();
        }
        let detection = refresh().await;
        *guard = Some(Entry {
            detection: detection.clone(),
            fetched_at: Instant::now(),
        });
        detection
    }
}
