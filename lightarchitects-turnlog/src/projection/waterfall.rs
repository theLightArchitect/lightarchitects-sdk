//! AYIN waterfall projection — Phase 1.5 note.
//!
//! With the AYIN alignment refactor, every [`crate::entry::TurnEntry`] wraps
//! an [`ayin::TraceSpan`] directly. The waterfall projection is now identity:
//! `entry.span` IS an AYIN-compatible span record; no conversion is needed to
//! feed the AYIN viewer at `localhost:3742`.
//!
//! The `WaterfallSpan` type (a manual re-shape of `TraceSpan`) is therefore
//! retired. Callers that previously used `WaterfallSpan` should use
//! `TurnEntry::span` directly, or read entries via [`crate::reader::TurnLogReader`]
//! and pass the spans to `ayin::TraceStore`.
//!
//! Week 2 will add a helper that streams session entries to an AYIN `TraceStore`
//! for cross-session visualisation — tracked in Phase 4 (projections + cleanup).
