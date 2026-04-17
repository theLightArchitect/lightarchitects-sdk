//! Canonical QUANTUM action enum — forensic investigation lifecycle.
//!
//! Every action that the QUANTUM MCP server (`qsTools`) supports is represented
//! here. The enum is split into three tiers:
//!
//! - **WORKFLOW** — gateway-routable, investigation lifecycle phases.
//! - **PUBLIC** — gateway-routable, utility actions.
//! - **INTERNAL** — orchestration primitives never routed through the gateway.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Canonical QUANTUM actions — forensic investigation lifecycle.
///
/// The 9 gateway-routable actions cover a complete investigation cycle
/// (triage -> sweep -> trace -> probe -> theorize -> verify -> close)
/// plus two utility actions (quick, research).
///
/// Four internal actions (`list`, `discover`, `execute`, `workflow`) handle
/// orchestration and are never exposed through the Light Architects gateway.
///
/// # Rename: `scan` -> `triage`
///
/// The `scan` action was renamed to `triage` to resolve a 3-way collision
/// with CORSO `guard` and SERAPH `scan`. The `#[serde(alias = "scan")]`
/// attribute provides backward compatibility for existing payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantumAction {
    // ── WORKFLOW — Investigation lifecycle (7) ──────────────────────────────
    /// Phase 1 — initial discovery (renamed from scan).
    #[serde(alias = "scan")]
    Triage,
    /// Phase 2 — evidence collection.
    Sweep,
    /// Phase 3 — pattern forensics.
    Trace,
    /// Phase 4 — multi-source enrichment.
    Probe,
    /// Phase 5 — hypothesis generation.
    Theorize,
    /// Phase 6 — solution validation.
    Verify,
    /// Phase 7 — deliverable generation.
    Close,

    // ── PUBLIC (2) ──────────────────────────────────────────────────────────
    /// Abbreviated 3-phase investigation.
    Quick,
    /// Unified multi-source research.
    Research,

    // ── INTERNAL (4) — not gateway-routed ───────────────────────────────────
    /// List active investigations.
    #[doc(hidden)]
    List,
    /// Discover investigation targets.
    #[doc(hidden)]
    Discover,
    /// Execute an investigation step.
    #[doc(hidden)]
    Execute,
    /// Manage investigation workflow state.
    #[doc(hidden)]
    Workflow,
}

impl QuantumAction {
    /// All gateway-routable actions (WORKFLOW + PUBLIC).
    pub const ALL_ROUTABLE: &[Self] = &[
        Self::Triage,
        Self::Sweep,
        Self::Trace,
        Self::Probe,
        Self::Theorize,
        Self::Verify,
        Self::Close,
        Self::Quick,
        Self::Research,
    ];

    /// Returns `true` for WORKFLOW and PUBLIC actions that are routed through
    /// the Light Architects gateway. Returns `false` for INTERNAL actions.
    #[must_use]
    pub const fn is_gateway_routable(&self) -> bool {
        !matches!(
            self,
            Self::List | Self::Discover | Self::Execute | Self::Workflow
        )
    }

    /// Returns the canonical snake\_case string used in MCP tool calls.
    ///
    /// Note: `Triage` returns `"triage"` (not `"scan"`). Use the serde alias
    /// for backward-compatible deserialization of `"scan"` payloads.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Triage => "triage",
            Self::Sweep => "sweep",
            Self::Trace => "trace",
            Self::Probe => "probe",
            Self::Theorize => "theorize",
            Self::Verify => "verify",
            Self::Close => "close",
            Self::Quick => "quick",
            Self::Research => "research",
            Self::List => "list",
            Self::Discover => "discover",
            Self::Execute => "execute",
            Self::Workflow => "workflow",
        }
    }
}

impl fmt::Display for QuantumAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for QuantumAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "triage" | "scan" => Ok(Self::Triage),
            "sweep" => Ok(Self::Sweep),
            "trace" => Ok(Self::Trace),
            "probe" => Ok(Self::Probe),
            "theorize" => Ok(Self::Theorize),
            "verify" => Ok(Self::Verify),
            "close" => Ok(Self::Close),
            "quick" => Ok(Self::Quick),
            "research" => Ok(Self::Research),
            "list" => Ok(Self::List),
            "discover" => Ok(Self::Discover),
            "execute" => Ok(Self::Execute),
            "workflow" => Ok(Self::Workflow),
            other => Err(format!("unknown QUANTUM action: {other}")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_all_routable_count() {
        assert_eq!(QuantumAction::ALL_ROUTABLE.len(), 9);
    }

    #[test]
    fn test_as_str_roundtrip() {
        for &action in QuantumAction::ALL_ROUTABLE {
            let s = action.as_str();
            let parsed: QuantumAction = s.parse().unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(parsed, action, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn test_internal_not_routable() {
        assert!(!QuantumAction::List.is_gateway_routable());
        assert!(!QuantumAction::Discover.is_gateway_routable());
        assert!(!QuantumAction::Execute.is_gateway_routable());
        assert!(!QuantumAction::Workflow.is_gateway_routable());
    }

    #[test]
    fn test_from_str() {
        assert_eq!("triage".parse::<QuantumAction>(), Ok(QuantumAction::Triage));
        assert_eq!("quick".parse::<QuantumAction>(), Ok(QuantumAction::Quick));
        assert_eq!("list".parse::<QuantumAction>(), Ok(QuantumAction::List));
        assert!("nonexistent".parse::<QuantumAction>().is_err());
    }

    #[test]
    fn test_scan_alias() {
        // FromStr backward compat
        assert_eq!("scan".parse::<QuantumAction>(), Ok(QuantumAction::Triage));

        // Serde backward compat
        let parsed: QuantumAction =
            serde_json::from_str("\"scan\"").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed, QuantumAction::Triage);
    }

    #[test]
    fn test_triage_serializes_as_triage() {
        let json = serde_json::to_string(&QuantumAction::Triage).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, "\"triage\"");
    }

    #[test]
    fn test_display() {
        assert_eq!(QuantumAction::Theorize.to_string(), "theorize");
        assert_eq!(QuantumAction::Triage.to_string(), "triage");
    }

    #[test]
    fn test_serde_roundtrip() {
        let action = QuantumAction::Research;
        let json = serde_json::to_string(&action).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, "\"research\"");
        let parsed: QuantumAction = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed, action);
    }
}
