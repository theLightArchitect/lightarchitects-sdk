//! Global event ring buffer with `SSE` fan-out and disk persistence.
//!
//! [`GlobalEventStore`] stores the last 1,000 [`GlobalEventEntry`] entries in a
//! ring buffer and broadcasts each entry to all connected `SSE` subscribers via a
//! [`tokio::sync::broadcast`] channel. A background task debounces writes to
//! `$config_dir/events.ndjson` so the ring survives server restarts up to the
//! 50 MiB rotation limit.
//!
//! # Consumer-side filtering
//!
//! Filtering is intentionally applied at the subscriber, not at publish time.
//! The ring buffer stores all events unfiltered so that new subscribers can
//! replay history without knowledge of prior filter state. Each `SSE` handler
//! wraps the receiver in an `EventFilter`-aware adapter (see `sse_handler.rs`).
//!
//! # Reconnect resume
//!
//! Clients include `Last-Event-ID: <seq>` on reconnect. The `SSE` handler calls
//! [`GlobalEventStore::snapshot_from`] to replay entries with `seq > last_seq`
//! before switching to live streaming. Entries beyond the 1,000-entry cap are
//! unrecoverable; the handler emits `event: lag` instead.

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use tokio::sync::{broadcast, mpsc};

use crate::events::types::{EventFilter, EventSource, GlobalEventEntry, WebEvent};

/// Capacity of the broadcast channel (number of unread messages per subscriber
/// before lag occurs).
const BROADCAST_CAP: usize = 256;

/// Maximum entries kept in the ring buffer.
pub const RING_CAP: usize = 1_000;

/// Background persist task wakes up after this many milliseconds of idle time.
const PERSIST_DEBOUNCE_MS: u64 = 500;

/// Maximum size of `events.ndjson` before rotation (50 MiB).
const PERSIST_MAX_BYTES: u64 = 50 * 1_024 * 1_024;

/// Shared global event store injected into [`crate::server::AppState`].
///
/// Cheap to clone — all state lives behind [`Arc`]s.
#[derive(Clone)]
pub struct GlobalEventStore {
    sender: broadcast::Sender<Arc<GlobalEventEntry>>,
    ring: Arc<std::sync::Mutex<VecDeque<Arc<GlobalEventEntry>>>>,
    next_seq: Arc<AtomicU64>,
    persist_tx: mpsc::UnboundedSender<Arc<GlobalEventEntry>>,
}

impl std::fmt::Debug for GlobalEventStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlobalEventStore")
            .field("next_seq", &self.next_seq.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl GlobalEventStore {
    /// Create a new store and spawn the background disk-persistence task.
    ///
    /// `persist_path` should be a writable file path (e.g.
    /// `$data_dir/events.ndjson`). Pass `None` to disable disk persistence.
    ///
    /// **Requires a Tokio runtime** — call from an `async` context or a thread
    /// already inside `tokio::Runtime`. Use [`GlobalEventStore::noop`] in
    /// synchronous unit tests.
    pub fn new(persist_path: Option<PathBuf>) -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_CAP);
        let ring = Arc::new(std::sync::Mutex::new(VecDeque::with_capacity(RING_CAP)));
        let next_seq = Arc::new(AtomicU64::new(1));
        let (persist_tx, persist_rx) = mpsc::unbounded_channel();

        if let Some(path) = persist_path {
            tokio::spawn(persist_task(persist_rx, path));
        } else {
            tokio::spawn(drain_task(persist_rx));
        }

        Self {
            sender,
            ring,
            next_seq,
            persist_tx,
        }
    }

    /// Create a no-op store that spawns no background tasks.
    ///
    /// Safe to call outside a Tokio runtime. Persist sends are silently
    /// discarded (the `persist_rx` is dropped immediately). Broadcast and ring
    /// buffer remain fully functional for in-process subscribers.
    ///
    /// Intended for use in synchronous unit tests via [`AppState::for_test`].
    #[must_use]
    pub fn noop() -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_CAP);
        let ring = Arc::new(std::sync::Mutex::new(VecDeque::with_capacity(RING_CAP)));
        let next_seq = Arc::new(AtomicU64::new(1));
        let (persist_tx, _persist_rx) = mpsc::unbounded_channel();
        // _persist_rx is dropped here; persist_tx.send() silently errors (ignored in push()).
        Self {
            sender,
            ring,
            next_seq,
            persist_tx,
        }
    }

    /// Push an event from `source` into the store.
    ///
    /// Assigns a monotone sequence number, timestamps the entry, appends it to
    /// the ring buffer (evicting the oldest if at capacity), broadcasts it to all
    /// live subscribers, and queues it for disk persistence.
    ///
    /// Returns the stored entry (useful for tests and in-process consumers).
    pub fn push(&self, source: EventSource, event: WebEvent) -> Arc<GlobalEventEntry> {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let entry = Arc::new(GlobalEventEntry {
            seq,
            timestamp: chrono::Utc::now(),
            source,
            event,
        });

        {
            let mut ring = self
                .ring
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if ring.len() >= RING_CAP {
                ring.pop_front();
            }
            ring.push_back(Arc::clone(&entry));
        }

        let _ = self.sender.send(Arc::clone(&entry));
        let _ = self.persist_tx.send(Arc::clone(&entry));

        entry
    }

    /// Subscribe to live events.
    ///
    /// Lag (more than [`BROADCAST_CAP`] unread messages) causes skipped events;
    /// the subscriber should detect `RecvError::Lagged` and request a snapshot.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<GlobalEventEntry>> {
        self.sender.subscribe()
    }

    /// Return a snapshot of all entries currently in the ring, newest-last.
    pub fn snapshot(&self) -> Vec<Arc<GlobalEventEntry>> {
        let ring = self
            .ring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ring.iter().cloned().collect()
    }

    /// Return entries with `seq > last_seq` (for reconnect resume).
    ///
    /// Returns `None` if `last_seq` falls outside the ring's oldest entry
    /// (i.e., the requested entries were evicted). The caller should emit a
    /// `lag` event and send the full snapshot instead.
    pub fn snapshot_from(&self, last_seq: u64) -> Option<Vec<Arc<GlobalEventEntry>>> {
        let ring = self
            .ring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let oldest = ring.front().map_or(0, |e| e.seq);
        if last_seq < oldest.saturating_sub(1) {
            return None;
        }
        Some(ring.iter().filter(|e| e.seq > last_seq).cloned().collect())
    }
}

/// Returns `true` if `entry` passes the filter.
pub fn matches_filter(entry: &GlobalEventEntry, filter: &EventFilter) -> bool {
    if let Some(ref build_id) = filter.build_id {
        match &entry.source {
            EventSource::BuildSession { codename } => {
                if codename != build_id {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// Background task: debounce-write entries to `path` as NDJSON.
///
/// Rotates the file when it exceeds [`PERSIST_MAX_BYTES`].
async fn persist_task(mut rx: mpsc::UnboundedReceiver<Arc<GlobalEventEntry>>, path: PathBuf) {
    use tokio::time::{Duration, sleep};

    let mut pending: Vec<Arc<GlobalEventEntry>> = Vec::new();

    loop {
        tokio::select! {
            entry = rx.recv() => {
                match entry {
                    Some(e) => pending.push(e),
                    None => break,
                }
            }
            () = sleep(Duration::from_millis(PERSIST_DEBOUNCE_MS)), if !pending.is_empty() => {
                flush_pending(&path, &pending);
                pending.clear();
            }
        }
    }

    if !pending.is_empty() {
        flush_pending(&path, &pending);
    }
}

fn flush_pending(path: &PathBuf, entries: &[Arc<GlobalEventEntry>]) {
    use std::io::Write as _;

    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > PERSIST_MAX_BYTES {
            let rotated = path.with_extension("ndjson.old");
            let _ = std::fs::rename(path, &rotated);
        }
    }

    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        return;
    };

    for entry in entries {
        if let Ok(line) = serde_json::to_string(entry.as_ref()) {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// Background task used when disk persistence is disabled: just drains the channel.
async fn drain_task(mut rx: mpsc::UnboundedReceiver<Arc<GlobalEventEntry>>) {
    while rx.recv().await.is_some() {}
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::events::types::{AyinStatus, WebEvent};

    fn make_store() -> GlobalEventStore {
        GlobalEventStore::new(None)
    }

    fn dummy_event() -> (EventSource, WebEvent) {
        (
            EventSource::ConductorWorker {
                task_id: "t1".into(),
            },
            WebEvent::AyinStatus(AyinStatus::Connected),
        )
    }

    #[tokio::test]
    async fn push_increments_seq() {
        let store = make_store();
        let (src, ev) = dummy_event();
        let e1 = store.push(src.clone(), ev.clone());
        let e2 = store.push(src.clone(), ev.clone());
        assert_eq!(e1.seq, 1);
        assert_eq!(e2.seq, 2);
    }

    #[tokio::test]
    async fn ring_cap_evicts_oldest() {
        let store = make_store();
        let (src, ev) = dummy_event();
        for _ in 0..RING_CAP + 10 {
            store.push(src.clone(), ev.clone());
        }
        let snap = store.snapshot();
        assert_eq!(snap.len(), RING_CAP);
        // oldest entry should have seq > 10 (the first 10 were evicted)
        assert!(snap.first().unwrap().seq > 10);
    }

    #[tokio::test]
    async fn snapshot_from_returns_slice() {
        let store = make_store();
        let (src, ev) = dummy_event();
        let e1 = store.push(src.clone(), ev.clone());
        let _e2 = store.push(src.clone(), ev.clone());
        let e3 = store.push(src.clone(), ev.clone());

        let slice = store.snapshot_from(e1.seq).unwrap();
        assert_eq!(slice.len(), 2, "expected e2 and e3");
        assert_eq!(slice.last().unwrap().seq, e3.seq);
    }

    #[tokio::test]
    async fn snapshot_from_none_on_evicted() {
        let store = make_store();
        let (src, ev) = dummy_event();
        for _ in 0..RING_CAP + 5 {
            store.push(src.clone(), ev.clone());
        }
        // seq=1 was evicted — snapshot_from(0) should return None
        assert!(store.snapshot_from(0).is_none());
    }

    #[tokio::test]
    async fn broadcast_receiver_gets_event() {
        let store = make_store();
        let mut rx = store.subscribe();
        let (src, ev) = dummy_event();
        let pushed = store.push(src, ev);
        let received = rx.recv().await.unwrap();
        assert_eq!(received.seq, pushed.seq);
    }

    #[test]
    fn matches_filter_build_id() {
        let (_, ev) = dummy_event();
        let entry = GlobalEventEntry {
            seq: 1,
            timestamp: chrono::Utc::now(),
            source: EventSource::BuildSession {
                codename: "my-build".into(),
            },
            event: ev.clone(),
        };
        let filter = EventFilter {
            build_id: Some("my-build".into()),
            ..Default::default()
        };
        assert!(matches_filter(&entry, &filter));

        let filter_wrong = EventFilter {
            build_id: Some("other".into()),
            ..Default::default()
        };
        assert!(!matches_filter(&entry, &filter_wrong));
    }
}
