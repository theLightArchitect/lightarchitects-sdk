//! Per-session Lightspace engine registry.
//!
//! Each session slot pairs a [`Lightspace`] reducer with a per-session
//! [`broadcast::Sender`] so SSE subscribers can filter events by topic
//! without contending on the global `event_tx`.  The slot is inserted on
//! first access and never explicitly removed — entries persist for the
//! process lifetime (session memory is bounded by the reducer state size,
//! ~8 KB per canvas, and the broadcast channel itself, 64-event ring).

use std::sync::{Arc, atomic::AtomicU64};

use dashmap::DashMap;
use lightarchitects_lightspace::Lightspace;
use tokio::sync::{Mutex, RwLock, broadcast};
use tracing::instrument;
use uuid::Uuid;

use crate::events::WebEventV2;
use crate::lightspace::hmac_seed::{HmacSeed, new_seed};

/// Per-session broadcast channel capacity (events before oldest is dropped).
const SESSION_CHANNEL_CAP: usize = 64;

/// A single active Lightspace session.
pub struct SessionSlot {
    /// Pure-reducer engine guarded by a read-write lock.
    ///
    /// Reads (canvas snapshot) are concurrent; writes (event dispatch) are
    /// exclusive.  Acquire write lock only in the dispatch path; acquire read
    /// lock for snapshot and replay.
    pub engine: Arc<RwLock<Lightspace>>,
    /// Per-session broadcast sender — subscribe BEFORE dispatching the first event.
    ///
    /// CWE-662: the subscriber is established before the producer to prevent
    /// lost events between dispatch and subscription.
    pub broadcast_tx: broadcast::Sender<WebEventV2>,
    /// Monotonic creation instant for TTL / retention checks.
    pub created_at: std::time::Instant,
    /// Per-session HMAC seed for the NDJSON event-log chain.
    ///
    /// Fixed at slot creation; never changes for the lifetime of the session.
    pub hmac_seed: HmacSeed,
    /// Monotonically-increasing event sequence counter.
    ///
    /// Incremented atomically on each `apply_event` call.
    pub event_counter: AtomicU64,
    /// Last HMAC chain value (`[0u8; 32]` for a fresh session).
    ///
    /// Protected by a `Mutex` so the read-update-write in `apply_event` is
    /// atomic with respect to other concurrent `apply_event` callers on the
    /// same session.  Held only for the duration of `persist::append`.
    pub prev_chain: Mutex<[u8; 32]>,
}

/// Concurrent registry of all active Lightspace sessions.
///
/// Use [`LightspaceRegistry::get_or_create`] from route handlers; the
/// method is idempotent and returns an `Arc` clone (no `DashMap` ref guard
/// escapes — safe across `.await` points).
pub struct LightspaceRegistry {
    inner: DashMap<Uuid, Arc<SessionSlot>>,
}

impl LightspaceRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    /// Return the existing slot for `session_id`, or create and insert a fresh one.
    ///
    /// The fresh [`Lightspace`] engine is initialised from [`empty_state`].
    ///
    /// [`empty_state`]: super::empty_state
    #[instrument(name = "lightspace.session.get_or_create", skip(self), fields(session_id = %session_id))]
    #[must_use]
    pub fn get_or_create(&self, session_id: Uuid) -> Arc<SessionSlot> {
        // Fast path: session already exists.
        if let Some(slot) = self.inner.get(&session_id) {
            return Arc::clone(slot.value());
        }
        // Slow path: mint a new slot.  Use `entry` API to handle races where
        // two concurrent requests arrive for the same new session_id.
        let (tx, _) = broadcast::channel(SESSION_CHANNEL_CAP);
        let slot = Arc::new(SessionSlot {
            engine: Arc::new(RwLock::new(super::empty_state::fresh(session_id))),
            broadcast_tx: tx,
            created_at: std::time::Instant::now(),
            hmac_seed: new_seed(),
            event_counter: AtomicU64::new(0),
            prev_chain: Mutex::new([0u8; 32]),
        });
        // or_insert_with returns a RefMut pointing at whatever won the race —
        // either the slot we built or a concurrent thread's slot.  Clone
        // through the RefMut to avoid a second get() + the associated expect.
        let entry = self
            .inner
            .entry(session_id)
            .or_insert_with(|| Arc::clone(&slot));
        Arc::clone(&*entry)
    }

    /// Return the slot for `session_id` if it exists.
    #[must_use]
    pub fn get(&self, session_id: &Uuid) -> Option<Arc<SessionSlot>> {
        self.inner.get(session_id).map(|r| Arc::clone(r.value()))
    }

    /// Number of active sessions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// `true` when no sessions are active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for LightspaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
