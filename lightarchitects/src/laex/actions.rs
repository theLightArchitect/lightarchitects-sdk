//! Canonical LÆX action enum — governance, canon-check, layer reviews.
//!
//! Every action that the LÆX inline gateway handler supports is represented
//! here. The enum is split into three tiers (mirroring `EvaAction`):
//!
//! - **PUBLIC** — gateway-routable, available to any SDK consumer.
//! - **WORKFLOW** — gateway-routable, but orchestration-only (not ad-hoc).
//! - **INTERNAL** — gateway-internal bookkeeping, never routed publicly.
//!
//! Priority routing rationale (per `orchestrate.rs` SIBLING_ROUTES slot 0):
//! LÆX governance trumps research on canon-related action collisions.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Canonical LÆX actions — governance, canon-check, 4-layer reviews.
///
/// The 9 gateway-routable actions cover canon-check (wraps existing
/// `core_tools/canon_check.rs`), canon-evaluate (wraps `canon_evaluate.rs`),
/// 4-layer governance audits (`matrix_ratify`, `layer1_review` through
/// `layer4_review`), effectiveness scoring (`effectiveness_score` invokes
/// `lasdlc-effectiveness-rubric.md`), and reflection (`reflect` retro
/// canon-evaluation).
///
/// Two internal actions (`register_decision`, `query_canon_drift`) exist for
/// inline gateway bookkeeping — never exposed through the public route table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaexAction {
    // ── PUBLIC (3) ──────────────────────────────────────────────────────────
    /// Read-only canon-registry consultation: returns canonical-context headers
    /// for a candidate decision. Wraps `core_tools/canon_check.rs::run`.
    CanonCheck,
    /// 5-criteria evaluation framework against a candidate decision. Wraps
    /// `core_tools/canon_evaluate.rs::run`.
    CanonEvaluate,
    /// 4-layer governance audit (Security/Methodology/Product/Ethics) over a
    /// build manifest, returning a ratification matrix.
    MatrixRatify,

    // ── WORKFLOW (6) ────────────────────────────────────────────────────────
    /// Score a plan against `lasdlc-effectiveness-rubric.md` (C1–C8 rubric).
    EffectivenessScore,
    /// Retrospective canon-evaluation ritual (Phase 6 Learn deliverable).
    Reflect,
    /// Layer 1 review — Security canon (threat model, baselines, hardening).
    Layer1Review,
    /// Layer 2 review — Methodology canon (LASDLC compliance, gates, citations).
    Layer2Review,
    /// Layer 3 review — Product gate (Northstar fit + ICP alignment).
    Layer3Review,
    /// Layer 4 review — Ethics + Compliance canon.
    Layer4Review,

    // ── INTERNAL (2) — not gateway-routed ───────────────────────────────────
    /// Append a ratification record to the inline canon decision-registry.
    #[doc(hidden)]
    RegisterDecision,
    /// Compute drift between current canon registry and the platform helix.
    #[doc(hidden)]
    QueryCanonDrift,
}

impl LaexAction {
    /// All gateway-routable actions (PUBLIC + WORKFLOW). Length = 9.
    pub const ALL_ROUTABLE: &[Self] = &[
        Self::CanonCheck,
        Self::CanonEvaluate,
        Self::MatrixRatify,
        Self::EffectivenessScore,
        Self::Reflect,
        Self::Layer1Review,
        Self::Layer2Review,
        Self::Layer3Review,
        Self::Layer4Review,
    ];

    /// Returns `true` for PUBLIC and WORKFLOW actions that are routed through
    /// the Light Architects gateway. Returns `false` for INTERNAL actions.
    #[must_use]
    pub const fn is_gateway_routable(&self) -> bool {
        !matches!(self, Self::RegisterDecision | Self::QueryCanonDrift)
    }

    /// Returns the canonical `snake_case` string used in MCP tool calls.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::CanonCheck => "canon_check",
            Self::CanonEvaluate => "canon_evaluate",
            Self::MatrixRatify => "matrix_ratify",
            Self::EffectivenessScore => "effectiveness_score",
            Self::Reflect => "reflect",
            Self::Layer1Review => "layer1_review",
            Self::Layer2Review => "layer2_review",
            Self::Layer3Review => "layer3_review",
            Self::Layer4Review => "layer4_review",
            Self::RegisterDecision => "register_decision",
            Self::QueryCanonDrift => "query_canon_drift",
        }
    }
}

impl fmt::Display for LaexAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for LaexAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "canon_check" => Ok(Self::CanonCheck),
            "canon_evaluate" => Ok(Self::CanonEvaluate),
            "matrix_ratify" => Ok(Self::MatrixRatify),
            "effectiveness_score" => Ok(Self::EffectivenessScore),
            "reflect" => Ok(Self::Reflect),
            "layer1_review" => Ok(Self::Layer1Review),
            "layer2_review" => Ok(Self::Layer2Review),
            "layer3_review" => Ok(Self::Layer3Review),
            "layer4_review" => Ok(Self::Layer4Review),
            "register_decision" => Ok(Self::RegisterDecision),
            "query_canon_drift" => Ok(Self::QueryCanonDrift),
            other => Err(format!("unknown LÆX action: {other}")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_all_routable_count() {
        assert_eq!(LaexAction::ALL_ROUTABLE.len(), 9);
    }

    #[test]
    fn test_as_str_roundtrip() {
        for &action in LaexAction::ALL_ROUTABLE {
            let s = action.as_str();
            let parsed: LaexAction = s.parse().unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(parsed, action, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn test_internal_not_routable() {
        assert!(!LaexAction::RegisterDecision.is_gateway_routable());
        assert!(!LaexAction::QueryCanonDrift.is_gateway_routable());
    }

    #[test]
    fn test_from_str() {
        assert_eq!(
            "canon_check".parse::<LaexAction>(),
            Ok(LaexAction::CanonCheck)
        );
        assert_eq!(
            "matrix_ratify".parse::<LaexAction>(),
            Ok(LaexAction::MatrixRatify)
        );
        assert_eq!(
            "effectiveness_score".parse::<LaexAction>(),
            Ok(LaexAction::EffectivenessScore)
        );
        assert_eq!(
            "register_decision".parse::<LaexAction>(),
            Ok(LaexAction::RegisterDecision)
        );
        assert!("nonexistent".parse::<LaexAction>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(LaexAction::CanonCheck.to_string(), "canon_check");
        assert_eq!(LaexAction::Layer3Review.to_string(), "layer3_review");
        assert_eq!(LaexAction::QueryCanonDrift.to_string(), "query_canon_drift");
    }

    #[test]
    fn test_serde_roundtrip() {
        let action = LaexAction::EffectivenessScore;
        let json = serde_json::to_string(&action).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, "\"effectiveness_score\"");
        let parsed: LaexAction = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed, action);
    }

    #[test]
    fn test_routable_excludes_internal() {
        for &action in LaexAction::ALL_ROUTABLE {
            assert!(
                action.is_gateway_routable(),
                "{action:?} in ALL_ROUTABLE must be gateway-routable"
            );
        }
    }
}
