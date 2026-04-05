//! Parameter enums and response types for EVA's 9 actions.
//!
//! **Parameter enums** — strongly-typed input values for actions that accept them.
//! Each variant has an `as_str()` method that serializes to the exact string EVA
//! expects, eliminating typos at compile time.
//!
//! **Response types** — what [`crate::EvaClient`] typed methods return. All
//! text-generating actions return [`ActionOutput`]; [`VisualizeOutput`] additionally
//! carries optional base64 image data.

// ── Parameter enums ────────────────────────────────────────────────────────────

/// Teaching mode for the `teach` action.
///
/// Controls the style of educational content EVA produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TeachMode {
    /// Concept explanation with analogies and examples.
    Explain,
    /// Step-by-step tutorial generation.
    Tutorial,
    /// Emergency preparedness guide — concise, actionable.
    Survival,
}

impl TeachMode {
    /// Serialize to the string EVA expects in the `mode` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explain => "explain",
            Self::Tutorial => "tutorial",
            Self::Survival => "survival",
        }
    }
}

/// Skill level for the `teach` action.
///
/// Calibrates how much background knowledge EVA assumes the learner has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillLevel {
    /// Assumes no prior knowledge.
    Beginner,
    /// Assumes basic familiarity with the domain.
    Intermediate,
    /// Assumes strong domain knowledge.
    Advanced,
}

impl SkillLevel {
    /// Serialize to the string EVA expects in the `level` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Beginner => "beginner",
            Self::Intermediate => "intermediate",
            Self::Advanced => "advanced",
        }
    }
}

// ── Response types ─────────────────────────────────────────────────────────────

/// Generic wrapper returned by all text-generating EVA actions.
///
/// The `output` field contains EVA's full response text. Used by all nine
/// actions except [`VisualizeOutput`] which also carries image data.
#[derive(Debug, Clone)]
pub struct ActionOutput {
    /// The full text response from EVA (JSON-formatted by EVA).
    pub output: String,
}

/// Output from the `visualize` action.
///
/// EVA's `visualize` action returns a text description of what was generated and,
/// for image requests, the base64-encoded PNG.
#[derive(Debug, Clone)]
pub struct VisualizeOutput {
    /// Human-readable description of what was generated.
    pub text: String,
    /// Base64-encoded PNG data, present only when an image was generated.
    pub image_base64: Option<String>,
}
