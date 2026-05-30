//! HITL relay — pending escalation queue for autonomous ironclaw builds.
//!
//! When a worker encounters a `UserEscalation` (`DecisionPipeline` Layer 4), the
//! bridge parks a [`HitlEntry`] here and sends a [`WebEvent::IronclawHitlEscalation`]
//! SSE event. The operator resolves the escalation via
//! `POST /api/control { kind: "ironclaw_hitl_resolution", escalation_nonce, ... }`,
//! which sends on the embedded oneshot and unblocks the waiting worker.
//!
//! # Anti-IDOR + anti-replay design
//!
//! - `call_id` — server-minted `UUIDv4` (never client-supplied). The queue is
//!   keyed by `call_id`; the legacy resolve endpoint uses this.
//! - `escalation_nonce` — server-minted `UUIDv7` embedded in Telegram
//!   `callback_data` (URL-safe base64). Validated by the `IronclawHitlResolver`
//!   on resolution; consumed exactly once (SERAPH#3 anti-replay, CWE-209).
//!   The nonce is NEVER logged or included in error messages.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use tokio::sync::oneshot;
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A pending HITL escalation awaiting operator resolution.
pub struct HitlEntry {
    /// Server-minted `UUIDv4` — used as the path parameter in the legacy resolve endpoint.
    pub call_id: Uuid,
    /// Server-minted `UUIDv7` — single-use anti-replay token (SERAPH#3).
    /// Embedded in Telegram `callback_data`; validated by the control handler.
    /// SECURITY: never log or include in error messages (CWE-209).
    pub escalation_nonce: Uuid,
    /// Build that triggered this escalation.
    pub build_id: Uuid,
    /// Task that originated the escalation.
    pub task_id: String,
    /// Human-readable reason surfaced to the operator.
    pub reason: String,
    /// Zero-based wave index at the time of escalation.
    pub wave_index: u32,
    /// Slot number (1–7) blocked by this escalation.
    pub worker_slot: u8,
    /// Wall-clock time the escalation was created.
    pub created_at: DateTime<Utc>,
    /// Sender half of the oneshot — worker awaits on the receiver.
    pub resolve_tx: oneshot::Sender<HitlDecision>,
}

/// Operator decision for a pending HITL escalation.
#[derive(Debug, Clone)]
pub struct HitlDecision {
    /// `true` = operator approved the blocked action; `false` = rejected.
    pub approved: bool,
    /// Optional free-text reason from the operator.
    pub operator_reason: Option<String>,
}

/// Shared map of pending HITL escalations, keyed by `call_id`.
///
/// [`Arc`]-wrapped so it can be shared between [`crate::server::AppState`]
/// (HTTP handlers) and the bridge background task (inserting new entries).
pub type HitlQueue = Arc<DashMap<Uuid, HitlEntry>>;

/// Construct an empty [`HitlQueue`].
#[must_use]
pub fn hitl_queue() -> HitlQueue {
    Arc::new(DashMap::new())
}

/// Insert a new escalation into the queue and return the
/// `(call_id, escalation_nonce, decision_receiver)` triple.
///
/// - `call_id` — identifies the entry for legacy HTTP resolution.
/// - `escalation_nonce` — `UUIDv7` anti-replay token; embed in
///   `IronclawHitlEscalationEvent.nonce` and Telegram `callback_data`.
/// - `decision_receiver` — worker awaits this for the operator verdict.
#[must_use]
pub fn park(
    queue: &HitlQueue,
    build_id: Uuid,
    task_id: String,
    reason: String,
    wave_index: u32,
    worker_slot: u8,
) -> (Uuid, Uuid, oneshot::Receiver<HitlDecision>) {
    let call_id = Uuid::new_v4();
    let escalation_nonce = Uuid::now_v7();
    let (resolve_tx, resolve_rx) = oneshot::channel();
    queue.insert(
        call_id,
        HitlEntry {
            call_id,
            escalation_nonce,
            build_id,
            task_id,
            reason,
            wave_index,
            worker_slot,
            created_at: Utc::now(),
            resolve_tx,
        },
    );
    (call_id, escalation_nonce, resolve_rx)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn park_inserts_entry_and_returns_matching_call_id() {
        let q = hitl_queue();
        let (call_id, _nonce, _rx) = park(
            &q,
            Uuid::new_v4(),
            "task-1".to_owned(),
            "unsafe code".to_owned(),
            0,
            1,
        );
        assert!(q.contains_key(&call_id));
    }

    #[test]
    fn park_returns_distinct_call_id_and_nonce() {
        let q = hitl_queue();
        let (call_id, nonce, _rx) = park(
            &q,
            Uuid::new_v4(),
            "task-1".to_owned(),
            "unsafe code".to_owned(),
            0,
            1,
        );
        assert_ne!(call_id, nonce, "call_id and nonce must be distinct UUIDs");
    }

    #[test]
    fn escalation_nonce_stored_in_entry() {
        let q = hitl_queue();
        let (call_id, nonce, _rx) = park(
            &q,
            Uuid::new_v4(),
            "task-2".to_owned(),
            "dep-add".to_owned(),
            1,
            3,
        );
        let entry = q.get(&call_id).unwrap();
        assert_eq!(
            entry.escalation_nonce, nonce,
            "entry.escalation_nonce must match the returned nonce"
        );
    }

    #[test]
    fn resolve_removes_entry() {
        let q = hitl_queue();
        let (call_id, _nonce, _rx) = park(
            &q,
            Uuid::new_v4(),
            "task-2".to_owned(),
            "dep-add".to_owned(),
            1,
            3,
        );
        let entry = q.remove(&call_id).map(|(_, e)| e);
        assert!(entry.is_some());
        assert!(!q.contains_key(&call_id));
    }

    #[tokio::test]
    async fn decision_is_received_by_worker() {
        let q = hitl_queue();
        let build_id = Uuid::new_v4();
        let (call_id, _nonce, rx) =
            park(&q, build_id, "task-3".to_owned(), "reason".to_owned(), 0, 2);

        let entry = q.remove(&call_id).unwrap().1;
        entry
            .resolve_tx
            .send(HitlDecision {
                approved: true,
                operator_reason: Some("looks fine".to_owned()),
            })
            .ok();

        let decision = rx.await.unwrap();
        assert!(decision.approved);
        assert_eq!(decision.operator_reason.as_deref(), Some("looks fine"));
    }

    #[test]
    fn different_parks_get_unique_ids() {
        let q = hitl_queue();
        let bid = Uuid::new_v4();
        let (id1, n1, _) = park(&q, bid, "t1".to_owned(), "r".to_owned(), 0, 1);
        let (id2, n2, _) = park(&q, bid, "t2".to_owned(), "r".to_owned(), 0, 2);
        assert_ne!(id1, id2, "call_ids must be unique");
        assert_ne!(n1, n2, "nonces must be unique");
    }
}
