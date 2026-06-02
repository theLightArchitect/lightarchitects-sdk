//! Canonical domain roles for the A2A message bus.
//!
//! Role strings appear in [`super::supervisor::HitlEscalation`],
//! [`super::decision_pipeline::DecisionContext`], and `WebEventV2.agent_id`.
//! The bus is role-addressed — never name-addressed (no sibling names in the protocol).
//!
//! # Wire format
//!
//! Serialises to kebab-case: `"engineer"`, `"quality"`, `"security"`, etc.
//! Deserialisation is case-sensitive; unknown strings map to a parse error.
//!
//! # Default
//!
//! [`AgentRole::Engineer`] is the default — a task without an explicit role
//! is treated as a write-class implementation task.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// A domain role on the A2A bus.
///
/// Every agent slot, HITL escalation, and SSE envelope carries a role so the
/// operator can filter and route by domain without coupling to named siblings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AgentRole {
    /// Code generation, architecture, and implementation tasks (default).
    #[default]
    Engineer,
    /// Standards, clippy, fmt, complexity, canon compliance.
    Quality,
    /// Threat surface, vulnerability assessment, application security.
    Security,
    /// Deploy pipeline, CI/CD, rollback, operational correctness.
    Ops,
    /// Prior art, dependency audit, risk scoring, investigation.
    Researcher,
    /// Test pyramid, coverage, property tests, regression guards.
    Testing,
    /// Helix enrichment, citation quality, documentation.
    Knowledge,
    /// The gateway process itself — emitted only by the webshell envelope layer.
    Gateway,
}

impl fmt::Display for AgentRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Engineer => "engineer",
            Self::Quality => "quality",
            Self::Security => "security",
            Self::Ops => "ops",
            Self::Researcher => "researcher",
            Self::Testing => "testing",
            Self::Knowledge => "knowledge",
            Self::Gateway => "gateway",
        };
        f.write_str(s)
    }
}

/// Error returned by [`AgentRole::from_str`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRole(pub String);

impl fmt::Display for UnknownRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown agent role: {:?}", self.0)
    }
}

impl std::error::Error for UnknownRole {}

impl FromStr for AgentRole {
    type Err = UnknownRole;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "engineer" => Ok(Self::Engineer),
            "quality" => Ok(Self::Quality),
            "security" => Ok(Self::Security),
            "ops" => Ok(Self::Ops),
            "researcher" => Ok(Self::Researcher),
            "testing" => Ok(Self::Testing),
            "knowledge" => Ok(Self::Knowledge),
            "gateway" => Ok(Self::Gateway),
            other => Err(UnknownRole(other.to_owned())),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_serde_engineer() {
        let json = serde_json::to_string(&AgentRole::Engineer).unwrap();
        assert_eq!(json, r#""engineer""#);
        let back: AgentRole = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AgentRole::Engineer);
    }

    #[test]
    fn all_roles_display_round_trip_via_from_str() {
        let roles = [
            AgentRole::Engineer,
            AgentRole::Quality,
            AgentRole::Security,
            AgentRole::Ops,
            AgentRole::Researcher,
            AgentRole::Testing,
            AgentRole::Knowledge,
            AgentRole::Gateway,
        ];
        for role in roles {
            let s = role.to_string();
            let parsed: AgentRole = s.parse().unwrap();
            assert_eq!(parsed, role, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn default_is_engineer() {
        assert_eq!(AgentRole::default(), AgentRole::Engineer);
    }

    #[test]
    fn unknown_role_returns_error() {
        let err = "corso".parse::<AgentRole>();
        assert!(err.is_err());
    }
}
