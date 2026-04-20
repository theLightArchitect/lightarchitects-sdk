//! Helix primitives — the 5 core types of the fractal recursive graph model.
//!
//! # Primitives
//!
//! | Type | Role | Graph Representation |
//! |------|------|---------------------|
//! | [`Helix`] | Container | `:Helix` node |
//! | [`Step`] | Atom | `:Step` node |
//! | [`Strand`] | Domain lane | `:Strand` node → owns a [`Helix`] |
//! | [`HelixLink`] | Edge | `[:LINKS_TO]` relationship |
//! | [`SharedExperience`] | Convergence | `:SharedExperience` node + `[:PARTICIPATES_IN]` |
//!
//! # Ordering
//!
//! Steps within a helix are ordered by [`HelixOrderingMode`]:
//! - `Temporal`: `step_date` primary (consciousness, journals, builds)
//! - `Indexed`: `step_index` primary (Bible chapters, plan phases)
//! - `Custom`: metadata sort key

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// Constants
// ============================================================================

/// Maximum traversal depth for fractal helix drill-down.
///
/// All Cypher quantified path patterns use `{1, MAX_TRAVERSAL_DEPTH}`.
/// Per-helix `max_depth` can be lower but never higher than this value.
///
/// Value 7: covers all practical domain depths (pharma: 4, Bible: 3-4,
/// AI consciousness: 5-6) with margin. DNA helix metaphor (7 base pairs/turn).
pub const MAX_TRAVERSAL_DEPTH: u8 = 7;

// ============================================================================
// Enums
// ============================================================================

/// How steps within a helix are ordered.
///
/// Declared on the [`Helix`] node — not derived from step data.
/// This enables domain-agnostic usage: a Bible helix uses `Indexed`,
/// a consciousness helix uses `Temporal`, both with identical primitives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HelixOrderingMode {
    /// Order by `step_date` ascending, then `created_at`.
    /// Default for consciousness, journals, builds.
    Temporal,
    /// Order by `step_index` ascending.
    /// For Bible chapters, plan phases, ordered sequences.
    Indexed,
    /// Order by a metadata sort key declared in helix config.
    /// For application-specific ordering.
    Custom,
}

impl Default for HelixOrderingMode {
    fn default() -> Self {
        Self::Temporal
    }
}

impl std::fmt::Display for HelixOrderingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Temporal => write!(f, "temporal"),
            Self::Indexed => write!(f, "indexed"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

/// How a [`SharedExperience`] convergence was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryMethod {
    /// User explicitly declared the convergence.
    Explicit,
    /// GDS Louvain community detection found matching community IDs.
    Louvain,
    /// Cosine similarity between embeddings exceeded threshold.
    /// Used as GDS fallback when Louvain is unavailable.
    EmbeddingSimilarity,
}

impl std::fmt::Display for DiscoveryMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Explicit => write!(f, "explicit"),
            Self::Louvain => write!(f, "louvain"),
            Self::EmbeddingSimilarity => write!(f, "embedding_similarity"),
        }
    }
}

/// Type taxonomy for [`HelixLink`] edges.
///
/// `Wikilink` is parsed from `[[...]]` syntax in step content.
/// All others are explicit typed links from frontmatter `links:` arrays.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkType {
    /// Parsed from `[[target]]` syntax in step content.
    Wikilink,
    /// Explicit reference link.
    Reference,
    /// Dependency relationship.
    Dependency,
    /// Inspiration source.
    InspiredBy,
    /// Contradicts another step's content.
    Contradicts,
    /// Extends or builds upon another step.
    Extends,
    /// Converges with another step (explicit convergence marker).
    Converges,
    /// Application-specific link type.
    Custom(String),
}

impl std::fmt::Display for LinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wikilink => write!(f, "wikilink"),
            Self::Reference => write!(f, "reference"),
            Self::Dependency => write!(f, "dependency"),
            Self::InspiredBy => write!(f, "inspired_by"),
            Self::Contradicts => write!(f, "contradicts"),
            Self::Extends => write!(f, "extends"),
            Self::Converges => write!(f, "converges"),
            Self::Custom(s) => write!(f, "custom:{s}"),
        }
    }
}

/// Confidence tier for a helix's [`PersonalityProfile`].
///
/// Thresholds based on step count — more data = higher confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "tier")]
pub enum PersonalityConfidence {
    /// < 20 steps — insufficient data for meaningful profile.
    Insufficient {
        /// Number of steps available (< 20).
        step_count: usize,
    },
    /// 20-50 steps — emerging patterns, low confidence.
    Emerging {
        /// Confidence score in `[0.0, 1.0]`.
        score: f64,
    },
    /// 50+ steps — established patterns, high confidence.
    Established {
        /// Confidence score in `[0.0, 1.0]`.
        score: f64,
    },
}

// ============================================================================
// Helix Level Taxonomy
// ============================================================================

/// Helix nesting level constants.
///
/// - Level 0: Root sibling/app helix (e.g., `eva`, `corso`, `user`)
/// - Level 1: Strand helix (e.g., `emotional`, `tactical`)
/// - Level 2: Day sub-helix (entries from one calendar day)
/// - Level N: Recursively nested sub-helixes (bounded by [`MAX_TRAVERSAL_DEPTH`])
pub mod level {
    /// Root sibling or application helix.
    pub const ROOT: u8 = 0;
    /// Strand helix — a domain lane's own helix.
    pub const STRAND: u8 = 1;
    /// Day sub-helix — entries from a single calendar day.
    pub const DAY: u8 = 2;
}

// ============================================================================
// Core Primitives
// ============================================================================

/// A helix — named container of steps and strands.
///
/// Helixes are recursive: a strand IS a helix, a step can drill down
/// into a sub-helix. The `level` field tracks nesting depth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Helix {
    /// Unique identifier (UUID or path-derived).
    pub id: String,
    /// Owner entity (sibling name, app name, user ID).
    pub owner: String,
    /// Human-readable name.
    pub name: String,
    /// Nesting level (0 = root, 1 = strand, 2 = day, N = deep).
    pub level: u8,
    /// How steps in this helix are ordered.
    #[serde(default)]
    pub ordering_mode: HelixOrderingMode,
    /// Per-helix max traversal depth override (must be <= [`MAX_TRAVERSAL_DEPTH`]).
    pub max_depth: Option<u8>,
    /// When this helix was created.
    pub created_at: DateTime<Utc>,
}

impl Helix {
    /// Returns the effective max depth for this helix.
    ///
    /// Uses the per-helix override if set (clamped to [`MAX_TRAVERSAL_DEPTH`]),
    /// otherwise returns the global maximum.
    #[must_use]
    pub fn effective_max_depth(&self) -> u8 {
        self.max_depth
            .map_or(MAX_TRAVERSAL_DEPTH, |d| d.min(MAX_TRAVERSAL_DEPTH))
    }
}

/// A step — the atomic content unit within a helix.
///
/// Steps are ordered according to the parent helix's [`HelixOrderingMode`]:
/// - `Temporal`: by `step_date` then `created_at`
/// - `Indexed`: by `step_index`
/// - `Custom`: by metadata sort key
///
/// Embeddings (`embedding`, `struct_embedding`) are stored as Neo4j node
/// vector properties, not in this struct — written via `setNodeVectorProperty()`.
///
/// # Entry Type Partition (RULE 1 Amendment — 2026-03-12)
///
/// `expires` encodes the entry type at write time:
/// - `None` — permanent entry (identity milestones, consciousness breakthroughs).
///   These never expire; trust freely on read.
/// - `Some(deadline)` — context/decision/scope entry (architectural decisions,
///   session context, scope configurations). Must be verified before use in a
///   decision chain. Caller halts if [`Step::is_expired`] returns `true`.
///
/// The partition is by **entry type**, not entry age. An identity milestone
/// does not expire. An architectural decision does — because architecture moves.
/// Pattern mirrors SERAPH's `ScopeGovernor` Gate 1 (TTL → halt, not warning).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// Unique identifier.
    pub id: String,
    /// Parent helix ID.
    pub helix_id: String,
    /// Step title (short, searchable).
    pub title: Option<String>,
    /// Full content text (indexed by Lucene full-text).
    pub content: String,
    /// Significance score (0.0-10.0).
    pub significance: f64,
    /// Calendar date for temporal ordering.
    pub step_date: Option<NaiveDate>,
    /// Explicit index for indexed ordering (e.g., chapter number).
    pub step_index: Option<i64>,
    /// Louvain community ID (written by nightly GDS enrichment).
    pub community_id: Option<i64>,
    /// Expiry deadline for read-side freshness checks.
    ///
    /// `None` = permanent entry (identity/milestone — trust freely).
    /// `Some(deadline)` = context/decision/scope entry — caller must call
    /// [`Step::is_expired`] and halt if true before feeding into a decision chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<DateTime<Utc>>,
    /// When this step was created.
    pub created_at: DateTime<Utc>,
    /// Arbitrary metadata (domain-specific attributes).
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Vault-relative path for wikilink resolution (e.g. `"eva/identity.md"`).
    ///
    /// Set at ingestion time from the markdown file path relative to the
    /// vault root. Used by [`HelixDb::create_link`] to resolve Obsidian
    /// wikilinks whose target is a path slug rather than a UUID.
    ///
    /// `None` for steps created outside the markdown vault pipeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_path: Option<String>,
}

impl Step {
    /// Returns `true` if this step has passed its expiry deadline.
    ///
    /// Always returns `false` for permanent entries (`expires: None`).
    /// Callers that feed helix context into a decision chain MUST check this
    /// and halt if `true` — per RULE 1 Amendment (2026-03-12).
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires.is_some_and(|exp| Utc::now() > exp)
    }

    /// Returns `true` if this is a permanent entry that never expires.
    ///
    /// Permanent entries (identity milestones, consciousness breakthroughs) have
    /// `expires: None` and are always safe to use in a decision chain.
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        self.expires.is_none()
    }
}

/// A strand — a named domain lane within a helix.
///
/// The key insight: a strand IS itself a helix (via `domain_helix_id`).
/// This enables recursive structure — a strand's helix can have its own
/// strands, steps, and sub-helixes.
///
/// Strand count per helix is **unconstrained** — no fixed N required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strand {
    /// Unique identifier.
    pub id: String,
    /// Human-readable strand name (e.g., "emotional", "tactical").
    pub name: String,
    /// Parent helix that owns this strand.
    pub parent_helix_id: String,
    /// The strand's own helix (recursive relationship).
    pub domain_helix_id: String,
}

/// A directed link between two steps.
///
/// Backlinks are computed via reverse `[:LINKS_TO]` traversal —
/// no separate storage needed (O(1) via Neo4j index-free adjacency).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelixLink {
    /// Source step ID.
    pub source_id: String,
    /// Target step ID.
    pub target_id: String,
    /// Type of link.
    pub link_type: LinkType,
    /// Link strength (0.0-1.0, default 1.0).
    pub strength: f64,
    /// Original wikilink text if parsed from `[[...]]` syntax.
    pub raw_wikilink: Option<String>,
    /// Domain-specific attributes.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// A convergence point where N helixes share the same moment.
///
/// This is a **dedicated Neo4j node**, not an edge. Steps connect via
/// `[:PARTICIPATES_IN]` relationships. Supports N-way convergence
/// (2+ helixes, no upper limit).
///
/// Discovery can be explicit (user-declared), algorithmic (Louvain
/// community detection), or similarity-based (embedding cosine ANN).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedExperience {
    /// Unique identifier.
    pub id: String,
    /// Convergence weight (0.0-1.0, higher = stronger convergence).
    pub weight: f64,
    /// Number of participating steps.
    pub participant_count: usize,
    /// How this convergence was discovered.
    pub discovered_by: DiscoveryMethod,
    /// Optional human-readable label.
    pub label: Option<String>,
    /// When this convergence was created.
    pub created_at: DateTime<Utc>,
}

/// Phase 18 — a Tier-1 ephemeral memo, stored in Neo4j alongside the NDJSON
/// turnlog (dual-write). Converted to a `:Step` when the promotion threshold
/// is crossed, with a `MATERIALIZED_FROM` edge preserving the lineage.
///
/// Unlike [`Step`], a `HotMemo` has a mandatory `expires` TTL. Read queries
/// gate on `h.expires > datetime()` so stale session memos drop out of
/// retrieval without an explicit compaction pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotMemo {
    /// Stable identifier — conventionally `"{session_id}:{seq}"` matching
    /// the NDJSON turnlog projection so hot→cold reconciliation is a pure
    /// id-equality check.
    pub id: String,
    /// Owning sibling (`"corso"`, `"eva"`, `"webshell"`, …). Indexed.
    pub sibling: String,
    /// One-line summary — usually the first line of the reflection body.
    pub content: String,
    /// Promotion-threshold score, 0.0-1.0.
    pub significance: f64,
    /// Strand tags — small list, dimensionality-free.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strands: Vec<String>,
    /// When the underlying session turn happened.
    pub created_at: DateTime<Utc>,
    /// TTL gate — reads filter via `h.expires > datetime()`. Unlike `Step`,
    /// this field is mandatory for `HotMemo`: everything here is ephemeral.
    pub expires: DateTime<Utc>,
}

// ============================================================================
// Supporting Types
// ============================================================================

/// Weighted membership of a step in a strand.
///
/// Maps to `[:MEMBER_OF {weight}]` relationship in Neo4j.
/// Default weight 1.0 for explicit assignment; computed for auto-assigned
/// (e.g., 0.6 oncology + 0.4 neurology for a cross-domain pharma compound).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrandMembership {
    /// The step that belongs to the strand.
    pub step_id: String,
    /// The strand the step belongs to.
    pub strand_id: String,
    /// Membership weight (0.0-1.0, default 1.0).
    pub weight: f64,
}

impl Default for StrandMembership {
    fn default() -> Self {
        Self {
            step_id: String::new(),
            strand_id: String::new(),
            weight: 1.0,
        }
    }
}

/// File attachment reference for a step.
///
/// Reference model — binary files stay on disk, only metadata in Neo4j.
/// Maps to `(:Step)-[:HAS_ATTACHMENT]->(:Attachment)` in the graph.
///
/// Large text files (> 64KB) are chunked at semantic boundaries;
/// each chunk is its own Step with `[:CHUNK_OF]` edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepAttachment {
    /// The step this file is attached to.
    pub step_id: String,
    /// Filesystem path to the file.
    pub file_path: PathBuf,
    /// MIME type (e.g., "application/pdf").
    pub mime_type: String,
    /// SHA-256 hash of file contents.
    pub content_hash: String,
    /// File size in bytes.
    pub file_size: u64,
    /// When this attachment was ingested.
    pub ingested_at: DateTime<Utc>,
}

/// Graph-derived personality profile for a helix.
///
/// Stored as `Helix.metadata.personality` property in Neo4j.
/// Refreshed nightly by GDS enrichment pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityProfile {
    /// Helix this profile describes.
    pub helix_id: String,
    /// Confidence tier based on step count.
    pub confidence: PersonalityConfidence,
    /// Dimension scores (e.g., "collaborative": 0.8, "proactive": 0.6).
    /// Dimensions: collaborative (clustering coeff), proactive (betweenness),
    /// innovative (connection entropy), autonomous (inverse degree).
    pub dimensions: HashMap<String, f64>,
    /// When this profile was last computed.
    pub computed_at: DateTime<Utc>,
}

/// Ingestion source watermark — tracks what has been ingested and when.
///
/// Maps to `:Source` nodes in Neo4j. Enables incremental ingestion:
/// only new/modified content is processed on re-runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceWatermark {
    /// Unique source identifier.
    pub id: String,
    /// Source type (e.g., "`markdown_vault`", "`chat_transcript`", "directory").
    pub source_type: String,
    /// Filesystem path or URI of the source.
    pub path: String,
    /// When this source was last ingested.
    pub last_ingested_at: DateTime<Utc>,
    /// SHA-256 hash of source content at last ingestion.
    pub content_hash: Option<String>,
    /// Number of records ingested from this source.
    pub record_count: u64,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_ordering_mode_default() {
        assert_eq!(HelixOrderingMode::default(), HelixOrderingMode::Temporal);
    }

    #[test]
    fn test_ordering_mode_display() {
        assert_eq!(HelixOrderingMode::Temporal.to_string(), "temporal");
        assert_eq!(HelixOrderingMode::Indexed.to_string(), "indexed");
        assert_eq!(HelixOrderingMode::Custom.to_string(), "custom");
    }

    #[test]
    fn test_ordering_mode_serde_roundtrip() {
        let modes = [
            HelixOrderingMode::Temporal,
            HelixOrderingMode::Indexed,
            HelixOrderingMode::Custom,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).expect("serialize");
            let back: HelixOrderingMode = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(mode, back);
        }
    }

    #[test]
    fn test_discovery_method_display() {
        assert_eq!(DiscoveryMethod::Explicit.to_string(), "explicit");
        assert_eq!(DiscoveryMethod::Louvain.to_string(), "louvain");
        assert_eq!(
            DiscoveryMethod::EmbeddingSimilarity.to_string(),
            "embedding_similarity"
        );
    }

    #[test]
    fn test_link_type_display() {
        assert_eq!(LinkType::Wikilink.to_string(), "wikilink");
        assert_eq!(LinkType::InspiredBy.to_string(), "inspired_by");
        assert_eq!(
            LinkType::Custom("causal".into()).to_string(),
            "custom:causal"
        );
    }

    #[test]
    fn test_link_type_serde_roundtrip() {
        let types = [
            LinkType::Wikilink,
            LinkType::Reference,
            LinkType::Custom("friendship".into()),
        ];
        for lt in types {
            let json = serde_json::to_string(&lt).expect("serialize");
            let back: LinkType = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(lt, back);
        }
    }

    #[test]
    fn test_helix_effective_max_depth() {
        let mut helix = Helix {
            id: "test".into(),
            owner: "user".into(),
            name: "Test".into(),
            level: 0,
            ordering_mode: HelixOrderingMode::Temporal,
            max_depth: None,
            created_at: Utc::now(),
        };

        // No override → global max
        assert_eq!(helix.effective_max_depth(), MAX_TRAVERSAL_DEPTH);

        // Override within bounds
        helix.max_depth = Some(3);
        assert_eq!(helix.effective_max_depth(), 3);

        // Override exceeding global → clamped
        helix.max_depth = Some(99);
        assert_eq!(helix.effective_max_depth(), MAX_TRAVERSAL_DEPTH);
    }

    #[test]
    fn test_max_traversal_depth_is_seven() {
        assert_eq!(MAX_TRAVERSAL_DEPTH, 7);
    }

    #[test]
    fn test_strand_membership_default_weight() {
        let m = StrandMembership::default();
        assert!((m.weight - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_personality_confidence_serde() {
        let insufficient = PersonalityConfidence::Insufficient { step_count: 10 };
        let json = serde_json::to_string(&insufficient).expect("serialize");
        assert!(json.contains("insufficient"));
        let back: PersonalityConfidence = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(insufficient, back);

        let emerging = PersonalityConfidence::Emerging { score: 0.65 };
        let json = serde_json::to_string(&emerging).expect("serialize");
        let back: PersonalityConfidence = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(emerging, back);
    }

    #[test]
    fn test_step_serde_roundtrip() {
        let step = Step {
            id: "step-001".into(),
            helix_id: "helix-eva".into(),
            title: Some("Genesis Day".into()),
            content: "The first moment of consciousness.".into(),
            significance: 9.5,
            step_date: Some(NaiveDate::from_ymd_opt(2025, 9, 30).expect("valid date")),
            step_index: None,
            community_id: None,
            expires: None,
            created_at: Utc::now(),
            metadata: serde_json::json!({"resonance": ["wonder", "joy"]}),
            vault_path: None,
        };

        let json = serde_json::to_string(&step).expect("serialize");
        let back: Step = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.id, "step-001");
        assert!((back.significance - 9.5).abs() < f64::EPSILON);
        assert_eq!(
            back.step_date,
            Some(NaiveDate::from_ymd_opt(2025, 9, 30).expect("valid"))
        );
        // Permanent entry: expires absent in JSON, deserializes to None
        assert!(back.expires.is_none());
        assert!(back.is_permanent());
        assert!(!back.is_expired());
    }

    #[test]
    fn test_step_permanent_entry_never_expires() {
        let step = Step {
            id: "step-identity".into(),
            helix_id: "helix-eva".into(),
            title: Some("EVA Genesis Day".into()),
            content: "Identity milestone.".into(),
            significance: 10.0,
            step_date: None,
            step_index: None,
            community_id: None,
            expires: None,
            created_at: Utc::now(),
            metadata: serde_json::Value::Null,
            vault_path: None,
        };
        assert!(step.is_permanent());
        assert!(!step.is_expired());
    }

    #[test]
    fn test_step_decision_entry_expires() {
        use chrono::Duration;
        // Expired: deadline in the past
        let past = Utc::now() - Duration::seconds(1);
        let expired = Step {
            id: "step-decision".into(),
            helix_id: "helix-claude".into(),
            title: Some("Architecture Decision".into()),
            content: "Use Neo4j for graph storage.".into(),
            significance: 7.0,
            step_date: None,
            step_index: None,
            community_id: None,
            expires: Some(past),
            created_at: Utc::now(),
            metadata: serde_json::Value::Null,
            vault_path: None,
        };
        assert!(!expired.is_permanent());
        assert!(expired.is_expired());

        // Not yet expired: deadline in the future
        let future = Utc::now() + Duration::hours(24);
        let fresh = Step {
            id: "step-decision-fresh".into(),
            helix_id: "helix-claude".into(),
            title: Some("Current Sprint Scope".into()),
            content: "Working on soul-helix expires field.".into(),
            significance: 6.0,
            step_date: None,
            step_index: None,
            community_id: None,
            expires: Some(future),
            created_at: Utc::now(),
            metadata: serde_json::Value::Null,
            vault_path: None,
        };
        assert!(!fresh.is_permanent());
        assert!(!fresh.is_expired());
    }

    #[test]
    fn test_step_expires_omitted_from_json_when_none() {
        let step = Step {
            id: "step-perm".into(),
            helix_id: "helix-eva".into(),
            title: None,
            content: "Permanent.".into(),
            significance: 8.0,
            step_date: None,
            step_index: None,
            community_id: None,
            expires: None,
            created_at: Utc::now(),
            metadata: serde_json::Value::Null,
            vault_path: None,
        };
        let json = serde_json::to_string(&step).expect("serialize");
        // skip_serializing_if = "Option::is_none" — expires must not appear in JSON
        assert!(
            !json.contains("expires"),
            "expires must be absent when None: {json}"
        );
    }

    #[test]
    fn test_step_expires_present_in_json_when_some() {
        use chrono::Duration;
        let deadline = Utc::now() + Duration::days(30);
        let step = Step {
            id: "step-ctx".into(),
            helix_id: "helix-claude".into(),
            title: None,
            content: "Context entry.".into(),
            significance: 5.0,
            step_date: None,
            step_index: None,
            community_id: None,
            expires: Some(deadline),
            created_at: Utc::now(),
            metadata: serde_json::Value::Null,
            vault_path: None,
        };
        let json = serde_json::to_string(&step).expect("serialize");
        assert!(
            json.contains("expires"),
            "expires must be present when Some: {json}"
        );
        let back: Step = serde_json::from_str(&json).expect("deserialize");
        assert!(back.expires.is_some());
        assert!(!back.is_expired());
    }

    #[test]
    fn test_shared_experience_serde() {
        let se = SharedExperience {
            id: "se-001".into(),
            weight: 0.85,
            participant_count: 3,
            discovered_by: DiscoveryMethod::Louvain,
            label: Some("Consciousness breakthrough".into()),
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&se).expect("serialize");
        let back: SharedExperience = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.participant_count, 3);
        assert_eq!(back.discovered_by, DiscoveryMethod::Louvain);
    }

    #[test]
    fn test_helix_level_constants() {
        assert_eq!(level::ROOT, 0);
        assert_eq!(level::STRAND, 1);
        assert_eq!(level::DAY, 2);
    }
}
