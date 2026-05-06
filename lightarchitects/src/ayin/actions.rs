//! Canonical AYIN action enum — observability sessions, spans, conversations.
//!
//! Every action that the AYIN HTTP viewer (`localhost:3742`) supports is
//! represented here. The enum is split into two tiers:
//!
//! - **PUBLIC** — gateway-routable, available to any SDK consumer.
//! - **INTERNAL** — viewer-only actions, never routed through the gateway.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Canonical AYIN actions — observability sessions, spans, conversations.
///
/// The 3 public actions cover trace session listing (`sessions`), span data
/// loading (`spans`), and conversation/decision trace retrieval
/// (`conversations`).
///
/// Two internal actions (`dashboard`, `vendor`) handle the viewer UI and
/// are never exposed through the Light Architects gateway.
///
/// AYIN communicates over HTTP (`reqwest` to `localhost:3742`), not MCP
/// subprocess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AyinAction {
    // ── PUBLIC (3) ──────────────────────────────────────────────────────────
    /// List all trace sessions.
    #[serde(alias = "list_sessions")]
    Sessions,
    /// Load `TraceSpan` JSON for a session.
    #[serde(alias = "get_spans")]
    Spans,
    /// Load conversation/decision traces.
    #[serde(alias = "get_conversations")]
    Conversations,

    // ── INTERNAL (2) — not gateway-routed ───────────────────────────────────
    /// Serve the viewer dashboard HTML.
    #[doc(hidden)]
    Dashboard,
    /// Serve vendored static assets.
    #[doc(hidden)]
    Vendor,
}

impl AyinAction {
    /// All gateway-routable actions (PUBLIC only).
    pub const ALL_ROUTABLE: &[Self] = &[Self::Sessions, Self::Spans, Self::Conversations];

    /// Returns `true` for PUBLIC actions that are routed through the Light
    /// Architects gateway. Returns `false` for INTERNAL viewer actions.
    #[must_use]
    pub const fn is_gateway_routable(&self) -> bool {
        !matches!(self, Self::Dashboard | Self::Vendor)
    }

    /// Returns the canonical snake\_case string used in API calls.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            Self::Spans => "spans",
            Self::Conversations => "conversations",
            Self::Dashboard => "dashboard",
            Self::Vendor => "vendor",
        }
    }
}

impl fmt::Display for AyinAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AyinAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sessions" | "list_sessions" => Ok(Self::Sessions),
            "spans" | "get_spans" => Ok(Self::Spans),
            "conversations" | "get_conversations" => Ok(Self::Conversations),
            "dashboard" => Ok(Self::Dashboard),
            "vendor" => Ok(Self::Vendor),
            other => Err(format!("unknown AYIN action: {other}")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_all_routable_count() {
        assert_eq!(AyinAction::ALL_ROUTABLE.len(), 3);
    }

    #[test]
    fn test_as_str_roundtrip() {
        for &action in AyinAction::ALL_ROUTABLE {
            let s = action.as_str();
            let parsed: AyinAction = s.parse().unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(parsed, action, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn test_internal_not_routable() {
        assert!(!AyinAction::Dashboard.is_gateway_routable());
        assert!(!AyinAction::Vendor.is_gateway_routable());
    }

    #[test]
    fn test_from_str() {
        assert_eq!("sessions".parse::<AyinAction>(), Ok(AyinAction::Sessions));
        assert_eq!("spans".parse::<AyinAction>(), Ok(AyinAction::Spans));
        assert_eq!(
            "list_sessions".parse::<AyinAction>(),
            Ok(AyinAction::Sessions)
        );
        assert_eq!("get_spans".parse::<AyinAction>(), Ok(AyinAction::Spans));
        assert_eq!(
            "get_conversations".parse::<AyinAction>(),
            Ok(AyinAction::Conversations)
        );
        assert_eq!(
            "conversations".parse::<AyinAction>(),
            Ok(AyinAction::Conversations)
        );
        assert_eq!("dashboard".parse::<AyinAction>(), Ok(AyinAction::Dashboard));
        assert!("nonexistent".parse::<AyinAction>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(AyinAction::Sessions.to_string(), "sessions");
        assert_eq!(AyinAction::Dashboard.to_string(), "dashboard");
    }

    #[test]
    fn test_serde_roundtrip() {
        let action = AyinAction::Conversations;
        let json = serde_json::to_string(&action).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, "\"conversations\"");
        let parsed: AyinAction = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed, action);
    }
}
