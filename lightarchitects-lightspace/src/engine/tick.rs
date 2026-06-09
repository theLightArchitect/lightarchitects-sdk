//! Low-level state mutations — one function per `CanvasEvent` arm.
//!
//! All functions take `&mut CanvasState` and return `Result<(), ReducerError>`.
//! No I/O, no clock reads, no syscalls. The borrow scoping discipline here
//! (read-then-drop-then-mutate) prevents the class of aliased-mutation bugs
//! that Redux/Elm can only prevent through discipline.

use super::contradictions::{self, MAX_CONTRADICTION_DEPTH};
use super::gates;
use crate::error::ReducerError;
use crate::security::{validate_content_uri_scheme, validate_provenance_source_scheme};
use crate::types::{
    Actor, CanvasState, CardData, CardState, CardTransition, ConfidenceRecord, DrawerFileAction,
    DrawerFileData, EvidenceTier, GateEvalResult, GraduationPending, Tombstone, UpdateMode,
};

const MAX_PAYLOAD_BYTES: usize = 65_536; // 64 KiB (CWE-770)
const MIN_BASIS_LEN: usize = 5;
/// Hard cap on canvas cards — prevents O(n) snapshot clone DoS (CWE-770).
pub const MAX_CARDS: usize = 500;

/// Insert a new card. Validates provenance, ID uniqueness, and canvas size cap.
pub fn apply_card(state: &mut CanvasState, mut card: CardData) -> Result<(), ReducerError> {
    if card.provenance.agent.is_empty() || card.provenance.source_uri.is_empty() {
        return Err(ReducerError::EmptyProvenance);
    }
    validate_provenance_source_scheme(&card.provenance.source_uri)?;
    if state.cards.contains_key(&card.id) {
        return Err(ReducerError::CardIdCollision(card.id));
    }
    if state.cards.len() >= MAX_CARDS {
        return Err(ReducerError::TooManyCards(MAX_CARDS));
    }
    card.state = CardState::Attached;
    state.cards.insert(card.id.clone(), card);
    Ok(())
}

/// Update card content via Replace, Append (RFC 6901), or Patch (RFC 6902).
pub fn apply_update(
    state: &mut CanvasState,
    card_id: String,
    seq: u64,
    mode: UpdateMode,
    path: Option<String>,
    payload: serde_json::Value,
) -> Result<(), ReducerError> {
    let last_seq = state.per_card_seq.get(&card_id).copied().unwrap_or(0);
    if seq <= last_seq {
        return Err(ReducerError::SeqRegression {
            card_id,
            expected_after: last_seq,
            got: seq,
        });
    }
    let payload_str = serde_json::to_string(&payload).map_err(|e| ReducerError::PatchFailed {
        card_id: card_id.clone(),
        reason: e.to_string(),
    })?;
    if payload_str.len() > MAX_PAYLOAD_BYTES {
        return Err(ReducerError::PayloadTooLarge(card_id));
    }
    if !state.cards.contains_key(&card_id) {
        return Err(ReducerError::CardNotFound(card_id));
    }
    {
        let card = state
            .cards
            .get_mut(&card_id)
            .ok_or_else(|| ReducerError::CardNotFound(card_id.clone()))?;
        match mode {
            UpdateMode::Replace => {
                card.content = payload;
            }
            UpdateMode::Append => {
                let p = path.as_deref().unwrap_or("");
                append_to_path(&mut card.content, p, payload, &card_id)?;
            }
            UpdateMode::Patch => {
                apply_json_patch(&mut card.content, payload, &card_id)?;
            }
        }
    } // mutable card borrow dropped
    state.per_card_seq.insert(card_id.clone(), seq);
    gates::auto_reeval_gates_for_field(state, &card_id, &path);
    Ok(())
}

/// Transition a card's lifecycle state.
pub fn apply_lifecycle(
    state: &mut CanvasState,
    card_id: String,
    transition: CardTransition,
    actor: Actor,
    ghost: bool,
    attribution: Option<String>,
) -> Result<(), ReducerError> {
    let (from, kind, title) = {
        let card = state
            .cards
            .get(&card_id)
            .ok_or_else(|| ReducerError::CardNotFound(card_id.clone()))?;
        (card.state.clone(), card.kind.clone(), card.title.clone())
    };
    let to = compute_target_state(&from, &transition, &card_id)?;
    authorise_transition(&transition, &actor, &card_id)?;
    {
        let card = state
            .cards
            .get_mut(&card_id)
            .ok_or_else(|| ReducerError::CardNotFound(card_id.clone()))?;
        card.state = to.clone();
        if let Some(attr) = attribution {
            card.attribution = Some(attr);
        }
    } // mutable card borrow dropped
    if to == CardState::Detached && ghost {
        state.tombstones.push(Tombstone {
            card_id: card_id.clone(),
            kind,
            title,
            detached_at_seq: state.snapshot_seq,
        });
    }
    Ok(())
}

/// Stage a card for graduation to the drawer.
pub fn apply_graduate(
    state: &mut CanvasState,
    card_id: String,
    file_id: String,
    content_uri: String,
    content_mime: String,
    retain_tombstone: bool,
) -> Result<(), ReducerError> {
    validate_content_uri_scheme(&content_uri)?;
    let card_state = state
        .cards
        .get(&card_id)
        .ok_or_else(|| ReducerError::CardNotFound(card_id.clone()))?
        .state
        .clone();
    if card_state != CardState::Attached {
        return Err(ReducerError::GraduateBadState(card_id));
    }
    state.pending_graduations.push(GraduationPending {
        card_id,
        file_id,
        content_uri,
        content_mime,
        retain_tombstone,
    });
    Ok(())
}

/// Record a gate evaluation result for a card.
pub fn apply_gating(
    state: &mut CanvasState,
    card_id: String,
    gate: String,
    satisfied: bool,
    reason: Option<String>,
) -> Result<(), ReducerError> {
    if !state.cards.contains_key(&card_id) {
        return Err(ReducerError::CardNotFound(card_id));
    }
    let eval_seq = state.snapshot_seq;
    gates::record_gate_eval(
        state,
        &card_id,
        GateEvalResult {
            gate,
            satisfied,
            reason,
            eval_seq,
        },
    );
    Ok(())
}

/// Update the branch-lane data in a BranchLane card's content.
pub fn apply_branch_lane(
    state: &mut CanvasState,
    card_id: String,
    lanes: serde_json::Value,
    fork_span_id: Option<String>,
    committed_lane_id: Option<String>,
) -> Result<(), ReducerError> {
    let card = state
        .cards
        .get_mut(&card_id)
        .ok_or_else(|| ReducerError::CardNotFound(card_id.clone()))?;
    if let Some(obj) = card.content.as_object_mut() {
        obj.insert("lanes".to_owned(), lanes);
        if let Some(fsid) = fork_span_id {
            obj.insert("fork_span_id".to_owned(), serde_json::Value::String(fsid));
        }
        if let Some(clid) = committed_lane_id {
            obj.insert(
                "committed_lane_id".to_owned(),
                serde_json::Value::String(clid),
            );
        }
    } else {
        card.content = serde_json::json!({
            "lanes": lanes,
            "fork_span_id": fork_span_id,
            "committed_lane_id": committed_lane_id,
        });
    }
    Ok(())
}

/// Record a confidence score and check for contradictions.
#[allow(clippy::too_many_arguments)]
pub fn apply_confidence(
    state: &mut CanvasState,
    target_id: String,
    target_kind: String,
    value: f64,
    basis: String,
    contradicts: Vec<String>,
    evidence_tier: EvidenceTier,
) -> Result<(), ReducerError> {
    if basis.len() < MIN_BASIS_LEN {
        return Err(ReducerError::ConfidenceBasisTooShort(target_id));
    }
    if !(0.0..=1.0).contains(&value) {
        return Err(ReducerError::ConfidenceOutOfRange { target_id, value });
    }
    let recorded_at_seq = state.snapshot_seq;
    state.confidence_records.push(ConfidenceRecord {
        target_id: target_id.clone(),
        target_kind,
        value,
        basis,
        contradicts: contradicts.clone(),
        evidence_tier,
        recorded_at_seq,
    });
    if !contradicts.is_empty() {
        let (cycle, depth) = contradictions::detect_cycle_or_depth(
            &state.confidence_records,
            &target_id,
            &contradicts,
        );
        if cycle || depth >= MAX_CONTRADICTION_DEPTH {
            let resolution = contradictions::synthesize_resolution(
                state,
                &target_id,
                &contradicts,
                depth,
                cycle,
            );
            state.pending_resolutions.push(resolution);
        }
    }
    Ok(())
}

/// Apply a confirmed contradiction resolution.
pub fn apply_contradiction_resolution(
    state: &mut CanvasState,
    winner_target_id: String,
    loser_target_ids: Vec<String>,
    seq: u64,
    _depth_reached: u32,
    _cycle_yielded: bool,
    contributing_seqs: Vec<u64>,
) -> Result<(), ReducerError> {
    let max_contrib = contributing_seqs.iter().copied().max().unwrap_or(0);
    if seq <= max_contrib {
        return Err(ReducerError::StaleResolution {
            got: seq,
            max_contrib,
        });
    }
    contradictions::apply_resolution(state, &winner_target_id, &loser_target_ids);
    Ok(())
}

/// Insert a new file into the session drawer.
pub fn apply_drawer_file(
    state: &mut CanvasState,
    file: DrawerFileData,
) -> Result<(), ReducerError> {
    if file.provenance.agent.is_empty() || file.provenance.source_uri.is_empty() {
        return Err(ReducerError::EmptyProvenance);
    }
    validate_provenance_source_scheme(&file.provenance.source_uri)?;
    validate_content_uri_scheme(&file.content_uri)?;
    state.drawer_files.insert(file.id.clone(), file);
    Ok(())
}

/// Perform an action on an existing drawer file.
pub fn apply_drawer_event(
    state: &mut CanvasState,
    file_id: String,
    action: DrawerFileAction,
    _actor: Actor,
    new_content_uri: Option<String>,
) -> Result<(), ReducerError> {
    match action {
        DrawerFileAction::Attach => {
            // Files are attached via DrawerFile events, not DrawerEvent.
            return Err(ReducerError::PatchFailed {
                card_id: file_id,
                reason: "use DrawerFile event to attach files".to_owned(),
            });
        }
        DrawerFileAction::Detach => {
            if state.drawer_files.shift_remove(&file_id).is_none() {
                return Err(ReducerError::FileNotFound(file_id));
            }
        }
        DrawerFileAction::Update => {
            let uri = new_content_uri.ok_or_else(|| ReducerError::PatchFailed {
                card_id: file_id.clone(),
                reason: "Update action requires new_content_uri".to_owned(),
            })?;
            validate_content_uri_scheme(&uri)?;
            let file = state
                .drawer_files
                .get_mut(&file_id)
                .ok_or(ReducerError::FileNotFound(file_id))?;
            file.content_uri = uri;
        }
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn compute_target_state(
    from: &CardState,
    transition: &CardTransition,
    card_id: &str,
) -> Result<CardState, ReducerError> {
    match (from, transition) {
        (CardState::Attached, CardTransition::Detach) => Ok(CardState::Detached),
        (CardState::Detached, CardTransition::Attach) => Ok(CardState::Attached),
        (CardState::Attached, CardTransition::Attach) => Err(ReducerError::IllegalTransition {
            card_id: card_id.to_owned(),
            reason: "card is already Attached".to_owned(),
        }),
        (CardState::Detached, CardTransition::Detach) => Err(ReducerError::IllegalTransition {
            card_id: card_id.to_owned(),
            reason: "card is already Detached".to_owned(),
        }),
    }
}

fn authorise_transition(
    transition: &CardTransition,
    actor: &Actor,
    card_id: &str,
) -> Result<(), ReducerError> {
    if *actor == Actor::Copilot && *transition == CardTransition::Detach {
        return Err(ReducerError::UnauthorisedTransition {
            card_id: card_id.to_owned(),
            transition: "Detach".to_owned(),
        });
    }
    Ok(())
}

fn append_to_path(
    content: &mut serde_json::Value,
    path: &str,
    payload: serde_json::Value,
    card_id: &str,
) -> Result<(), ReducerError> {
    let target = content
        .pointer_mut(path)
        .ok_or_else(|| ReducerError::PatchFailed {
            card_id: card_id.to_owned(),
            reason: format!("path not found: {path}"),
        })?;
    let arr = target
        .as_array_mut()
        .ok_or_else(|| ReducerError::PatchFailed {
            card_id: card_id.to_owned(),
            reason: format!("path {path} is not an array"),
        })?;
    arr.push(payload);
    Ok(())
}

fn apply_json_patch(
    content: &mut serde_json::Value,
    payload: serde_json::Value,
    card_id: &str,
) -> Result<(), ReducerError> {
    let patch: json_patch::Patch =
        serde_json::from_value(payload).map_err(|e| ReducerError::PatchFailed {
            card_id: card_id.to_owned(),
            reason: e.to_string(),
        })?;
    json_patch::patch(content, &patch).map_err(|e| ReducerError::PatchFailed {
        card_id: card_id.to_owned(),
        reason: e.to_string(),
    })
}
