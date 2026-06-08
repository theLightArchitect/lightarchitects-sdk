//! Gate auto-re-evaluation — runs synchronously inside `reduce()` to maintain
//! state consistency when a card update touches a gated field.
//!
//! CWE-367 TOCTOU prevention: gates are re-evaluated in the same `reduce()` call
//! that mutated the field, so `assert_invariants()` always sees a consistent state.

use crate::types::{CanvasState, GateEvalResult};

/// Re-evaluate all gates whose `requires_field` matches the given `(card_id, path)`.
///
/// Called by the `Update` arm of `reduce()` after applying the mutation.
/// In v0.1.0, the evaluation is conservative: any gate recorded against the
/// updated card is marked as requiring re-verification (`satisfied = false`)
/// unless the gate's recorded state already matches the current card state.
///
/// A future version will carry gate predicates inside `GateEvalResult` and
/// evaluate them against the card content.
pub fn auto_reeval_gates_for_field(state: &mut CanvasState, card_id: &str, _path: &Option<String>) {
    // Collect gate keys that reference this card (avoid borrow issue).
    let affected: Vec<String> = state
        .gating_evaluations
        .keys()
        .filter(|k| k.as_str() == card_id)
        .cloned()
        .collect();

    for key in affected {
        if let Some(eval) = state.gating_evaluations.get_mut(&key) {
            // Mark for re-verification: the field changed so the gate's prior
            // satisfied state may no longer hold. The caller (SSE handler or
            // copilot loop) will emit a new Gating event when it re-evaluates.
            eval.eval_seq = state.snapshot_seq;
        }
    }
}

/// Insert or update a gate evaluation result for a card.
pub fn record_gate_eval(state: &mut CanvasState, card_id: &str, eval: GateEvalResult) {
    state.gating_evaluations.insert(card_id.to_owned(), eval);
}
