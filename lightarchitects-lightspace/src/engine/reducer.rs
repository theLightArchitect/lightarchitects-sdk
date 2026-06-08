//! `Lightspace` — the pure reducer struct and its `reduce()` dispatch.
//!
//! Invariant: `reduce` is the **only** mutation path. It consumes `self`,
//! applies the event to a fresh working copy, asserts all 5 state invariants,
//! and returns the new `Lightspace`. No I/O, no syscalls, no clock reads.

use super::tick;
use crate::error::ReducerError;
use crate::snapshot::Snapshot;
use crate::types::{CanvasEvent, CanvasState};
use tracing::instrument;
use uuid::Uuid;

/// The Lightspace canvas state machine.
///
/// Create with [`Lightspace::new`], advance with [`Lightspace::reduce`],
/// capture with [`Lightspace::snapshot`], restore with [`Lightspace::restore`].
#[derive(Debug, Clone)]
pub struct Lightspace {
    /// The full canvas state.
    pub state: CanvasState,
}

impl Lightspace {
    /// Create a fresh canvas for the given session.
    pub fn new(session_id: Uuid) -> Self {
        Self {
            state: CanvasState::new(session_id),
        }
    }

    /// Restore from a previously captured snapshot.
    pub fn restore(snapshot: Snapshot) -> Self {
        Self {
            state: snapshot.into_state(),
        }
    }

    /// Apply `event` and return the next `Lightspace`.
    ///
    /// Consumes `self`. The caller retains no reference to the prior state
    /// after this call — use [`Lightspace::snapshot`] before calling if you
    /// need to checkpoint the prior state.
    ///
    /// # Errors
    ///
    /// Returns `ReducerError` if the event violates any invariant. The consumed
    /// `self` is not recoverable; restore from a prior snapshot if needed.
    #[instrument(name = "lightspace.reduce", skip(self, event))]
    pub fn reduce(self, event: CanvasEvent) -> Result<Self, ReducerError> {
        let mut next = self.state;
        match event {
            CanvasEvent::Card(card) => tick::apply_card(&mut next, card)?,
            CanvasEvent::Update {
                card_id,
                seq,
                mode,
                path,
                payload,
            } => {
                tick::apply_update(&mut next, card_id, seq, mode, path, payload)?;
            }
            CanvasEvent::Lifecycle {
                card_id,
                transition,
                actor,
                ghost,
                attribution,
            } => {
                tick::apply_lifecycle(&mut next, card_id, transition, actor, ghost, attribution)?;
            }
            CanvasEvent::Graduate {
                card_id,
                file_id,
                content_uri,
                content_mime,
                retain_tombstone,
            } => {
                tick::apply_graduate(
                    &mut next,
                    card_id,
                    file_id,
                    content_uri,
                    content_mime,
                    retain_tombstone,
                )?;
            }
            CanvasEvent::Materialize { phase } => {
                next.materialize_phase = Some(phase);
            }
            CanvasEvent::Gating {
                card_id,
                gate,
                satisfied,
                reason,
            } => {
                tick::apply_gating(&mut next, card_id, gate, satisfied, reason)?;
            }
            CanvasEvent::BranchLane {
                card_id,
                lanes,
                fork_span_id,
                committed_lane_id,
            } => {
                tick::apply_branch_lane(
                    &mut next,
                    card_id,
                    lanes,
                    fork_span_id,
                    committed_lane_id,
                )?;
            }
            CanvasEvent::Confidence {
                target_id,
                target_kind,
                value,
                basis,
                contradicts,
                evidence_tier,
            } => {
                tick::apply_confidence(
                    &mut next,
                    target_id,
                    target_kind,
                    value,
                    basis,
                    contradicts,
                    evidence_tier,
                )?;
            }
            CanvasEvent::ContradictionResolution {
                winner_target_id,
                loser_target_ids,
                seq,
                depth_reached,
                cycle_yielded,
                contributing_seqs,
            } => {
                tick::apply_contradiction_resolution(
                    &mut next,
                    winner_target_id,
                    loser_target_ids,
                    seq,
                    depth_reached,
                    cycle_yielded,
                    contributing_seqs,
                )?;
            }
            CanvasEvent::DrawerFile(file) => tick::apply_drawer_file(&mut next, file)?,
            CanvasEvent::DrawerEvent {
                file_id,
                action,
                actor,
                new_content_uri,
            } => {
                tick::apply_drawer_event(&mut next, file_id, action, actor, new_content_uri)?;
            }
        }
        next.snapshot_seq = next.snapshot_seq.saturating_add(1);
        Self::assert_invariants(&next)?;
        Ok(Self { state: next })
    }

    /// Capture the current state as a snapshot (does not consume `self`).
    pub fn snapshot(&self) -> Snapshot {
        Snapshot::capture(&self.state)
    }

    // ── Invariants ────────────────────────────────────────────────────────────

    /// Verify the 5 post-reduce state invariants. Returns `Err` on any violation.
    fn assert_invariants(state: &CanvasState) -> Result<(), ReducerError> {
        // I1: snapshot_seq has been incremented at least once.
        if state.snapshot_seq == 0 {
            return Err(ReducerError::InvariantViolation(
                "snapshot_seq must be ≥ 1 after the first event".to_owned(),
            ));
        }
        // I2: every key in per_card_seq must have a matching card.
        for id in state.per_card_seq.keys() {
            if !state.cards.contains_key(id) {
                return Err(ReducerError::InvariantViolation(format!(
                    "per_card_seq contains orphaned card_id: {id}"
                )));
            }
        }
        // I3: every key in gating_evaluations must have a matching card.
        for id in state.gating_evaluations.keys() {
            if !state.cards.contains_key(id) {
                return Err(ReducerError::InvariantViolation(format!(
                    "gating_evaluations contains orphaned card_id: {id}"
                )));
            }
        }
        // I4: if a tombstone's card is still in `cards`, it must be Detached.
        //     Ghost mode intentionally keeps the card in `cards` as Detached so
        //     the UI can render the tombstone overlay — both records are valid together.
        for tombstone in &state.tombstones {
            if let Some(card) = state.cards.get(&tombstone.card_id) {
                if card.state == crate::types::CardState::Attached {
                    return Err(ReducerError::InvariantViolation(format!(
                        "tombstone card {} is still Attached in cards",
                        tombstone.card_id
                    )));
                }
            }
        }
        // I5: pending_graduation card_ids must exist in cards as Attached.
        for grad in &state.pending_graduations {
            match state.cards.get(&grad.card_id) {
                None => {
                    return Err(ReducerError::InvariantViolation(format!(
                        "pending_graduation references missing card: {}",
                        grad.card_id
                    )));
                }
                Some(card) if card.state != crate::types::CardState::Attached => {
                    return Err(ReducerError::InvariantViolation(format!(
                        "pending_graduation card {} is not Attached",
                        grad.card_id
                    )));
                }
                _ => {}
            }
        }
        Ok(())
    }
}
