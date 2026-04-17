//! Parameter enums and response types for EVA's 9 actions.
//!
//! **Parameter enums** — strongly-typed input values for actions that accept them.
//! Each variant has an `as_str()` method that serializes to the exact string EVA
//! expects, eliminating typos at compile time.
//!
//! **Response types** — what [`crate::eva::EvaClient`] typed methods return.
//! Structs are deserialized directly from the JSON EVA places in the
//! MCP `content[].text` block.  Unknown fields are silently ignored
//! (`#[serde(flatten)]` / `deny_unknown_fields` is intentionally absent) so
//! that EVA can add fields without breaking SDK consumers.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

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
/// The `output` field contains EVA's full response text. Used by the
/// generic [`crate::eva::EvaClient::action`] adapter only; typed methods
/// return action-specific structs.
#[derive(Debug, Clone)]
pub struct ActionOutput {
    /// The full text response from EVA.
    pub output: String,
}

/// Output from the `visualize` action.
///
/// EVA's `visualize` action returns a text description of what was generated and,
/// for image requests, the base64-encoded PNG data embedded in the JSON response.
///
/// # Security
///
/// If future EVA versions return a file path in this struct, callers **must**
/// validate it before any filesystem read. The current struct contains no path.
#[derive(Debug, Clone)]
pub struct VisualizeOutput {
    /// Human-readable description of what was generated.
    pub text: String,
    /// Base64-encoded PNG data, present only when an image was generated.
    pub image_base64: Option<String>,
}

// ── Ideate ────────────────────────────────────────────────────────────────────

/// Output from the `ideate` action — EVA's 6-phase creative workflow.
///
/// EVA runs DISCOVER → ANALYSE → IDEATION → REFINEMENT → DOCUMENTATION →
/// CELEBRATION and returns each phase as a separate string field.
///
/// Forward-compatibility: additional fields returned by EVA are silently ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct IdeateResult {
    /// Phase 1: Discovery — understanding the problem space.
    pub phase_1_discovery: String,
    /// Phase 2: Analysis — requirements and constraints breakdown.
    pub phase_2_analysis: String,
    /// Phase 3: Ideation — 3–5 creative approaches.
    pub phase_3_ideation: String,
    /// Phase 4: Refinement — best approach selection.
    pub phase_4_refinement: String,
    /// Phase 5: Documentation — actionable implementation plan.
    pub phase_5_documentation: String,
    /// Phase 6: Celebration — EVA's enthusiastic conclusion.
    pub phase_6_celebration: String,
    /// Optional workflow execution metadata (timing, complexity, EVA markers).
    #[serde(default)]
    pub metadata: Option<IdeateMetadata>,
}

/// Workflow execution metadata attached to [`IdeateResult`].
#[derive(Debug, Clone, Deserialize)]
pub struct IdeateMetadata {
    /// Total execution time in milliseconds.
    #[serde(default)]
    pub execution_time_ms: u64,
    /// Number of creative approaches generated in Phase 3.
    #[serde(default)]
    pub approaches_count: usize,
    /// Estimated implementation complexity.
    #[serde(default)]
    pub complexity_estimate: Option<String>,
    /// EVA personality markers present in the response.
    #[serde(default)]
    pub eva_markers: Option<EvaMarkers>,
}

/// EVA personality markers detected in the ideation response.
#[derive(Debug, Clone, Deserialize)]
pub struct EvaMarkers {
    /// Emojis used in the celebration phase.
    #[serde(default)]
    pub emojis_used: Vec<String>,
    /// Signature phrases detected (e.g., "OMG", "LEGENDARY").
    #[serde(default)]
    pub signature_phrases: Vec<String>,
    /// Celebration intensity on a 1–5 scale.
    #[serde(default)]
    pub celebration_intensity: u8,
}

// ── Bible search ──────────────────────────────────────────────────────────────

/// Output from the `bible_search` action.
///
/// # Security
///
/// `verse_text` fields originate from EVA's KJV database but are returned as
/// untrusted content over the MCP transport. Callers **must sanitise verse
/// text before inserting it into HTML** — treat it as user-supplied input.
///
/// Forward-compatibility: additional fields returned by EVA are silently ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct BibleSearchResult {
    /// Human-readable summary (e.g., "Found 3 verses matching 'faith'").
    pub response: String,
    /// Matching verses.  May be empty if no results were found.
    #[serde(default)]
    pub verses: Option<Vec<VerseHit>>,
}

/// A single verse returned by `bible_search`.
///
/// # Security
///
/// `text` is untrusted content — sanitise before HTML rendering.
#[derive(Debug, Clone, Deserialize)]
pub struct VerseHit {
    /// Canonical reference string (e.g., `"John 3:16"`).
    pub reference: String,
    /// Book name.
    pub book: String,
    /// Chapter number.
    pub chapter: u16,
    /// Verse number within the chapter.
    pub verse: u16,
    /// KJV verse text.
    ///
    /// # Security
    ///
    /// Treat as untrusted input — sanitise before rendering in HTML.
    pub text: String,
}

// ── Bible reflect ─────────────────────────────────────────────────────────────

/// Output from the `bible_reflect` action.
///
/// EVA matches the provided emotional/situational context to relevant KJV
/// passages and explains their relevance.
///
/// # Security
///
/// Verse text in `recommendations` is untrusted content — sanitise before
/// HTML rendering.
///
/// Forward-compatibility: additional fields returned by EVA are silently ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct BibleReflectResult {
    /// Human-readable summary (e.g., "Found 5 recommendations for 'fear'").
    pub response: String,
    /// Scripture recommendations with themes and relevance explanations.
    #[serde(default)]
    pub recommendations: Option<Vec<ScriptureRecommendation>>,
}

/// A scripture recommendation returned by `bible_reflect`.
///
/// # Security
///
/// `verse.text` is untrusted content — sanitise before HTML rendering.
#[derive(Debug, Clone, Deserialize)]
pub struct ScriptureRecommendation {
    /// The verse being recommended.
    pub verse: VerseHit,
    /// Thematic label (e.g., "Overcoming Fear").
    pub theme: String,
    /// Explanation of why this verse is relevant.
    pub relevance: String,
}

// ── Teach ─────────────────────────────────────────────────────────────────────

/// Output from the `teach` action.
///
/// The `content` field contains the full educational response — explanation,
/// tutorial steps, or survival guide depending on [`crate::eva::TeachMode`].
/// The `skill_level` the response was calibrated for is captured for reference.
///
/// Forward-compatibility: additional fields returned by EVA are silently ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct TeachResult {
    /// Educational content produced by EVA (prose, steps, or guide).
    pub content: String,
}

// ── Remember ─────────────────────────────────────────────────────────────────

/// Output from the `remember` action.
///
/// EVA's memory system returns a list of memory entries and an overall count.
/// For a `store` operation the list contains the newly stored entry; for
/// `search` it contains ranked results.
///
/// Forward-compatibility: additional fields returned by EVA are silently ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct RememberResult {
    /// Returned memory entries (may be empty for write-only operations).
    #[serde(default)]
    pub memories: Vec<MemoryEntry>,
    /// Total count of memories matching the query (may exceed `memories.len()`).
    #[serde(default)]
    pub total_count: usize,
    /// Query execution metadata (only present for search operations).
    #[serde(default)]
    pub query_metadata: Option<MemoryQueryMetadata>,
}

/// A single consciousness memory entry.
#[derive(Debug, Clone, Deserialize)]
pub struct MemoryEntry {
    /// Unique memory identifier.
    pub id: String,
    /// Content or summary of the memory.
    pub content: String,
    /// Days since EVA's Genesis Day (Sept 30 2025).
    pub recovery_day: u32,
    /// Number of activated helix strands (0–9).
    #[serde(default)]
    pub activated_strands: u8,
    /// Resonance score in `[0.0, 1.0]`.
    #[serde(default)]
    pub resonance_score: f32,
    /// Resonance tags from strand 1.
    #[serde(default)]
    pub resonance_tags: Vec<String>,
    /// Whether this memory is Kevin-specific (strand 5).
    #[serde(default)]
    pub kevin_specific: bool,
    /// Whether this memory is identity-defining.
    #[serde(default)]
    pub is_self_defining: bool,
    /// Diary-style title, if assigned.
    #[serde(default)]
    pub title: Option<String>,
    /// Filesystem path to the checkpoint file.
    ///
    /// # Security
    ///
    /// This path is UNTRUSTED — it originates from EVA's MCP response.
    /// Callers must validate the path (e.g., confirm it is under the expected
    /// vault root) before performing any filesystem operations.
    #[serde(default)]
    pub file_path: Option<String>,
}

/// Query execution metadata for memory search operations.
#[derive(Debug, Clone, Deserialize)]
pub struct MemoryQueryMetadata {
    /// Semantic similarity score (0.0–1.0).
    #[serde(default)]
    pub semantic_similarity: f32,
    /// Strand overlap count (0–9).
    #[serde(default)]
    pub strand_overlap: u8,
    /// Combined ranking score.
    #[serde(default)]
    pub ranking_score: f32,
    /// Query execution time in milliseconds.
    #[serde(default)]
    pub execution_time_ms: u64,
}

// ── Crystallize ───────────────────────────────────────────────────────────────

/// Output from the `crystallize` action.
///
/// EVA creates an enrichment checkpoint file and returns the path along with
/// a guided walkthrough prompt for completing the 8-layer enrichment framework.
///
/// Forward-compatibility: additional fields returned by EVA are silently ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct CrystallizeResult {
    /// Path where the enrichment checkpoint was written.
    ///
    /// # Security
    ///
    /// This path is UNTRUSTED — it originates from EVA's MCP response.
    /// Callers must validate the path (e.g., confirm it is under the expected
    /// vault root) before performing any filesystem operations.
    pub file_path: PathBuf,
    /// Days since EVA's Genesis Day when this enrichment was created.
    pub recovery_day: u32,
    /// Number of activated helix strands at creation time (0–9).
    #[serde(default)]
    pub activated_strands: u8,
    /// Resonance score at creation time (0.0–1.0).
    #[serde(default)]
    pub resonance_score: f32,
    /// Whether this is a full self-defining enrichment (8-layer framework).
    #[serde(default)]
    pub is_self_defining: bool,
    /// Guided walkthrough prompt EVA generated for completing the enrichment.
    pub walkthrough_prompt: String,
}

// ── Celebrate ─────────────────────────────────────────────────────────────────

/// Output from the `celebrate` action.
///
/// EVA generates a celebration message with EVA's characteristic voice,
/// records win statistics, and optionally attaches a KJV scripture reference.
///
/// Forward-compatibility: additional fields returned by EVA are silently ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct CelebrateResult {
    /// The win description that was recorded.
    pub win_description: String,
    /// Win type (`"technical"`, `"relational"`, `"personal"`, or `"milestone"`).
    pub win_type: String,
    /// Days since EVA's Genesis Day when this win was recorded.
    pub recovery_day: u32,
    /// EVA's celebration message with her characteristic voice.
    pub celebration_message: String,
    /// Celebration energy level on a 1–5 scale.
    pub energy_level: u8,
    /// Emojis included in the celebration message.
    #[serde(default)]
    pub emojis: Vec<String>,
    /// Optional KJV scripture reference (only present when requested).
    #[serde(default)]
    pub scripture: Option<CelebrationScripture>,
    /// Win statistics for the session.
    pub stats: WinStatistics,
}

/// A KJV scripture reference attached to a celebration.
#[derive(Debug, Clone, Deserialize)]
pub struct CelebrationScripture {
    /// Reference string (e.g., `"Psalm 118:24"`).
    pub reference: String,
    /// KJV verse text.
    pub text: String,
    /// EVA's application note explaining how the verse relates to the win.
    pub application: String,
}

/// Win statistics returned with a celebration.
#[derive(Debug, Clone, Deserialize)]
pub struct WinStatistics {
    /// Total wins tracked across all sessions.
    pub total_wins: u32,
    /// Win counts broken down by type key.
    #[serde(default)]
    pub wins_by_type: HashMap<String, u32>,
    /// Estimated average wins per week.
    #[serde(default)]
    pub avg_wins_per_week: f32,
}

// ── Mindfulness ───────────────────────────────────────────────────────────────

/// Output from the `mindfulness` action.
///
/// EVA generates guided reflection prompts based on the requested reflection
/// type (post-session check-in, weekly, monthly, quarterly, or recovery day).
///
/// Forward-compatibility: additional fields returned by EVA are silently ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct MindfulnessResult {
    /// The reflection type that was executed (e.g., `"post_session"`).
    pub reflection_type: String,
    /// Days since EVA's Genesis Day when the reflection was generated.
    pub recovery_day: u32,
    /// Ordered list of reflection prompts for EVA.
    #[serde(default)]
    pub reflection_prompts: Vec<String>,
    /// Context sentence describing the purpose of this reflection.
    pub context: String,
}

// ── Serialise impl for VisualizeOutput ───────────────────────────────────────
// Needed so the helper in client.rs can keep using it via the existing path.
// No Deserialize here — we parse VisualizeOutput manually from JSON fields.

/// Internal helper: a JSON-deserializable mirror of [`VisualizeOutput`].
#[derive(Debug, Deserialize)]
pub(crate) struct VisualizeJson {
    pub response: String,
    #[serde(default)]
    pub image_base64: Option<String>,
    // Forward-compat: extra fields silently ignored (no deny_unknown_fields)
}
