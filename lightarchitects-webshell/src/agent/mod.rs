//! Agent session hosting — SSE streaming + WebSocket control for the webshell.
//!
//! This module provides the backend infrastructure for Option E:
//! - SSE endpoint streams `AgentEvent` variants to the browser in real time.
//! - WebSocket endpoint provides bidirectional control (send messages, approve
//!   permissions, interrupt, steer).
//!
//! The actual agent loop lives in the `lightarchitects-cli` binary; this
//! module hosts it as a subprocess and translates between NDJSON on stdio
//! and the SSE/WebSocket protocol.

pub mod bridge;
pub mod protocol;
pub mod sse;
pub mod ws;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use dashmap::DashMap;
use tokio::process::Child;
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use tracing::{info, warn};

use crate::session::BuildSession;

use protocol::{AgentEvent, ControlMessage};

/// Capacity for the per-build agent event broadcast channel.
///
/// Sized for high-frequency tool-call + streaming text rates. 512 gives
/// slow browsers headroom without unbounded memory growth.
pub const AGENT_EVENT_BUF: usize = 512;

/// Capacity for the bridge control channel.
///
/// Bounded to apply backpressure: a client flooding messages faster than
/// the bridge consumes them will receive `ControlResponse::Reject` once the
/// queue fills.
const CONTROL_CHANNEL_CAP: usize = 64;

/// Live agent session hosted inside a `BuildSession`.
///
/// Created lazily on first agent activity (SSE connect or WebSocket message).
/// Holds the subprocess bridge, event broadcast channel, and pending
/// permission request queue.
pub struct AgentSessionHost {
    /// Broadcast sender for `AgentEvent` — SSE subscribers and WebSocket
    /// consumers both receive from here.
    pub event_tx: broadcast::Sender<AgentEvent>,

    /// Channel to the bridge task — used to send `ControlMessage` from
    /// the WebSocket handler into the runner.
    pub control_tx: mpsc::Sender<ControlMessage>,

    /// Child process handle — stored so the session can kill it on drop
    /// or when the build is destroyed.
    pub child: Mutex<Option<Child>>,

    /// Prevents concurrent fallback turns from racing on the broadcast channel.
    ///
    /// Set to `true` when a fallback `run` is in flight; subsequent
    /// `SendMessage`s are rejected until it clears.
    pub fallback_in_flight: AtomicBool,

    /// Pending tool-permission requests awaiting operator approval.
    ///
    /// Key: server-generated `call_id` (`Uuid::new_v4().to_string()`).
    /// Value: `oneshot::Sender<bool>` — send `true` to approve, `false` to deny.
    ///
    /// Populated when `WebEvent::PermissionRequest` is emitted; resolved by
    /// `POST /api/builds/:id/copilot/approve`. Bounded at
    /// [`MAX_PENDING_PERMISSIONS`] entries; insert returns HTTP 429 on overflow.
    ///
    /// On `Drop`, all pending senders receive `false` (deny) to unblock waiting
    /// agent turns. See `Drop` impl below.
    ///
    /// # Note
    /// A1 validation (Sprint 4-A Phase 1) found the Claude CLI does not emit
    /// `permission_request` events at this version. This field is scaffolded
    /// for forward-compatibility; the `/approve` route is wired in Sprint 4-B
    /// once CLI support is confirmed.
    pub permission_queue: Arc<DashMap<String, oneshot::Sender<bool>>>,
}

impl AgentSessionHost {
    /// Create a new host.
    ///
    /// Returns `(host, control_rx)` — the receiver must be passed to
    /// `bridge::spawn_bridge()` to wire the stdin/control loop.
    #[must_use]
    pub fn new() -> (Self, mpsc::Receiver<ControlMessage>) {
        let (event_tx, _) = broadcast::channel(AGENT_EVENT_BUF);
        let (control_tx, control_rx) = mpsc::channel(CONTROL_CHANNEL_CAP);
        let host = Self {
            event_tx: event_tx.clone(),
            control_tx,
            child: Mutex::new(None),
            fallback_in_flight: AtomicBool::new(false),
            permission_queue: Arc::new(DashMap::new()),
        };
        (host, control_rx)
    }
}

/// Maximum number of concurrent pending permission requests per build session.
///
/// Inserts beyond this limit return HTTP 429 to prevent unbounded queue growth
/// under adversarial or runaway agent conditions (§3.3 constraint 3, SA-10).
pub const MAX_PENDING_PERMISSIONS: usize = 64;

impl Drop for AgentSessionHost {
    fn drop(&mut self) {
        // Drain the permission queue — collect keys first to avoid holding a
        // DashMap shard reference while calling send() (deadlock risk, SA-22).
        let keys: Vec<String> = self
            .permission_queue
            .iter()
            .map(|e| e.key().clone())
            .collect();
        for key in keys {
            if let Some((_, sender)) = self.permission_queue.remove(&key) {
                // Deny all pending requests on teardown. Agent turns blocked on
                // oneshot::Receiver::await will unblock with `false`.
                let _ = sender.send(false);
                warn!(call_id = %key, "permission_queue drained on AgentSessionHost drop — denied");
            }
        }
    }
}

/// Lazily initialise the agent host on a `BuildSession`.
///
/// If the session already has an `AgentSessionHost`, returns a clone of its
/// `event_tx`.  Otherwise creates one, stores it, spawns the bridge, and
/// returns the `event_tx`.
///
/// The `agent_host` mutex is held across the entire slow path to prevent
/// TOCTOU races where two concurrent requests both observe `None` and
/// spawn duplicate bridges.
pub async fn ensure_agent_host(
    session: &BuildSession,
) -> (broadcast::Sender<AgentEvent>, mpsc::Sender<ControlMessage>) {
    let mut guard = session.agent_host.lock().await;

    if let Some(host) = guard.as_ref() {
        return (host.event_tx.clone(), host.control_tx.clone());
    }

    // Slow path: create, spawn, and store — all under the same lock.
    let (host, control_rx) = AgentSessionHost::new();
    let tx = host.event_tx.clone();
    let ctrl = host.control_tx.clone();

    let child = bridge::spawn_bridge(session, host.event_tx.clone(), control_rx).await;
    if let Some(c) = child {
        let mut host_with_child = host;
        host_with_child.child = Mutex::new(Some(c));
        *guard = Some(Arc::new(host_with_child));
        info!(build_id = %session.build_id, "agent host initialised and bridge spawned");
    } else {
        // Bridge spawn failed — still store the host so future requests
        // don't retry spawning indefinitely.  The child field stays `None`
        // and fallback mode will be used.
        *guard = Some(Arc::new(host));
        warn!(build_id = %session.build_id, "bridge spawn failed; host stored for fallback mode");
    }

    (tx, ctrl)
}

#[cfg(test)]
#[allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn agent_session_host_creates_channels() {
        let (host, mut control_rx) = AgentSessionHost::new();
        assert_eq!(host.event_tx.receiver_count(), 0);
        assert!(!host.fallback_in_flight.load(Ordering::Relaxed));

        // control channel capacity is 64 — try_send should succeed immediately.
        for i in 0..64 {
            host.control_tx
                .try_send(ControlMessage::Ping)
                .unwrap_or_else(|_| panic!("send {i} should succeed"));
        }
        // 65th send should fail with Full.
        assert!(
            host.control_tx.try_send(ControlMessage::Ping).is_err(),
            "65th send should hit capacity"
        );

        // Drain to confirm bounded semantics.
        for _ in 0..64 {
            control_rx.blocking_recv().expect("drain control_rx");
        }
    }

    #[test]
    fn fallback_in_flight_atomic_swap() {
        let (host, _rx) = AgentSessionHost::new();
        assert!(!host.fallback_in_flight.load(Ordering::Relaxed));

        let swapped = host
            .fallback_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        assert!(swapped);
        assert!(host.fallback_in_flight.load(Ordering::Relaxed));

        // Second swap should fail because already true.
        let swapped_again = host
            .fallback_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        assert!(!swapped_again);
    }
}
