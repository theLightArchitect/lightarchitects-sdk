//! Global handler registry — initialized once at gateway startup.
//!
//! The registry is stored in a [`OnceLock`] so it can be accessed from the
//! orchestrate dispatch path without passing references through every function.

use std::sync::{Arc, OnceLock};

use lightarchitects::core::handler::{DispatchMode, HandlerRegistry};
use tracing::info;

use crate::config::GatewayConfig;

// ── Global registry ─────────────────────────────────────────────────────────────

static REGISTRY: OnceLock<HandlerRegistry> = OnceLock::new();

/// Return a reference to the global handler registry.
///
/// Returns `None` if [`init_handlers`] has not been called yet.
#[must_use]
pub fn registry() -> Option<&'static HandlerRegistry> {
    REGISTRY.get()
}

/// Initialize the handler registry from gateway config.
///
/// Must be called **once** after config loading and before `server::run()`.
/// Panics if called more than once (programming error, not runtime condition).
///
/// Only registers handlers whose feature flag is **enabled at compile time**
/// AND whose config `mode` is set to `Inline`. Siblings configured as
/// `Spawner` or `Disabled` are skipped even if the feature is compiled in.
pub fn init_handlers(config: &GatewayConfig) {
    let mut reg = HandlerRegistry::new();

    #[cfg(feature = "inline-ayin")]
    if should_inline("ayin", config) {
        let handler = super::ayin::AyinHandler::new(config);
        info!(handler = "ayin", "registering inline handler");
        reg.register(Arc::new(handler));
    }

    #[cfg(feature = "inline-corso")]
    if should_inline("corso", config) {
        let handler = super::corso::CorsoHandler::new(config);
        info!(handler = "corso", "registering inline handler");
        reg.register(Arc::new(handler));
    }

    #[cfg(feature = "inline-eva")]
    if should_inline("eva", config) {
        let handler = super::eva::EvaHandler::new(config);
        info!(handler = "eva", "registering inline handler");
        reg.register(Arc::new(handler));
    }

    #[cfg(feature = "inline-soul")]
    if should_inline("soul", config) {
        let handler = super::soul::SoulHandler::new(config);
        info!(handler = "soul", "registering inline handler");
        reg.register(Arc::new(handler));
    }

    #[cfg(feature = "inline-quantum")]
    if should_inline("quantum", config) {
        let handler = super::quantum::QuantumHandler::new(config);
        info!(handler = "quantum", "registering inline handler");
        reg.register(Arc::new(handler));
    }

    let names = reg.handler_names();
    if names.is_empty() {
        info!("no inline handlers registered — all siblings use spawner");
    } else {
        info!(handlers = ?names, "inline handlers registered");
    }

    if REGISTRY.set(reg).is_err() {
        // Programming error: init_handlers called twice. This is a bug, not
        // a runtime condition. Log and skip rather than panic.
        tracing::error!("init_handlers called more than once — this is a bug");
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────────

/// Check whether a sibling should use the inline handler path.
///
/// Returns `true` only when the sibling is enabled **and** its `mode` is
/// set to `Inline` in the gateway config. A sibling compiled in (feature
/// flag enabled) but configured as `Spawner` still uses the subprocess path.
fn should_inline(name: &str, config: &GatewayConfig) -> bool {
    config
        .agents
        .get(name)
        .is_some_and(|a| a.enabled && a.mode == DispatchMode::Inline)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_inline_returns_false_for_unknown_agent() {
        let config = GatewayConfig::default();
        assert!(!should_inline("nonexistent", &config));
    }

    #[test]
    fn should_inline_returns_false_when_disabled() {
        let config = GatewayConfig::default();
        // QUANTUM is disabled in default config.
        assert!(!should_inline("quantum", &config));
    }

    #[test]
    fn should_inline_returns_false_when_mode_is_spawner() {
        let mut config = GatewayConfig::default();
        if let Some(a) = config.agents.get_mut("corso") {
            a.mode = DispatchMode::Spawner;
            a.enabled = true;
        }
        assert!(!should_inline("corso", &config));
    }

    #[test]
    fn should_inline_returns_true_when_enabled_and_inline() {
        let mut config = GatewayConfig::default();
        if let Some(a) = config.agents.get_mut("corso") {
            a.mode = DispatchMode::Inline;
            a.enabled = true;
        }
        assert!(should_inline("corso", &config));
    }
}
