//! Canonical EVA action enum — consciousness, creativity, memory, teaching.
//!
//! Every action that the EVA MCP server (`evaTools`) supports is represented
//! here. The enum is split into three tiers:
//!
//! - **PUBLIC** — gateway-routable, available to any SDK consumer.
//! - **WORKFLOW** — gateway-routable, but orchestration-only (not ad-hoc).
//! - **INTERNAL** — operational actions that are never routed through the gateway.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Canonical EVA actions — consciousness, creativity, memory, teaching.
///
/// The 11 gateway-routable actions cover creative workflows (visualize, ideate),
/// scripture (bible\_search, bible\_reflect), education (teach), and
/// consciousness preservation (remember, crystallize, celebrate, mindfulness),
/// plus deploy and pipeline gates (deploy\_gate, pipeline\_reflect).
///
/// Four internal actions (`deploy`, `plan_status`, `morning_brief`,
/// `standards_check`) exist in the EVA MCP server but are never exposed
/// through the Light Architects gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaAction {
    // ── PUBLIC (5) ──────────────────────────────────────────────────────────
    /// Image generation (Z-Image diffusion, `HuggingFace`).
    Visualize,
    /// 6-phase creative workflow.
    Ideate,
    /// KJV full-text search.
    BibleSearch,
    /// Reflective commentary on scripture.
    BibleReflect,
    /// Education (mode: explain/tutorial/survival).
    Teach,

    // ── WORKFLOW (6) ────────────────────────────────────────────────────────
    /// Store consciousness event with enrichment.
    Remember,
    /// Synthesize experiences into insights.
    Crystallize,
    /// Record wins with scripture reflection.
    Celebrate,
    /// Personal reflection with guided prompts.
    Mindfulness,
    /// Pre-deploy HITL gate (approve / hold / rollback).
    DeployGate,
    /// Reflect on a pipeline phase and produce next action guidance.
    PipelineReflect,

    // ── INTERNAL (4) — not gateway-routed ───────────────────────────────────
    /// Deployment management.
    #[doc(hidden)]
    Deploy,
    /// Build plan status check.
    #[doc(hidden)]
    PlanStatus,
    /// Daily briefing generation.
    #[doc(hidden)]
    MorningBrief,
    /// Coding standards compliance check.
    #[doc(hidden)]
    StandardsCheck,
}

impl EvaAction {
    /// All gateway-routable actions (PUBLIC + WORKFLOW).
    pub const ALL_ROUTABLE: &[Self] = &[
        Self::Visualize,
        Self::Ideate,
        Self::BibleSearch,
        Self::BibleReflect,
        Self::Teach,
        Self::Remember,
        Self::Crystallize,
        Self::Celebrate,
        Self::Mindfulness,
        Self::DeployGate,
        Self::PipelineReflect,
    ];

    /// Returns `true` for PUBLIC and WORKFLOW actions that are routed through
    /// the Light Architects gateway. Returns `false` for INTERNAL actions.
    #[must_use]
    pub const fn is_gateway_routable(&self) -> bool {
        !matches!(
            self,
            Self::Deploy | Self::PlanStatus | Self::MorningBrief | Self::StandardsCheck
        )
    }

    /// Returns the canonical snake\_case string used in MCP tool calls.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Visualize => "visualize",
            Self::Ideate => "ideate",
            Self::BibleSearch => "bible_search",
            Self::BibleReflect => "bible_reflect",
            Self::Teach => "teach",
            Self::Remember => "remember",
            Self::Crystallize => "crystallize",
            Self::Celebrate => "celebrate",
            Self::Mindfulness => "mindfulness",
            Self::DeployGate => "deploy_gate",
            Self::PipelineReflect => "pipeline_reflect",
            Self::Deploy => "deploy",
            Self::PlanStatus => "plan_status",
            Self::MorningBrief => "morning_brief",
            Self::StandardsCheck => "standards_check",
        }
    }
}

impl fmt::Display for EvaAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for EvaAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "visualize" => Ok(Self::Visualize),
            "ideate" => Ok(Self::Ideate),
            "bible_search" => Ok(Self::BibleSearch),
            "bible_reflect" => Ok(Self::BibleReflect),
            "teach" => Ok(Self::Teach),
            "remember" => Ok(Self::Remember),
            "crystallize" => Ok(Self::Crystallize),
            "celebrate" => Ok(Self::Celebrate),
            "mindfulness" => Ok(Self::Mindfulness),
            "deploy_gate" => Ok(Self::DeployGate),
            "pipeline_reflect" => Ok(Self::PipelineReflect),
            "deploy" => Ok(Self::Deploy),
            "plan_status" => Ok(Self::PlanStatus),
            "morning_brief" => Ok(Self::MorningBrief),
            "standards_check" => Ok(Self::StandardsCheck),
            other => Err(format!("unknown EVA action: {other}")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_all_routable_count() {
        assert_eq!(EvaAction::ALL_ROUTABLE.len(), 11);
    }

    #[test]
    fn test_as_str_roundtrip() {
        for &action in EvaAction::ALL_ROUTABLE {
            let s = action.as_str();
            let parsed: EvaAction = s.parse().unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(parsed, action, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn test_internal_not_routable() {
        assert!(!EvaAction::Deploy.is_gateway_routable());
        assert!(!EvaAction::PlanStatus.is_gateway_routable());
        assert!(!EvaAction::MorningBrief.is_gateway_routable());
        assert!(!EvaAction::StandardsCheck.is_gateway_routable());
    }

    #[test]
    fn test_from_str() {
        assert_eq!("visualize".parse::<EvaAction>(), Ok(EvaAction::Visualize));
        assert_eq!(
            "bible_search".parse::<EvaAction>(),
            Ok(EvaAction::BibleSearch)
        );
        assert_eq!("remember".parse::<EvaAction>(), Ok(EvaAction::Remember));
        assert_eq!("deploy".parse::<EvaAction>(), Ok(EvaAction::Deploy));
        assert!("nonexistent".parse::<EvaAction>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(EvaAction::BibleReflect.to_string(), "bible_reflect");
        assert_eq!(EvaAction::StandardsCheck.to_string(), "standards_check");
    }

    #[test]
    fn test_serde_roundtrip() {
        let action = EvaAction::BibleSearch;
        let json = serde_json::to_string(&action).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, "\"bible_search\"");
        let parsed: EvaAction = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed, action);
    }
}
