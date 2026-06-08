//! Strategy routing bridge — dispatches a conversation turn to the correct strategy
//! or falls back to native [`ConversationSession`] dispatch.
//!
//! Phase 2 implements routing detection ([`should_route_to_strategy`]).
//! Full strategy and native dispatch are wired in Phase 3.

use std::sync::Arc;

use lightarchitects::agent::loops::StrategyRegistry;

use super::session::{ConvSSEEvent, ConvSessionHandle};

/// Return the canonical strategy name if `message` begins with a known strategy prefix.
///
/// Matching is case-insensitive and strips leading whitespace.
/// Checks all registered strategy profiles via [`StrategyRegistry::profile`].
/// Returns `None` for native dispatch.
///
/// # Examples
///
/// ```
/// assert_eq!(should_route_to_strategy("/build scaffold auth"), Some("build"));
/// assert_eq!(should_route_to_strategy("/REACT audit"), Some("react"));
/// assert_eq!(should_route_to_strategy("plain message"), None);
/// assert_eq!(should_route_to_strategy("/unknown cmd"), None);
/// ```
pub fn should_route_to_strategy(message: &str) -> Option<&'static str> {
    let trimmed = message.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }
    let slug = trimmed[1..].split_whitespace().next().unwrap_or("");
    if slug.is_empty() {
        return None;
    }
    // Look up via profile registry — covers all 19 strategies including Class B (react, etc.)
    StrategyRegistry::profile(&slug.to_lowercase()).map(|p| p.strategy_name)
}

/// Dispatch a conversation turn using strategy routing.
///
/// Sends a `StrategyPhase` event on `handle.event_tx` immediately, then executes
/// the strategy against the session. Phase 2 stub — returns an Error event.
/// Phase 3 wires `StrategyRegistry::lookup` + `ConversationSession` execution.
pub fn dispatch_conversation_strategy(handle: Arc<ConvSessionHandle>, strategy_name: &str) {
    let _ = handle.event_tx.send(ConvSSEEvent::StrategyPhase {
        phase: "pending".to_owned(),
        strategy: strategy_name.to_owned(),
    });
    // WHY: Phase 3 wires LitellmConfig provider + StrategyRegistry::lookup dispatch.
    let _ = handle.event_tx.send(ConvSSEEvent::Error {
        message: format!("Strategy '{strategy_name}' dispatch — wired in Phase 3"),
    });
    let turn_id = uuid::Uuid::new_v4();
    let _ = handle.event_tx.send(ConvSSEEvent::Done { turn_id });
}

/// Dispatch a native (non-strategy) conversation turn.
///
/// Phase 2 stub — returns an Error event. Phase 3 wires `LitellmConfig::build_provider`
/// + `ConversationSession::turn` execution.
pub fn dispatch_conversation_native(handle: Arc<ConvSessionHandle>) {
    // WHY: Phase 3 wires LitellmConfig + ConversationSession.
    let _ = handle.event_tx.send(ConvSSEEvent::Error {
        message: "Native dispatch — wired in Phase 3".to_owned(),
    });
    let turn_id = uuid::Uuid::new_v4();
    let _ = handle.event_tx.send(ConvSSEEvent::Done { turn_id });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_known_strategies() {
        assert_eq!(
            should_route_to_strategy("/build scaffold auth"),
            Some("build")
        );
        assert_eq!(should_route_to_strategy("/secure scan"), Some("secure"));
        assert_eq!(should_route_to_strategy("/scrum review"), Some("scrum"));
        assert_eq!(should_route_to_strategy("/enrich"), Some("enrich"));
    }

    #[test]
    fn routes_case_insensitive() {
        assert_eq!(should_route_to_strategy("/BUILD test"), Some("build"));
        assert_eq!(should_route_to_strategy("  /SECURE scan"), Some("secure"));
    }

    #[test]
    fn does_not_route_plain_messages() {
        assert_eq!(should_route_to_strategy("build this"), None);
        assert_eq!(should_route_to_strategy(""), None);
        assert_eq!(should_route_to_strategy("/"), None);
        assert_eq!(should_route_to_strategy("/unknown_xyz cmd"), None);
    }
}
