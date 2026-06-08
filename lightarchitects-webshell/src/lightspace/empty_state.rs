//! First-paint canvas state for a fresh Lightspace session.

use lightarchitects_lightspace::Lightspace;
use uuid::Uuid;

/// Return a fresh [`Lightspace`] engine seeded with an empty canvas.
///
/// The canvas inherits `session_id` so every event emitted from it carries
/// the correct correlation key without an extra parameter.
#[must_use]
pub fn fresh(session_id: Uuid) -> Lightspace {
    Lightspace::new(session_id)
}
