//! Conversation format abstractions.
//!
//! A `ConversationFormat` defines the structured slots (speaking turns) for
//! a conversation session. The interest scorer uses slot metadata to route
//! speakers — e.g. `canon_check: true` slots always include LÆX.

/// A single speaking slot in a conversation format.
#[derive(Debug, Clone)]
pub struct Slot {
    /// Slot label (e.g. `"intro"`, `"canon_check"`, `"resolution"`).
    pub label: &'static str,
    /// When `true`, LÆX is always eligible for this slot regardless of score.
    pub canon_check: bool,
}

/// Trait for named conversation formats that declare their slot structure.
pub trait ConversationFormat: Send + Sync {
    /// Human-readable format name.
    fn name(&self) -> &'static str;

    /// Ordered speaking slots for this format.
    fn slots(&self) -> &[Slot];
}

// ─────────────────────────────────────────────────────────────────
// Built-in formats
// ─────────────────────────────────────────────────────────────────

/// LÆX-mediated canon evaluation format.
///
/// Used when the session topic requires alignment with the squad's
/// constitutional principles. LÆX always speaks in the `canon_check` slot.
pub struct CanonEvaluation;

impl ConversationFormat for CanonEvaluation {
    fn name(&self) -> &'static str {
        "canon_evaluation"
    }

    fn slots(&self) -> &[Slot] {
        &[
            Slot {
                label: "framing",
                canon_check: false,
            },
            Slot {
                label: "canon_check",
                canon_check: true,
            },
            Slot {
                label: "resolution",
                canon_check: false,
            },
        ]
    }
}

/// Free-form exploratory format — no canon gate.
///
/// Used for rubber-duck debugging sessions where unconstrained ideation
/// is more valuable than alignment checks.
pub struct RubberDuck;

impl ConversationFormat for RubberDuck {
    fn name(&self) -> &'static str {
        "rubber_duck"
    }

    fn slots(&self) -> &[Slot] {
        &[
            Slot {
                label: "problem",
                canon_check: false,
            },
            Slot {
                label: "ideation",
                canon_check: false,
            },
            Slot {
                label: "reflection",
                canon_check: false,
            },
        ]
    }
}
