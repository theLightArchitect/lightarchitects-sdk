//! All errors that `Lightspace::reduce` can return — one variant per failure mode.

use thiserror::Error;

/// Errors produced by `Lightspace::reduce`.
#[derive(Debug, Error)]
pub enum ReducerError {
    /// Two cards attempted to register the same ID.
    #[error("card id collision: {0}")]
    CardIdCollision(String),

    /// A card update arrived with `seq` ≤ the last-seen seq for that card.
    #[error("seq regression for card {card_id}: expected > {expected_after}, got {got}")]
    SeqRegression {
        /// Card that received the out-of-order update.
        card_id: String,
        /// Last seen seq.
        expected_after: u64,
        /// The seq that arrived.
        got: u64,
    },

    /// Canvas has reached the maximum card capacity (CWE-770).
    #[error("canvas card limit exceeded (max {0} cards)")]
    TooManyCards(usize),

    /// An update payload exceeded 64 KiB (CWE-770).
    #[error("update payload too large for card {0} (max 64 KiB)")]
    PayloadTooLarge(String),

    /// A lifecycle transition is not valid from the current card state.
    #[error("illegal state transition for card {card_id}: {reason}")]
    IllegalTransition {
        /// ID of the card.
        card_id: String,
        /// Human-readable reason.
        reason: String,
    },

    /// A copilot actor attempted a privileged transition.
    #[error("copilot is not authorised to perform '{transition}' on card {card_id}")]
    UnauthorisedTransition {
        /// ID of the card.
        card_id: String,
        /// Transition name.
        transition: String,
    },

    /// A referenced `card_id` does not exist in state.
    #[error("card not found: {0}")]
    CardNotFound(String),

    /// A `content_uri` uses a disallowed scheme (CWE-22 + LLM07).
    #[error("disallowed content_uri scheme in: {0}")]
    DisallowedUriScheme(String),

    /// A path component would escape the lightspace root (CWE-22).
    #[error("path traversal detected in: {0}")]
    PathTraversal(String),

    /// Confidence `basis` string is too short (non-trivial claim required).
    #[error("confidence basis for target {0} is too short (min 5 chars)")]
    ConfidenceBasisTooShort(String),

    /// Confidence value is outside `0.0..=1.0`.
    #[error("confidence value {value} is out of range [0.0, 1.0] for target {target_id}")]
    ConfidenceOutOfRange {
        /// Target ID.
        target_id: String,
        /// The invalid value.
        value: f64,
    },

    /// A `ContradictionResolution` arrived with a stale seq.
    #[error("resolution seq {got} is not greater than max contributing seq {max_contrib}")]
    StaleResolution {
        /// Seq on the resolution event.
        got: u64,
        /// Max of `contributing_seqs`.
        max_contrib: u64,
    },

    /// A `Graduate` event targeted a card that is not `Attached`.
    #[error("graduation requires Attached card; card {0} is in wrong state")]
    GraduateBadState(String),

    /// A state invariant was violated after applying an event.
    #[error("reducer invariant violated: {0}")]
    InvariantViolation(String),

    /// RFC 6902 patch application failed.
    #[error("JSON patch failed for card {card_id}: {reason}")]
    PatchFailed {
        /// ID of the card being patched.
        card_id: String,
        /// Error description.
        reason: String,
    },

    /// Provenance `agent` or `source_uri` is empty.
    #[error("provenance agent and source_uri must be non-empty")]
    EmptyProvenance,

    /// A referenced `file_id` does not exist in drawer.
    #[error("drawer file not found: {0}")]
    FileNotFound(String),
}
