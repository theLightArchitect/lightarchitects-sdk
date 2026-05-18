//! Supervisor — long-running session monitoring the `ironclaw-hitl` channel.
//!
//! Per canonical IRONCLAW PDF spec (Supervisor Design §):
//! - Loads all context once at startup (canon docs, Northstar, LASDLC plan, 10 LightArchitects registry)
//! - Prompt-cached system context (~80K tokens, ~10% of base price per call thereafter)
//! - Polls `ironclaw-hitl` channel, applies Canon → Northstar → LightArchitect, escalates to User only as last resort
//! - Appends every decision to the decision ledger via `crate::turnlog::TurnEntry`
//!   (HMAC-chained for tamper detection — stronger than canonical PDF's plain `decisions.md`)
//! - Between builds: refreshes context (re-serializes updated decision log into new session)
//!   to prevent context window bloat from degrading late-program decision quality
//!
//! Phase 4 implementation — uses `crate::platform::PlatformClient` for canon resolution,
//! `crate::squad_registry` for LightArchitect dispatch.
//!
//! Phase 1 stub — channel + loop declared in Phase 4.
