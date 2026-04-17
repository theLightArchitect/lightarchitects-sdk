//! Projections over the turnlog — Week 2+ scope.
//!
//! Three built-in projections:
//! * [`session`] — `SessionProjection` (replaces `SessionState.yaml`)
//! * [`waterfall`] — `WaterfallProjection` (AYIN span stream)
//! * [`training`] — `TrainingProjection` (`CanonicalTurn` row builder)
//!
//! All projections are read-only transforms over a session's log entries.
//! They never modify the log.

pub mod session;
pub mod training;
pub mod waterfall;
