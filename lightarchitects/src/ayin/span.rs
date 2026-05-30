//! Trace span types — re-exported from [`la_ayinspan`].
//!
//! `la_ayinspan` is the canonical source for all span types. This module
//! re-exports them so SDK consumers can import from `lightarchitects::ayin`
//! without depending on `la_ayinspan` directly.

pub use la_ayinspan::{
    Actor, DecisionPoint, StrandActivation, TraceContext, TraceError, TraceOutcome, TraceSpan,
};

/// Backward-compatible alias: [`Sibling`] is now [`Actor`].
pub type Sibling = Actor;
