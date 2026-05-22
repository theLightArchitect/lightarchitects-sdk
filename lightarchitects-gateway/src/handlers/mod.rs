//! In-process sibling handlers for the gateway monolith.
//!
//! When `inline-*` feature flags are enabled, each sibling's handler logic is
//! compiled directly into the gateway binary. The handlers implement
//! [`SiblingHandler`](lightarchitects::core::handler::SiblingHandler) and are
//! registered in a global [`HandlerRegistry`] at startup.
//!
//! # Canon XLIII — Sibling Substrate Partition
//!
//! The Sibling Sovereignty Criterion (SSC) determines whether a sibling may be
//! absorbed inline. A sibling scoring ≥2 SSC clauses ([K] knowledge / [O] observation /
//! [P] performance-isolated / [S] trust boundary) must exist as an independent daemon.
//! Absorption of a daemon-class sibling violates §H without Canon XXXIX re-ratification.
//!
//! # Feature flags — SSC partition
//!
//! | Feature | Handler | SSC Score | Form | Notes |
//! |---------|---------|-----------|------|-------|
//! | `inline-corso` | `CorsoHandler` | 0 | **Inline** | Stateless ClaudeCliProvider dispatch |
//! | `inline-quantum` | `QuantumHandler` | 0 | **Inline** | Stateless research dispatch |
//! | `inline-laex` | `LaexHandler` | 0 | **Inline** (only form) | No standalone binary |
//! | ~~`inline-soul`~~ | — | 3 | **Daemon** | [K] Neo4j + [O] AYIN + [P] fastembed; compile_error if enabled |
//! | ~~`inline-eva`~~ | — | 2 | **Daemon** | [K] vaults + [O] hook pipeline; compile_error if enabled |
//! | ~~`inline-ayin`~~ | — | 2 | **Daemon** (Pattern C) | [O] observability + [P] :3742; compile_error if enabled |
//! | — (intentional) | — | 2 | **Daemon** (spawner-only) | SERAPH: [P] scan tools + [S] red-team principal |

// ── Canon XLIII SSC enforcement — daemon siblings may not be absorbed inline ───
// SSC scores ≥2 require daemon form. Enabling these features is a canon violation.

#[cfg(feature = "inline-soul")]
compile_error!(
    "Canon XLIII violation: SOUL scores SSC=3 ([K] Neo4j + [O] AYIN + [P] fastembed). \
     SOUL must remain a daemon. Remove `inline-soul` from your feature list. \
     See platform-canon.md §Canon XLIII."
);

#[cfg(feature = "inline-eva")]
compile_error!(
    "Canon XLIII violation: EVA scores SSC=2 ([K] memory vaults + [O] hook pipeline). \
     EVA must remain a daemon. Remove `inline-eva` from your feature list. \
     See platform-canon.md §Canon XLIII."
);

#[cfg(feature = "inline-ayin")]
compile_error!(
    "Canon XLIII violation: AYIN scores SSC=2 ([O] observability platform + [P] :3742 dashboard). \
     AYIN must remain a daemon (Pattern C). Remove `inline-ayin` from your feature list. \
     See platform-canon.md §Canon XLIII."
);

#[cfg(any(
    feature = "inline-corso",
    feature = "inline-quantum",
    feature = "inline-laex",
))]
mod registry;

#[cfg(any(
    feature = "inline-corso",
    feature = "inline-quantum",
    feature = "inline-laex",
))]
pub use registry::{init_handlers, registry};

// ── Individual handler modules (SSC-absorbable siblings only) ──────────────────

#[cfg(feature = "inline-corso")]
mod corso;

#[cfg(feature = "inline-quantum")]
mod quantum;

#[cfg(feature = "inline-laex")]
mod laex;

// ── Re-export handler structs for integration tests ───────────────────────────

#[cfg(feature = "inline-corso")]
pub use corso::CorsoHandler;

#[cfg(feature = "inline-quantum")]
pub use quantum::QuantumHandler;

#[cfg(feature = "inline-laex")]
pub use laex::LaexHandler;

// ── No inline handlers enabled — provide stub ──────────────────────────────────

#[cfg(not(any(
    feature = "inline-corso",
    feature = "inline-quantum",
    feature = "inline-laex",
)))]
/// No-op initializer when no inline handlers are compiled in.
pub fn init_handlers(_config: &crate::config::GatewayConfig) {
    // Nothing to initialize — all siblings use the spawner.
}

#[cfg(not(any(
    feature = "inline-corso",
    feature = "inline-quantum",
    feature = "inline-laex",
)))]
/// Returns `None` when no inline handlers are compiled in.
#[must_use]
pub fn registry() -> Option<&'static lightarchitects::core::handler::HandlerRegistry> {
    None
}
