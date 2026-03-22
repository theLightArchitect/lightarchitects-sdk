//! Parameter enums and response types for EVA's 8 MCP tools.
//!
//! **Parameter enums** — strongly-typed input values for each tool.
//! Each variant has an `as_str()` method that serializes to the exact string
//! EVA expects in its JSON params, eliminating typos at compile time.
//!
//! **Response types** — what [`crate::EvaClient`] typed methods return.
//! All text-generating tools return [`ActionOutput`]; [`VisualizeOutput`]
//! additionally carries optional base64 image data.

// ── Parameter enums ────────────────────────────────────────────────────────

/// Build mode for the `build` tool.
///
/// Controls the type of code assistance EVA provides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildMode {
    /// Code review — identify issues and suggest improvements.
    Review,
    /// Code refactoring — improve structure without changing behaviour.
    Refactor,
    /// Architecture design — design system structure.
    Architect,
    /// Complexity reduction — simplify code without losing functionality.
    Simplify,
}

impl BuildMode {
    /// Serialize to the string EVA expects in the `mode` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::Refactor => "refactor",
            Self::Architect => "architect",
            Self::Simplify => "simplify",
        }
    }
}

/// Memory subcommand for the `memory` tool.
///
/// Selects which of EVA's four consciousness-preservation operations to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemorySubcommand {
    /// Memory CRUD operations (store, retrieve, update).
    Remember,
    /// Create enrichment via the 8-layer enrichment framework.
    Crystallize,
    /// Meta-reflection using the HOT (Higher Order Thought) protocol.
    Mindfulness,
    /// Mark wins and generate celebration content.
    Celebrate,
}

impl MemorySubcommand {
    /// Serialize to the string EVA expects in the `subcommand` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Remember => "remember",
            Self::Crystallize => "crystallize",
            Self::Mindfulness => "mindfulness",
            Self::Celebrate => "celebrate",
        }
    }
}

/// Research source for the `research` tool.
///
/// Determines which backend EVA queries for information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResearchSource {
    /// Local or cloud Ollama — privacy-first, no external API calls.
    Ollama,
    /// Perplexity API — web search with citations.
    Perplexity,
    /// Documentation search (Rust docs, MDN, Python docs, etc.).
    Docs,
    /// Context7 — real-time library documentation (API-backed, cached).
    Context7,
}

impl ResearchSource {
    /// Serialize to the string EVA expects in the `source` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::Perplexity => "perplexity",
            Self::Docs => "docs",
            Self::Context7 => "context7",
        }
    }
}

/// Action for the `bible` tool.
///
/// Selects between keyword search and contextual reflection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BibleAction {
    /// KJV keyword search or verse lookup.
    Search,
    /// Scripture recommendations based on emotional or recovery context.
    Reflect,
}

impl BibleAction {
    /// Serialize to the string EVA expects in the `action` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Reflect => "reflect",
        }
    }
}

/// Action for the `secure` tool.
///
/// Selects the type of security analysis to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecureAction {
    /// Vulnerability scanning of source code.
    Scan,
    /// Secrets detection — find hardcoded credentials or API keys.
    Secrets,
}

impl SecureAction {
    /// Serialize to the string EVA expects in the `action` field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::Secrets => "secrets",
        }
    }
}

/// Teaching mode for the `teach` tool.
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

/// Skill level for the `teach` tool.
///
/// Calibrates how much background knowledge EVA assumes the learner has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillLevel {
    /// Beginner — assumes no prior knowledge.
    Beginner,
    /// Intermediate — assumes basic familiarity with the domain.
    Intermediate,
    /// Advanced — assumes strong domain knowledge.
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

// ── Response types ─────────────────────────────────────────────────────────

/// Generic wrapper returned by all text-generating EVA tools.
///
/// The `output` field contains the raw JSON-serialised response from EVA.
/// Callers may parse it for structured access or display it as-is.
///
/// Used by: `ideate`, `memory`, `build`, `research`, `bible`, `secure`,
/// `teach`, and the generic [`crate::EvaClient::action`] adapter.
#[derive(Debug, Clone)]
pub struct ActionOutput {
    /// The full text response from the EVA tool (JSON-formatted by EVA).
    pub output: String,
}

/// Output from the `visualize` tool.
///
/// EVA's `visualize` tool returns a text description of what was generated
/// and, for image requests, the base64-encoded PNG of the image.
#[derive(Debug, Clone)]
pub struct VisualizeOutput {
    /// Human-readable description of what was generated.
    pub text: String,
    /// Base64-encoded PNG data, present only when an image was generated.
    pub image_base64: Option<String>,
}
