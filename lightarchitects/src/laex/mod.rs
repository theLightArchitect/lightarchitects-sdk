//! Typed client for LÆX's `laexTools` gateway-dispatched orchestrator.
//!
//! LÆX is the canon-keeper / governance-umbrella sibling. Unlike SOUL/CORSO/EVA/
//! QUANTUM/SERAPH, LÆX has no standalone stdio binary — it runs **inline**
//! within the lightarchitects-gateway and dispatches into existing core_tools
//! (`canon_check`, `canon_evaluate`) plus inline-defined helpers for the four
//! governance-layer reviews.
//!
//! LÆX exposes 9 gateway-routable actions:
//! `canon_check`, `canon_evaluate`, `matrix_ratify`, `effectiveness_score`,
//! `reflect`, `layer1_review`, `layer2_review`, `layer3_review`, `layer4_review`.
//!
//! Two internal actions (`register_decision`, `query_canon_drift`) exist for
//! gateway-internal bookkeeping but are never routed through the public surface.
//!
//! # Quick start
//!
//! ```no_run
//! # async fn example() -> Result<(), lightarchitects::core::SdkError> {
//! use lightarchitects::laex::LaexClient;
//!
//! let client = LaexClient::builder().api_key("la_your_key_here").build()?;
//!
//! let check = client.canon_check("ship hot-fix without test", false).await?;
//! println!("{}", check.framework);
//!
//! let score = client
//!     .effectiveness_score("plan-id-123")
//!     .await?;
//! println!("score = {} (rubric = {})", score.score, score.rubric);
//! # Ok(()) }
//! ```

/// Canonical LÆX action enum — governance, canon-check, layer reviews.
pub mod actions;
mod client;
mod content;
mod types;

// ── Public API surface ────────────────────────────────────────────────────────

pub use actions::LaexAction;
pub use client::{LaexClient, LaexClientBuilder};
pub use types::{
    ActionOutput, CanonCheckResult, CanonEvaluateResult, EffectivenessScoreResult, GovernanceLayer,
    LayerReviewResult, MatrixRatifyResult, QueryCanonDriftResult, ReflectResult,
    RegisterDecisionResult,
};
