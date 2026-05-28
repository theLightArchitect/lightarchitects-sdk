//! LASDLC semantic-convention attribute constants for AYIN spans.
//!
//! This module is the SDK-side mirror of the canonical constants defined in
//! `AYIN-DEV/ayin/src/semconv/`. Both must be kept in sync when attribute keys
//! are added or renamed — the canonical source is AYIN-DEV; this mirror is the
//! integration surface for gateway and webshell consumers.
pub mod helix_generation;
pub mod lasdlc;
