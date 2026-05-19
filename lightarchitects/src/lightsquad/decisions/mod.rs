//! Decision log subsystem for lightsquad.
//!
//! Records every gate decision (Canon, Northstar, LightArchitect, User) as an
//! append-only NDJSON log with HMAC-SHA256 integrity chaining. Each entry
//! commits to the previous entry's hash, making deletion or mutation of past
//! entries detectable.

/// HMAC-chained NDJSON decision log.
pub mod hash_chain;
