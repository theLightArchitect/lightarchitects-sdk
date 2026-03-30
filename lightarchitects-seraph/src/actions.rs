//! Canonical SERAPH action enum — pentest orchestration and investigation.
//!
//! Every action that the SERAPH MCP server (`penTools`) supports is represented
//! here. The enum is split into three tiers:
//!
//! - **PUBLIC** — gateway-routable, available to any SDK consumer.
//! - **WORKFLOW** — gateway-routable, investigation lifecycle and vault sync.
//! - **INTERNAL** — scope-gated wing actions, never routed through the gateway.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Canonical SERAPH actions — pentest orchestration and investigation.
///
/// The 6 gateway-routable actions cover system status, the investigation
/// lifecycle (start/advance/close/report), and evidence vault synchronization.
///
/// Eleven internal actions correspond to SERAPH's scope-gated wings and
/// knowledge services. These are never exposed through the Light Architects
/// gateway — they are invoked only by the SERAPH binary itself under
/// `ScopeGovernor` authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeraphAction {
    // ── PUBLIC (1) ──────────────────────────────────────────────────────────
    /// System status (CPU/memory/disk).
    Status,

    // ── WORKFLOW (5) ────────────────────────────────────────────────────────
    /// Begin pentest investigation with evidence chain.
    InvestigateStart,
    /// Advance to next phase with findings.
    InvestigateAdvance,
    /// Close with evidence summary.
    InvestigateClose,
    /// Generate report (Mermaid + JSON).
    InvestigateReport,
    /// Sync evidence to SOUL vault.
    VaultSync,

    // ── INTERNAL — scope-gated wings (11) ───────────────────────────────────
    /// Packet capture and traffic analysis.
    #[doc(hidden)]
    Capture,
    /// Network and port scanning.
    #[doc(hidden)]
    Scan,
    /// Binary and protocol analysis.
    #[doc(hidden)]
    Analyze,
    /// Open-source intelligence gathering.
    #[doc(hidden)]
    Osint,
    /// Continuous monitoring and alerting.
    #[doc(hidden)]
    Monitor,
    /// Command execution on target.
    #[doc(hidden)]
    Execute,
    /// Payload detonation in sandbox.
    #[doc(hidden)]
    Detonate,
    /// Multi-tool orchestration for attack chains.
    #[doc(hidden)]
    Orchestrate,
    /// Search SERAPH knowledge base.
    #[doc(hidden)]
    KnowledgeSearch,
    /// Read from SERAPH knowledge base.
    #[doc(hidden)]
    KnowledgeRead,
    /// Knowledge base statistics.
    #[doc(hidden)]
    KnowledgeStats,
}

impl SeraphAction {
    /// All gateway-routable actions (PUBLIC + WORKFLOW).
    pub const ALL_ROUTABLE: &[Self] = &[
        Self::Status,
        Self::InvestigateStart,
        Self::InvestigateAdvance,
        Self::InvestigateClose,
        Self::InvestigateReport,
        Self::VaultSync,
    ];

    /// Returns `true` for PUBLIC and WORKFLOW actions that are routed through
    /// the Light Architects gateway. Returns `false` for INTERNAL scope-gated
    /// wing actions.
    #[must_use]
    pub const fn is_gateway_routable(&self) -> bool {
        !matches!(
            self,
            Self::Capture
                | Self::Scan
                | Self::Analyze
                | Self::Osint
                | Self::Monitor
                | Self::Execute
                | Self::Detonate
                | Self::Orchestrate
                | Self::KnowledgeSearch
                | Self::KnowledgeRead
                | Self::KnowledgeStats
        )
    }

    /// Returns the canonical snake\_case string used in MCP tool calls.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::InvestigateStart => "investigate_start",
            Self::InvestigateAdvance => "investigate_advance",
            Self::InvestigateClose => "investigate_close",
            Self::InvestigateReport => "investigate_report",
            Self::VaultSync => "vault_sync",
            Self::Capture => "capture",
            Self::Scan => "scan",
            Self::Analyze => "analyze",
            Self::Osint => "osint",
            Self::Monitor => "monitor",
            Self::Execute => "execute",
            Self::Detonate => "detonate",
            Self::Orchestrate => "orchestrate",
            Self::KnowledgeSearch => "knowledge_search",
            Self::KnowledgeRead => "knowledge_read",
            Self::KnowledgeStats => "knowledge_stats",
        }
    }
}

impl fmt::Display for SeraphAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SeraphAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "status" => Ok(Self::Status),
            "investigate_start" => Ok(Self::InvestigateStart),
            "investigate_advance" => Ok(Self::InvestigateAdvance),
            "investigate_close" => Ok(Self::InvestigateClose),
            "investigate_report" => Ok(Self::InvestigateReport),
            "vault_sync" => Ok(Self::VaultSync),
            "capture" => Ok(Self::Capture),
            "scan" => Ok(Self::Scan),
            "analyze" => Ok(Self::Analyze),
            "osint" => Ok(Self::Osint),
            "monitor" => Ok(Self::Monitor),
            "execute" => Ok(Self::Execute),
            "detonate" => Ok(Self::Detonate),
            "orchestrate" => Ok(Self::Orchestrate),
            "knowledge_search" => Ok(Self::KnowledgeSearch),
            "knowledge_read" => Ok(Self::KnowledgeRead),
            "knowledge_stats" => Ok(Self::KnowledgeStats),
            other => Err(format!("unknown SERAPH action: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_routable_count() {
        assert_eq!(SeraphAction::ALL_ROUTABLE.len(), 6);
    }

    #[test]
    fn test_as_str_roundtrip() {
        for &action in SeraphAction::ALL_ROUTABLE {
            let s = action.as_str();
            let parsed: SeraphAction = s.parse().unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(parsed, action, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn test_internal_not_routable() {
        assert!(!SeraphAction::Capture.is_gateway_routable());
        assert!(!SeraphAction::Scan.is_gateway_routable());
        assert!(!SeraphAction::Analyze.is_gateway_routable());
        assert!(!SeraphAction::Osint.is_gateway_routable());
        assert!(!SeraphAction::Monitor.is_gateway_routable());
        assert!(!SeraphAction::Execute.is_gateway_routable());
        assert!(!SeraphAction::Detonate.is_gateway_routable());
        assert!(!SeraphAction::Orchestrate.is_gateway_routable());
        assert!(!SeraphAction::KnowledgeSearch.is_gateway_routable());
        assert!(!SeraphAction::KnowledgeRead.is_gateway_routable());
        assert!(!SeraphAction::KnowledgeStats.is_gateway_routable());
    }

    #[test]
    fn test_from_str() {
        assert_eq!("status".parse::<SeraphAction>(), Ok(SeraphAction::Status));
        assert_eq!(
            "investigate_start".parse::<SeraphAction>(),
            Ok(SeraphAction::InvestigateStart)
        );
        assert_eq!(
            "vault_sync".parse::<SeraphAction>(),
            Ok(SeraphAction::VaultSync)
        );
        assert_eq!("capture".parse::<SeraphAction>(), Ok(SeraphAction::Capture));
        assert!("nonexistent".parse::<SeraphAction>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(
            SeraphAction::InvestigateReport.to_string(),
            "investigate_report"
        );
        assert_eq!(SeraphAction::KnowledgeStats.to_string(), "knowledge_stats");
    }

    #[test]
    fn test_serde_roundtrip() {
        let action = SeraphAction::InvestigateStart;
        let json = serde_json::to_string(&action).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, "\"investigate_start\"");
        let parsed: SeraphAction = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed, action);
    }
}
