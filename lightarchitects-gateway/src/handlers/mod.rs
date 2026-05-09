//! In-process sibling handlers for the gateway monolith.
//!
//! When `inline-*` feature flags are enabled, each sibling's handler logic is
//! compiled directly into the gateway binary. The handlers implement
//! [`SiblingHandler`](lightarchitects::core::handler::SiblingHandler) and are
//! registered in a global [`HandlerRegistry`] at startup.
//!
//! # Feature flags
//!
//! | Feature | Handler | Notes |
//! |---------|---------|-------|
//! | `inline-ayin` | `AyinHandler` | JSONL storage, no external deps |
//! | `inline-corso` | `CorsoHandler` | Trinity pipeline, PyO3 |
//! | `inline-eva` | `EvaHandler` | Hook chain, LLM providers |
//! | `inline-soul` | `SoulHandler` | Filesystem vault (use `helix` for Neo4j) |
//! | `inline-quantum` | `QuantumHandler` | Hook system, providers |
//!
//! SERAPH is intentionally **not** inlinable — it stays spawner-only for
//! process isolation (offensive security tools must not crash the gateway).

#[cfg(any(
    feature = "inline-ayin",
    feature = "inline-corso",
    feature = "inline-eva",
    feature = "inline-soul",
    feature = "inline-quantum",
))]
mod registry;

#[cfg(any(
    feature = "inline-ayin",
    feature = "inline-corso",
    feature = "inline-eva",
    feature = "inline-soul",
    feature = "inline-quantum",
))]
pub use registry::{init_handlers, registry};

// ── Individual handler modules (feature-gated) ─────────────────────────────────

#[cfg(feature = "inline-ayin")]
mod ayin;

#[cfg(feature = "inline-corso")]
mod corso;

#[cfg(feature = "inline-eva")]
mod eva;

#[cfg(feature = "inline-soul")]
mod soul;

#[cfg(feature = "inline-quantum")]
mod quantum;

// ── No inline handlers enabled — provide stub ──────────────────────────────────

#[cfg(not(any(
    feature = "inline-ayin",
    feature = "inline-corso",
    feature = "inline-eva",
    feature = "inline-soul",
    feature = "inline-quantum",
)))]
/// No-op initializer when no inline handlers are compiled in.
pub fn init_handlers(_config: &crate::config::GatewayConfig) {
    // Nothing to initialize — all siblings use the spawner.
}

#[cfg(not(any(
    feature = "inline-ayin",
    feature = "inline-corso",
    feature = "inline-eva",
    feature = "inline-soul",
    feature = "inline-quantum",
)))]
/// Returns `None` when no inline handlers are compiled in.
#[must_use]
pub fn registry() -> Option<&'static lightarchitects::core::handler::HandlerRegistry> {
    None
}
