//! Parameter enums and response types for LÆX's 9 routable actions.
//!
//! **Parameter enums** — strongly-typed input values for actions that accept them.
//! [`GovernanceLayer`] enumerates the 4 LÆX governance layers (Security /
//! Methodology / Product / Ethics).
//!
//! **Response types** — what [`crate::laex::LaexClient`] typed methods return.
//! Structs are deserialized directly from the JSON LÆX places in the MCP
//! `content[].text` block. Unknown fields are silently ignored
//! (`deny_unknown_fields` is intentionally absent) so that the gateway can
//! add fields without breaking SDK consumers.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

// ── Parameter enums ────────────────────────────────────────────────────────────

/// LÆX governance layers (Layer 1 through Layer 4).
///
/// Maps to the 4-layer model in the SDLC coverage map: Security canon,
/// Methodology canon, Product gate (Northstar fit), Ethics + Compliance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GovernanceLayer {
    /// Layer 1 — Security canon (threat model, baselines, hardening).
    Security,
    /// Layer 2 — Methodology canon (LASDLC compliance, gates, citations).
    Methodology,
    /// Layer 3 — Product gate (Northstar fit + ICP alignment).
    Product,
    /// Layer 4 — Ethics + Compliance canon.
    Ethics,
}

impl GovernanceLayer {
    /// Serialize to the canonical layer-id string the gateway expects.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Security => "layer1_security",
            Self::Methodology => "layer2_methodology",
            Self::Product => "layer3_product",
            Self::Ethics => "layer4_ethics",
        }
    }

    /// Numeric layer index (1–4).
    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::Security => 1,
            Self::Methodology => 2,
            Self::Product => 3,
            Self::Ethics => 4,
        }
    }
}

// ── Response types ─────────────────────────────────────────────────────────────

/// Generic wrapper returned by all text-generating LÆX actions.
///
/// The `output` field contains LÆX's full response text. Used by the generic
/// [`crate::laex::LaexClient::action`] adapter only; typed methods return
/// action-specific structs.
#[derive(Debug, Clone)]
pub struct ActionOutput {
    /// The full text response from LÆX.
    pub output: String,
}

// ── Canon check ───────────────────────────────────────────────────────────────

/// Output from the `canon_check` action.
///
/// Wraps the existing `core_tools/canon_check.rs::run` function. Returns the
/// canonical-context headers relevant to a candidate decision so the model
/// can self-evaluate against the canon registry.
///
/// Forward-compatibility: additional fields returned by the gateway are
/// silently ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct CanonCheckResult {
    /// The decision string that was checked.
    #[serde(default)]
    pub decision: String,
    /// Concatenated canonical-context framework prepared for the model.
    #[serde(default)]
    pub framework: String,
    /// List of canon-registry headers that matched the decision context.
    #[serde(default)]
    pub canon_headers: Vec<String>,
    /// When `verbose=true` was requested, full canon excerpts.
    #[serde(default)]
    pub verbose_excerpts: Option<Vec<String>>,
}

// ── Canon evaluate ────────────────────────────────────────────────────────────

/// Output from the `canon_evaluate` action.
///
/// Wraps `core_tools/canon_evaluate.rs::run`. Returns the 5-criteria framework
/// for evaluating a candidate decision (`convergent_evidence`, `biblical_grounding`,
/// `decision_shaping`, `pressure_tested`, `kevin_ratifies`).
///
/// Forward-compatibility: additional criteria fields are silently ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct CanonEvaluateResult {
    /// The candidate string that was evaluated.
    #[serde(default)]
    pub candidate: String,
    /// 5-criteria framework returned as criterion-name → prompt mapping.
    #[serde(default)]
    pub criteria: BTreeMap<String, String>,
    /// Optional summary line LÆX produces to orient the evaluator.
    #[serde(default)]
    pub summary: Option<String>,
}

// ── Matrix ratify ─────────────────────────────────────────────────────────────

/// Output from the `matrix_ratify` action.
///
/// 4-layer governance audit over a build manifest (Security / Methodology /
/// Product / Ethics). Each layer returns a verdict + rationale.
#[derive(Debug, Clone, Deserialize)]
pub struct MatrixRatifyResult {
    /// Build manifest path that was ratified.
    #[serde(default)]
    pub manifest_path: String,
    /// Per-layer verdict map. Keys are `layer1_security` through
    /// `layer4_ethics`. Values are structured layer-review payloads.
    #[serde(default)]
    pub layers: BTreeMap<String, LayerReviewResult>,
    /// Overall ratification verdict (e.g. `"APPROVED"`, `"APPROVED_WITH_CONDITIONS"`,
    /// `"BLOCKED"`).
    #[serde(default)]
    pub overall_verdict: String,
    /// Free-form rationale summarising the cross-layer synthesis.
    #[serde(default)]
    pub rationale: Option<String>,
}

// ── Effectiveness score ───────────────────────────────────────────────────────

/// Output from the `effectiveness_score` action.
///
/// Scores a plan against `lasdlc-effectiveness-rubric.md` (C1–C8 rubric).
/// Returns the overall score plus per-criterion breakdown.
#[derive(Debug, Clone, Deserialize)]
pub struct EffectivenessScoreResult {
    /// Plan ID or codename that was scored.
    #[serde(default)]
    pub plan_id: String,
    /// Rubric identifier (`"lasdlc-effectiveness-rubric"`).
    #[serde(default)]
    pub rubric: String,
    /// Overall score (typical range: 0.0–10.0).
    #[serde(default)]
    pub score: f32,
    /// Per-criterion score breakdown (criterion-id → score).
    #[serde(default)]
    pub criterion_scores: BTreeMap<String, f32>,
    /// Optional narrative explaining each criterion's score.
    #[serde(default)]
    pub narrative: Option<String>,
}

// ── Reflect ───────────────────────────────────────────────────────────────────

/// Output from the `reflect` action.
///
/// Retrospective canon-evaluation ritual (Phase 6 Learn deliverable).
/// Returns themes, gaps, and follow-up recommendations.
#[derive(Debug, Clone, Deserialize)]
pub struct ReflectResult {
    /// Reflection scope (e.g. build codename, time window).
    #[serde(default)]
    pub scope: String,
    /// Identified themes / patterns across the reflection scope.
    #[serde(default)]
    pub themes: Vec<String>,
    /// Identified gaps where canonical guidance was absent or unclear.
    #[serde(default)]
    pub gaps: Vec<String>,
    /// Recommended follow-up actions or canon entries to author.
    #[serde(default)]
    pub follow_ups: Vec<String>,
    /// Optional narrative summary.
    #[serde(default)]
    pub summary: Option<String>,
}

// ── Layer review ──────────────────────────────────────────────────────────────

/// Output from a single layer review (`layer1_review` through `layer4_review`).
///
/// Used both as the standalone return type for direct layer reviews and as
/// the per-layer payload inside [`MatrixRatifyResult::layers`].
#[derive(Debug, Clone, Deserialize)]
pub struct LayerReviewResult {
    /// Numeric layer index (1–4).
    #[serde(default)]
    pub layer: u8,
    /// Layer name (e.g. `"security"`, `"methodology"`, `"product"`, `"ethics"`).
    #[serde(default)]
    pub layer_name: String,
    /// Verdict (`"PASS"`, `"PASS_WITH_CONDITIONS"`, `"FAIL"`).
    #[serde(default)]
    pub verdict: String,
    /// Rationale explaining the verdict.
    #[serde(default)]
    pub rationale: String,
    /// Findings discovered during the review.
    #[serde(default)]
    pub findings: Vec<String>,
    /// Conditions that must hold for verdict to remain valid.
    #[serde(default)]
    pub conditions: Vec<String>,
}

// ── Internal action results ───────────────────────────────────────────────────

/// Output from the internal `register_decision` action.
///
/// Confirms a ratification record was appended to the canon decision-registry.
/// Internal action — not gateway-routed.
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterDecisionResult {
    /// Registry record id (uuid or sequence number).
    #[serde(default)]
    pub record_id: String,
    /// Wall-clock timestamp when the record was appended (ISO8601).
    #[serde(default)]
    pub timestamp: String,
    /// Decision string that was registered.
    #[serde(default)]
    pub decision: String,
}

/// Output from the internal `query_canon_drift` action.
///
/// Reports drift between the in-memory canon registry and the platform helix
/// authoritative state. Internal action — not gateway-routed.
#[derive(Debug, Clone, Deserialize)]
pub struct QueryCanonDriftResult {
    /// Number of canon entries that differ between local and helix.
    #[serde(default)]
    pub drift_count: usize,
    /// Per-entry drift detail (canon-id → drift descriptor).
    #[serde(default)]
    pub drifts: BTreeMap<String, Value>,
    /// Whether the registry can be auto-reconciled or needs HITL.
    #[serde(default)]
    pub auto_reconcilable: bool,
}
