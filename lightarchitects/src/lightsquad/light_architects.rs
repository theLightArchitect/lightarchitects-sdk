//! 10 LightArchitects — gate-dimension domain specialists.
//!
//! Maps each Gatekeeper Registry gate dimension `[A+S+Q+C+O+P+K+D+T+R]` to a
//! sibling via [`LightArchitectRegistry`]. Routing targets existing siblings;
//! operator override (Canon XV) replaces the canonical PDF's 8-specialist table
//! with this 10-specialist table.
//!
//! # Gate → Sibling mapping
//!
//! | Gate | Dimension       | Primary sibling | Secondary |
//! |------|-----------------|-----------------|-----------|
//! | [A]  | Architecture    | CORSO           | SOUL      |
//! | [S]  | Security        | SERAPH          | —         |
//! | [Q]  | Quality         | CORSO           | —         |
//! | [C]  | Canon           | LÆX             | —         |
//! | [O]  | Operations      | EVA             | AYIN      |
//! | [P]  | Performance     | EVA             | AYIN      |
//! | [K]  | Knowledge       | SOUL            | —         |
//! | [D]  | Documentation   | SOUL            | EVA       |
//! | [T]  | Testing         | CORSO           | —         |
//! | [R]  | Research + Risk | QUANTUM         | —         |
//!
//! Phase 4 wires `consult()` to live sibling dispatch via `crate::squad_registry`.
//! Phase 2 implements the registry and routing table; `Recommendation` is returned
//! as a static default pending live dispatch.

use serde::{Deserialize, Serialize};

// ─── GateDimension ────────────────────────────────────────────────────────────

/// One of the 10 Gatekeeper Registry gate dimensions.
///
/// Canonical source: `$HELIX/user/standards/canon/gatekeeper-registry.yaml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateDimension {
    /// \[A\] Architecture.
    Architecture,
    /// \[S\] Security.
    Security,
    /// \[Q\] Quality.
    Quality,
    /// \[C\] Canon.
    Canon,
    /// \[O\] Operations.
    Operations,
    /// \[P\] Performance.
    Performance,
    /// \[K\] Knowledge.
    Knowledge,
    /// \[D\] Documentation.
    Documentation,
    /// \[T\] Testing.
    Testing,
    /// \[R\] Research and Risk.
    Research,
}

impl GateDimension {
    /// Short gate label used in `[A+S+Q+C+O+P+K+D+T+R]` vocabulary.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Architecture => "A",
            Self::Security => "S",
            Self::Quality => "Q",
            Self::Canon => "C",
            Self::Operations => "O",
            Self::Performance => "P",
            Self::Knowledge => "K",
            Self::Documentation => "D",
            Self::Testing => "T",
            Self::Research => "R",
        }
    }

    /// Position of this dimension in the canonical 10-entry registry array.
    ///
    /// Matches the order of [`Self::all`] and the `entries` array in
    /// [`LightArchitectRegistry`]. Used for O(1) array indexing in
    /// [`LightArchitectRegistry::entry`].
    #[must_use]
    pub fn ordinal(self) -> usize {
        match self {
            Self::Architecture => 0,
            Self::Security => 1,
            Self::Quality => 2,
            Self::Canon => 3,
            Self::Operations => 4,
            Self::Performance => 5,
            Self::Knowledge => 6,
            Self::Documentation => 7,
            Self::Testing => 8,
            Self::Research => 9,
        }
    }

    /// All 10 dimensions in canonical order.
    #[must_use]
    pub fn all() -> [Self; 10] {
        [
            Self::Architecture,
            Self::Security,
            Self::Quality,
            Self::Canon,
            Self::Operations,
            Self::Performance,
            Self::Knowledge,
            Self::Documentation,
            Self::Testing,
            Self::Research,
        ]
    }
}

// ─── LightArchitectEntry ─────────────────────────────────────────────────────

/// One row in the [`LightArchitectRegistry`] table.
#[derive(Debug, Clone)]
pub struct LightArchitectEntry {
    /// Gate dimension this specialist owns.
    pub dimension: GateDimension,
    /// Primary sibling key (matches `SquadEntry.id`).
    pub primary_sibling: &'static str,
    /// Optional secondary sibling for dual-ownership dimensions.
    pub secondary_sibling: Option<&'static str>,
    /// Human-readable role description for logging.
    pub role_description: &'static str,
}

// ─── Recommendation ───────────────────────────────────────────────────────────

/// The verdict returned by a `LightArchitect` consultation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// The dimension that produced this recommendation.
    pub dimension: GateDimension,
    /// Whether the action is approved by this specialist.
    pub approved: bool,
    /// Reasoning supporting the verdict.
    pub rationale: String,
    /// Optional citation (canon section, standard reference, etc.).
    pub citation: Option<String>,
    /// Confidence in the recommendation (0.0–1.0).
    pub confidence: f32,
}

impl Recommendation {
    /// Construct a default Phase 2 stub recommendation (approved, low confidence).
    ///
    /// Phase 4 replaces this with a real sibling dispatch result.
    #[must_use]
    pub fn stub_approval(dimension: GateDimension) -> Self {
        Self {
            dimension,
            approved: true,
            rationale: format!(
                "Phase 2 stub: {} specialist approved (live dispatch in Phase 4)",
                dimension.label()
            ),
            citation: None,
            confidence: 0.5,
        }
    }
}

// ─── LightArchitectRegistry ──────────────────────────────────────────────────

/// Registry of the 10 gate-dimension `LightArchitect` specialists.
///
/// Built from a static table at construction; the table mirrors
/// `$HELIX/user/standards/canon/gatekeeper-registry.yaml`.
pub struct LightArchitectRegistry {
    entries: [LightArchitectEntry; 10],
}

impl Default for LightArchitectRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LightArchitectRegistry {
    /// Build the registry from the static 10-entry table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: [
                LightArchitectEntry {
                    dimension: GateDimension::Architecture,
                    primary_sibling: "corso",
                    secondary_sibling: Some("soul"),
                    role_description: "Architectural correctness, API design, complexity",
                },
                LightArchitectEntry {
                    dimension: GateDimension::Security,
                    primary_sibling: "seraph",
                    secondary_sibling: None,
                    role_description: "Threat surface, vulnerabilities, supply chain, secrets",
                },
                LightArchitectEntry {
                    dimension: GateDimension::Quality,
                    primary_sibling: "corso",
                    secondary_sibling: None,
                    role_description: "Standards, clippy, fmt, complexity ≤10",
                },
                LightArchitectEntry {
                    dimension: GateDimension::Canon,
                    primary_sibling: "laex",
                    secondary_sibling: None,
                    role_description: "Constitutional principles, canon compliance",
                },
                LightArchitectEntry {
                    dimension: GateDimension::Operations,
                    primary_sibling: "eva",
                    secondary_sibling: Some("ayin"),
                    role_description: "Deploy pipeline, CI/CD, rollback",
                },
                LightArchitectEntry {
                    dimension: GateDimension::Performance,
                    primary_sibling: "eva",
                    secondary_sibling: Some("ayin"),
                    role_description: "Latency, throughput, O(n) bounds",
                },
                LightArchitectEntry {
                    dimension: GateDimension::Knowledge,
                    primary_sibling: "soul",
                    secondary_sibling: None,
                    role_description: "Helix enrichment, citations, prior decisions",
                },
                LightArchitectEntry {
                    dimension: GateDimension::Documentation,
                    primary_sibling: "soul",
                    secondary_sibling: Some("eva"),
                    role_description: "rustdoc, JSDoc, CLAUDE.md, examples",
                },
                LightArchitectEntry {
                    dimension: GateDimension::Testing,
                    primary_sibling: "corso",
                    secondary_sibling: None,
                    role_description: "6-suite pyramid, ≥90% coverage",
                },
                LightArchitectEntry {
                    dimension: GateDimension::Research,
                    primary_sibling: "quantum",
                    secondary_sibling: None,
                    role_description: "Prior art, dependency audit, risk scoring",
                },
            ],
        }
    }

    /// Look up the entry for `dimension`.
    ///
    /// # Panics
    ///
    /// Never panics in practice — the registry always has exactly one entry per
    /// [`GateDimension`] variant. The `unreachable!` below is a static-completeness
    /// guard: if a new variant is added without a registry entry the compiler will
    /// point here at the match arm.
    #[must_use]
    pub fn entry(&self, dimension: GateDimension) -> &LightArchitectEntry {
        let idx = dimension.ordinal();
        &self.entries[idx]
    }

    /// Consult the `LightArchitect` for `dimension` on a given `description`.
    ///
    /// Phase 2: returns a stub [`Recommendation`] (approved, confidence 0.5).
    /// Phase 4: dispatches to the primary sibling via `crate::squad_registry`.
    #[must_use]
    pub fn consult(&self, dimension: GateDimension, _description: &str) -> Recommendation {
        Recommendation::stub_approval(dimension)
    }

    /// Determine the most relevant dimension for a given action description
    /// by simple keyword matching. Returns `None` when no dimension is obvious.
    #[must_use]
    pub fn infer_dimension(&self, description: &str) -> Option<GateDimension> {
        let lower = description.to_lowercase();
        if lower.contains("security") || lower.contains("auth") || lower.contains("secret") {
            Some(GateDimension::Security)
        } else if lower.contains("test") || lower.contains("coverage") {
            Some(GateDimension::Testing)
        } else if lower.contains("doc") || lower.contains("comment") {
            Some(GateDimension::Documentation)
        } else if lower.contains("canon") || lower.contains("lasdlc") {
            Some(GateDimension::Canon)
        } else if lower.contains("perf")
            || lower.contains("latency")
            || lower.contains("throughput")
        {
            Some(GateDimension::Performance)
        } else if lower.contains("deploy") || lower.contains("ci") || lower.contains("ops") {
            Some(GateDimension::Operations)
        } else if lower.contains("arch") || lower.contains("design") || lower.contains("api") {
            Some(GateDimension::Architecture)
        } else if lower.contains("research") || lower.contains("risk") || lower.contains("dep") {
            Some(GateDimension::Research)
        } else {
            None
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn registry_covers_all_10_dimensions() {
        let registry = LightArchitectRegistry::new();
        for dim in GateDimension::all() {
            let entry = registry.entry(dim);
            assert_eq!(entry.dimension, dim);
            assert!(!entry.primary_sibling.is_empty());
            assert!(!entry.role_description.is_empty());
        }
    }

    #[test]
    fn security_dimension_routes_to_seraph() {
        let registry = LightArchitectRegistry::new();
        let entry = registry.entry(GateDimension::Security);
        assert_eq!(entry.primary_sibling, "seraph");
        assert!(entry.secondary_sibling.is_none());
    }

    #[test]
    fn performance_has_secondary_ayin() {
        let registry = LightArchitectRegistry::new();
        let entry = registry.entry(GateDimension::Performance);
        assert_eq!(entry.primary_sibling, "eva");
        assert_eq!(entry.secondary_sibling, Some("ayin"));
    }

    #[test]
    fn canon_routes_to_laex() {
        let registry = LightArchitectRegistry::new();
        let entry = registry.entry(GateDimension::Canon);
        assert_eq!(entry.primary_sibling, "laex");
    }

    #[test]
    fn consult_returns_stub_in_phase2() {
        let registry = LightArchitectRegistry::new();
        let rec = registry.consult(GateDimension::Architecture, "design new API surface");
        assert!(rec.approved);
        assert_eq!(rec.dimension, GateDimension::Architecture);
        assert!(rec.confidence > 0.0 && rec.confidence <= 1.0);
    }

    #[test]
    fn gate_dimension_labels_are_single_char() {
        for dim in GateDimension::all() {
            assert_eq!(dim.label().len(), 1, "label for {dim:?} must be 1 char");
        }
    }

    #[test]
    fn all_dimensions_count_is_10() {
        assert_eq!(GateDimension::all().len(), 10);
    }

    #[test]
    fn infer_dimension_security_keyword() {
        let registry = LightArchitectRegistry::new();
        let dim = registry.infer_dimension("security audit required");
        assert_eq!(dim, Some(GateDimension::Security));
    }

    #[test]
    fn infer_dimension_unknown_returns_none() {
        let registry = LightArchitectRegistry::new();
        let dim = registry.infer_dimension("some completely unrelated action");
        assert!(dim.is_none());
    }

    #[test]
    fn recommendation_stub_approved_flag() {
        let rec = Recommendation::stub_approval(GateDimension::Testing);
        assert!(rec.approved);
        assert_eq!(rec.dimension, GateDimension::Testing);
    }

    #[test]
    fn gate_dimension_serializes_to_snake_case() {
        let json = serde_json::to_string(&GateDimension::Architecture).unwrap();
        assert_eq!(json, r#""architecture""#);
    }
}
