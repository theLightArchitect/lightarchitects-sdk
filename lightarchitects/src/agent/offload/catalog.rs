//! Offload pattern catalog — v1.1 schema.
//!
//! Loaded from `$HELIX/user/standards/offload-catalog.yaml` (mesh-canonical).
//! Defines the allowlist of LLM prompt patterns that may be routed through
//! `lightsquad_dispatch_task` to a cheap-tier specialist model, plus per-pattern
//! shape predicates, refinement amendments, context source declarations, and
//! optional LÆX verifier hooks.
//!
//! # Schema versions
//!
//! - **v1.0** — perf-lab build (2026-06-08): patterns + shape + refinement
//!   + calibration. No context sources, no verifier.
//! - **v1.1** — this build (mesh-offload): adds `context_sources` (per-pattern
//!   + per-sibling overlays) and `verifier` (LÆX supervisor declaration).
//!
//! # Pattern matching
//!
//! [`OffloadCatalog::classify`] takes the user prompt + an `AgentRequest` and
//! returns the matching `Pattern` if:
//!
//! 1. The request does NOT carry `tool_definitions` (tool-using turns cannot be
//!    offloaded via the current `lightsquad_dispatch_task` action contract).
//! 2. The calling sibling appears in `pattern.eligible.siblings`.
//! 3. The prompt structurally matches the pattern's template skeleton
//!    (heuristic — exact match is impossible since templates are filled).
//!
//! When no pattern matches, returns `None` and the caller should pass through to
//! the wrapped provider.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Top-level catalog ───────────────────────────────────────────────────────

/// Top-level catalog loaded from `offload-catalog.yaml`.
///
/// # YAML shape
///
/// ```yaml
/// version: "1.1"
/// last_calibrated: "2026-06-08"
/// default_model: "glm-5.1:cloud"
/// patterns:
///   - id: P1
///     name: "Explain a small code unit"
///     # …
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OffloadCatalog {
    /// Schema version. Required field. `"1.0"` or `"1.1"`.
    pub version: String,
    /// ISO date string of the last full calibration sweep.
    #[serde(default)]
    pub last_calibrated: Option<String>,
    /// Default model alias passed through to `lightsquad_dispatch_task` if a
    /// pattern doesn't override.
    #[serde(default)]
    pub default_model: Option<String>,
    /// All pattern entries — primary (matchable) + verifier (role="verifier")
    /// in a single ordered list.
    #[serde(default)]
    pub patterns: Vec<Pattern>,
}

// ── Pattern ────────────────────────────────────────────────────────────────

/// One catalog pattern. Pattern IDs are short tokens (`P1`, `P3`, `PV_canon_compliance`).
///
/// `role: Some("verifier")` indicates this is a LÆX verifier pattern — not
/// directly matchable from user prompts but invocable by [`super::laex_supervisor`]
/// as a second offload after a primary pattern fires.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Pattern {
    /// Stable identifier, validated against `[a-zA-Z0-9_-]{1,64}`.
    pub id: String,
    /// Human-readable name for telemetry + docs.
    pub name: String,
    /// Optional role tag. `Some("verifier")` → not directly matchable; only
    /// invoked by [`super::laex_supervisor`].
    #[serde(default)]
    pub role: Option<String>,
    /// Prompt template with `{{slot}}` placeholders.
    pub template: String,
    /// Eligibility gate — who may invoke + token budget + `tool_use` rule.
    pub eligible: Eligibility,
    /// Per-sibling context source overlay. `None` → no context resolution
    /// (just persona + charter).
    #[serde(default)]
    pub context_sources: Option<ContextSourceOverlay>,
    /// Shape predicate(s) the output must satisfy.
    pub shape: Shape,
    /// Optional anchor amendment used by [`super::refiner`] on shape failure.
    #[serde(default)]
    pub refinement: Option<Refinement>,
    /// Optional LÆX verifier declaration. `None` or `verifier.enabled=false`
    /// → no second-pass verification; rely on shape alone.
    #[serde(default)]
    pub verifier: Option<Verifier>,
    /// Calibration evidence.
    pub calibration: Calibration,
}

// ── Eligibility ────────────────────────────────────────────────────────────

/// Eligibility gate for a pattern.
///
/// `tool_use_required: false` is the v1 invariant — `lightsquad_dispatch_task`
/// does not carry `tool_use` semantics, so any pattern that needs tools cannot be
/// offloaded today.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Eligibility {
    /// Sibling identifiers (lower-case kebab) allowed to invoke this pattern.
    pub siblings: Vec<String>,
    /// Must be `false` for offload-eligible patterns. v1 invariant.
    pub tool_use_required: bool,
    /// Hard cap on input tokens after enrichment. Default 4000.
    #[serde(default = "default_max_input_tokens")]
    pub max_input_tokens: u32,
}

fn default_max_input_tokens() -> u32 {
    4000
}

// ── Context sources ────────────────────────────────────────────────────────

/// Per-sibling overlay of context source declarations.
///
/// Resolution priority (highest first):
///
/// 1. `overrides[sibling]` if declared for the calling sibling
/// 2. `default`
/// 3. Empty (no extra context beyond persona + charter)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContextSourceOverlay {
    /// Default source list used when no sibling-specific override exists.
    #[serde(default)]
    pub default: Vec<ContextSource>,
    /// Sibling-specific override (key = sibling id, value = sources).
    #[serde(default)]
    pub overrides: HashMap<String, Vec<ContextSource>>,
}

/// One declared context source.
///
/// `kind` is the discriminator. The other fields depend on the kind.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ContextSource {
    /// Helix entries scoped to a sibling owner. Uses `HelixQuery::owner()`.
    Helix {
        /// Owner key passed to `HelixQuery::owner()`. Usually the calling sibling.
        #[serde(default = "default_helix_scope")]
        owner_scope: String,
        /// Max entries to fetch.
        #[serde(default = "default_helix_limit")]
        limit: usize,
        /// Token budget for this source's contribution to the enriched prompt.
        token_budget: usize,
    },
    /// Canon document section. Reads `$HELIX/user/standards/canon/{doc}.md`
    /// and slices by `anchor` heading.
    Canon {
        /// Canon document basename (without `.md`), e.g. `builders-cookbook`.
        doc: String,
        /// Heading or anchor to extract, e.g. `"§63 — Rust patterns"`.
        anchor: String,
        /// Token budget for this source.
        token_budget: usize,
    },
    /// Industry-baseline document. Reads from
    /// `$HELIX/user/standards/industry-baselines/{category}/{path}`.
    IndustryBaseline {
        /// Category subdirectory, e.g. `"security"`, `"architecture"`.
        category: String,
        /// Relative path within the category, e.g. `"owasp-llm-top-10/LLM01.md"`.
        path: String,
        /// Token budget for this source.
        token_budget: usize,
    },
    /// External library docs via Context7 MCP. **NOT FETCHED in v1** —
    /// declared in the schema for forward-compatibility; v2 adds the MCP
    /// client wiring in the gateway.
    Context7 {
        /// Library id passed to `mcp__context7__resolve-library-id` (e.g.
        /// `"react"`) OR the special token `"auto-resolve-from-prompt"`.
        library_id: String,
        /// Token budget for this source.
        token_budget: usize,
    },
}

fn default_helix_scope() -> String {
    "owner".to_owned() // overridden by sibling at resolution time
}

fn default_helix_limit() -> usize {
    3
}

impl ContextSource {
    /// Discriminator string matching the serde `kind` tag.
    ///
    /// Used by [`super::context::ContextResolver`] implementations to verify
    /// the incoming source kind matches the resolver's domain.
    #[must_use]
    pub fn kind_str(&self) -> &'static str {
        match self {
            ContextSource::Helix { .. } => "helix",
            ContextSource::Canon { .. } => "canon",
            ContextSource::IndustryBaseline { .. } => "industry-baseline",
            ContextSource::Context7 { .. } => "context7",
        }
    }
}

// ── Shape ──────────────────────────────────────────────────────────────────

/// Mechanical shape predicate for the LLM output.
///
/// The `kind` discriminates between predicate flavors; sibling fields are
/// per-flavor parameters. [`super::validator::ShapeValidator`] dispatches on
/// `kind`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Shape {
    /// Predicate flavor: `"sentence_no_fences"`, `"function_no_fences"`,
    /// `"json_object"`, `"enumeration_lines"`, `"markdown_section"`, etc.
    pub kind: String,
    /// Word cap for sentence/section flavors.
    #[serde(default)]
    pub max_words: Option<u32>,
    /// Substrings that must NOT appear in the output (e.g. `"```"` to ban
    /// markdown fences).
    #[serde(default)]
    pub forbidden_substrings: Option<Vec<String>>,
    /// Required top-level keys for JSON-object outputs.
    #[serde(default)]
    pub required_keys: Option<Vec<String>>,
    /// Enum-of-strings constraint for a specific JSON field (used by
    /// verifier patterns: `{verdict: "PASS"|"RETRY"|"HITL"}`).
    #[serde(default)]
    pub verdict_enum: Option<Vec<String>>,
    /// When `Some(true)`, output must start with the pattern's refinement
    /// anchor verbatim.
    #[serde(default)]
    pub starts_with_anchor: Option<bool>,
}

// ── Refinement ─────────────────────────────────────────────────────────────

/// Anchor amendment used by [`super::refiner`] on shape failure or LÆX RETRY
/// verdict.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Refinement {
    /// Sharper directive appended to the retry prompt.
    pub anchor: String,
}

// ── Verifier ───────────────────────────────────────────────────────────────

/// LÆX verifier hook declared on a primary pattern.
///
/// When `enabled=true` and the primary offload returns a shape-valid output,
/// [`super::laex_supervisor`] runs the pattern named `pattern` as a second
/// offload to vet the primary output against canon + industry-baseline.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Verifier {
    /// Master switch. If `false`, no second-pass verification occurs.
    pub enabled: bool,
    /// Pattern ID of the verifier pattern (must exist elsewhere in the catalog
    /// with `role: "verifier"`). E.g. `"PV_canon_compliance"`.
    #[serde(default)]
    pub pattern: Option<String>,
    /// What to do on a `RETRY` or `HITL` verdict: `"AUTO_RETRY"` (run one
    /// refined retry then HITL on second failure) or `"HITL"` (immediately
    /// escalate).
    #[serde(default)]
    pub escalate_on_fail: Option<String>,
    /// Cap on auto-retries before HITL. Default 1.
    #[serde(default = "default_max_auto_retries")]
    pub max_auto_retries: u8,
}

fn default_max_auto_retries() -> u8 {
    1
}

// ── Calibration ────────────────────────────────────────────────────────────

/// Calibration evidence: when the pattern was last dry-run-tested, how many
/// samples, and the observed success rate.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Calibration {
    /// ISO date string of last dry-run sweep (`None` = uncalibrated; verifier
    /// patterns may carry `None` until Day 8 calibration in this build).
    #[serde(default)]
    pub last_dry_run: Option<String>,
    /// Number of dry-runs in the most recent sweep.
    #[serde(default)]
    pub sample_count: Option<u32>,
    /// Observed success rate `[0.0, 1.0]`.
    #[serde(default)]
    pub success_rate: Option<f64>,
}

// ── Errors ─────────────────────────────────────────────────────────────────

/// Errors raised by catalog operations.
#[derive(Debug, Error)]
pub enum CatalogError {
    /// I/O failure reading the YAML file.
    #[error("read catalog file {path:?}: {source}")]
    Io {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// YAML parse failure.
    #[error("parse catalog YAML: {0}")]
    Parse(#[from] serde_yaml::Error),
    /// A pattern's `id` did not match the allowed regex.
    #[error("invalid pattern id {id:?}: must match [a-zA-Z0-9_-]{{1,64}}")]
    InvalidId {
        /// The offending id.
        id: String,
    },
    /// A pattern with `tool_use_required=true` is offload-eligible — that's a
    /// v1 invariant violation (offload is for no-tool-use patterns only).
    #[error("pattern {id:?}: tool_use_required must be false in v1")]
    ToolUseRequired {
        /// The offending id.
        id: String,
    },
    /// Schema version is unsupported.
    #[error("unsupported catalog schema version {0:?}; supported: 1.0, 1.1")]
    UnsupportedVersion(String),
    /// A `verifier.pattern` references an id that doesn't exist in the catalog.
    #[error("pattern {id:?}: verifier.pattern {verifier_id:?} not found in catalog")]
    DanglingVerifier {
        /// The pattern declaring the verifier.
        id: String,
        /// The missing verifier id.
        verifier_id: String,
    },
}

// ── Loading + validation ───────────────────────────────────────────────────

impl OffloadCatalog {
    /// Load the catalog from the canonical helix path:
    /// `$HOME/lightarchitects/soul/helix/user/standards/offload-catalog.yaml`.
    ///
    /// # Errors
    ///
    /// - [`CatalogError::Io`] if the file is missing or unreadable.
    /// - [`CatalogError::Parse`] if the YAML is malformed.
    /// - [`CatalogError::InvalidId`] / [`CatalogError::ToolUseRequired`] /
    ///   [`CatalogError::UnsupportedVersion`] / [`CatalogError::DanglingVerifier`]
    ///   on validation failure.
    pub fn load_from_helix() -> Result<Self, CatalogError> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let path = home
            .join("lightarchitects")
            .join("soul")
            .join("helix")
            .join("user")
            .join("standards")
            .join("offload-catalog.yaml");
        Self::load_from_path(&path)
    }

    /// Load the catalog from an explicit path. Useful for tests.
    ///
    /// # Errors
    ///
    /// See [`Self::load_from_helix`].
    pub fn load_from_path(path: &std::path::Path) -> Result<Self, CatalogError> {
        let bytes = std::fs::read(path).map_err(|e| CatalogError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        let catalog: Self = serde_yaml::from_slice(&bytes)?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// Parse from an in-memory YAML string. Useful for tests.
    ///
    /// # Errors
    ///
    /// See [`Self::load_from_helix`].
    pub fn from_yaml_str(yaml: &str) -> Result<Self, CatalogError> {
        let catalog: Self = serde_yaml::from_str(yaml)?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// Run structural validation: schema version, pattern ids, `tool_use`
    /// invariant, verifier references.
    fn validate(&self) -> Result<(), CatalogError> {
        match self.version.as_str() {
            "1.0" | "1.1" => {}
            other => return Err(CatalogError::UnsupportedVersion(other.to_owned())),
        }
        // Pattern id regex + tool_use invariant
        for p in &self.patterns {
            if !is_valid_pattern_id(&p.id) {
                return Err(CatalogError::InvalidId { id: p.id.clone() });
            }
            if p.eligible.tool_use_required {
                return Err(CatalogError::ToolUseRequired { id: p.id.clone() });
            }
        }
        // Verifier reference resolution
        let known_ids: std::collections::HashSet<&str> =
            self.patterns.iter().map(|p| p.id.as_str()).collect();
        for p in &self.patterns {
            if let Some(v) = &p.verifier {
                if v.enabled {
                    if let Some(ref vid) = v.pattern {
                        if !known_ids.contains(vid.as_str()) {
                            return Err(CatalogError::DanglingVerifier {
                                id: p.id.clone(),
                                verifier_id: vid.clone(),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Find a pattern by id (linear scan; catalog is small).
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Pattern> {
        self.patterns.iter().find(|p| p.id == id)
    }

    /// Filter primary (non-verifier) patterns eligible for the given sibling.
    pub fn primaries_for_sibling<'a>(
        &'a self,
        sibling: &'a str,
    ) -> impl Iterator<Item = &'a Pattern> + 'a {
        self.patterns.iter().filter(move |p| {
            p.role.as_deref() != Some("verifier")
                && p.eligible.siblings.iter().any(|s| s == sibling)
        })
    }

    /// Classify a request — does any allowlist pattern match?
    ///
    /// Returns `Some(&Pattern)` on match (caller may offload via
    /// `lightsquad_dispatch_task`), `None` otherwise (caller should pass
    /// through to the wrapped provider).
    ///
    /// # Match rules
    ///
    /// 1. `tool_definitions` must be empty (offload can't carry `tool_use`).
    /// 2. Pattern must list the calling sibling in `eligible.siblings`.
    /// 3. Pattern's `id` must appear as an explicit `kind` hint via the
    ///    `ClassificationHint` argument — v1 uses caller-declared hints
    ///    rather than fuzzy template matching (which is error-prone).
    ///
    /// The caller-declared hint comes from the caller's per-task knowledge of
    /// which pattern fits. Strategies declare hints via `kind` in their
    /// `AgentRequest` extensions; the `OffloadAwareProvider` extracts the hint.
    /// When no hint is present, this returns `None` — better to fall through
    /// than to mis-route a non-matching task.
    #[must_use]
    pub fn classify(&self, hint: ClassificationHint<'_>) -> Option<&Pattern> {
        if hint.tool_use_present {
            return None;
        }
        let pattern_id = hint.pattern_id?;
        let p = self.get(pattern_id)?;
        if p.role.as_deref() == Some("verifier") {
            // Verifier patterns are not directly invokable by classify; LÆX
            // supervisor invokes them by id.
            return None;
        }
        if !p.eligible.siblings.iter().any(|s| s == hint.sibling) {
            return None;
        }
        Some(p)
    }
}

/// Input to [`OffloadCatalog::classify`].
///
/// Sibling and `tool_use` are derived from the request; `pattern_id` is a
/// caller-declared hint (taken from `kind` field on `AgentRequest`-extensions
/// once wired by `OffloadAwareProvider`).
#[derive(Debug, Clone, Copy)]
pub struct ClassificationHint<'a> {
    /// Sibling identifier (lower-case kebab) from `AgentRequest.sibling_identity`.
    pub sibling: &'a str,
    /// Caller-declared pattern id, e.g. `"P3"`.
    pub pattern_id: Option<&'a str>,
    /// Whether the request carries `tool_definitions`. Offload requires false.
    pub tool_use_present: bool,
}

// ── Internal helpers ───────────────────────────────────────────────────────

fn is_valid_pattern_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    id.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const MIN_VALID_YAML: &str = r#"
version: "1.1"
last_calibrated: "2026-06-08"
default_model: "glm-5.1:cloud"
patterns:
  - id: P1
    name: "Explain code"
    template: "In one sentence: {{code}}"
    eligible:
      siblings: ["claude", "corso"]
      tool_use_required: false
      max_input_tokens: 4000
    shape:
      kind: "sentence_no_fences"
      max_words: 50
      forbidden_substrings: ["```"]
    refinement:
      anchor: "respond with one sentence"
    calibration:
      last_dry_run: "2026-06-08"
      sample_count: 3
      success_rate: 1.0
"#;

    const FULL_V11_YAML: &str = r#"
version: "1.1"
patterns:
  - id: P3
    name: "Generate function"
    template: "Write {{lang}} function {{name}}"
    eligible:
      siblings: ["corso"]
      tool_use_required: false
    context_sources:
      default:
        - kind: "canon"
          doc: "builders-cookbook"
          anchor: "§63"
          token_budget: 800
      overrides:
        corso:
          - kind: "industry-baseline"
            category: "security"
            path: "owasp/LLM01.md"
            token_budget: 600
    shape:
      kind: "function_no_fences"
      starts_with_anchor: true
      forbidden_substrings: ["```"]
    refinement:
      anchor: "respond starting with function"
    verifier:
      enabled: true
      pattern: "PV_canon"
      escalate_on_fail: "AUTO_RETRY"
      max_auto_retries: 1
    calibration:
      last_dry_run: "2026-06-08"
      sample_count: 3
      success_rate: 0.83
  - id: PV_canon
    name: "Verifier"
    role: "verifier"
    template: "Vet {{primary_output}}"
    eligible:
      siblings: ["laex"]
      tool_use_required: false
    shape:
      kind: "json_object"
      required_keys: ["verdict", "reason", "amendment_hint"]
      verdict_enum: ["PASS", "RETRY", "HITL"]
    calibration:
      last_dry_run: null
"#;

    #[test]
    fn parses_minimal_v11_yaml() {
        let cat = OffloadCatalog::from_yaml_str(MIN_VALID_YAML).unwrap();
        assert_eq!(cat.version, "1.1");
        assert_eq!(cat.patterns.len(), 1);
        assert_eq!(cat.patterns[0].id, "P1");
    }

    #[test]
    fn parses_full_v11_with_context_sources_and_verifier() {
        let cat = OffloadCatalog::from_yaml_str(FULL_V11_YAML).unwrap();
        let p3 = cat.get("P3").unwrap();
        let ctx = p3.context_sources.as_ref().unwrap();
        assert_eq!(ctx.default.len(), 1);
        assert!(ctx.overrides.contains_key("corso"));
        let v = p3.verifier.as_ref().unwrap();
        assert!(v.enabled);
        assert_eq!(v.pattern.as_deref(), Some("PV_canon"));
        assert_eq!(v.max_auto_retries, 1);
    }

    #[test]
    fn rejects_unsupported_version() {
        let bad = r#"
version: "9.9"
patterns: []
"#;
        let err = OffloadCatalog::from_yaml_str(bad).unwrap_err();
        assert!(matches!(err, CatalogError::UnsupportedVersion(v) if v == "9.9"));
    }

    #[test]
    fn rejects_invalid_pattern_id() {
        let bad = r#"
version: "1.1"
patterns:
  - id: "bad id with spaces"
    name: "x"
    template: "x"
    eligible:
      siblings: ["claude"]
      tool_use_required: false
    shape:
      kind: "sentence_no_fences"
    calibration: {}
"#;
        let err = OffloadCatalog::from_yaml_str(bad).unwrap_err();
        assert!(matches!(err, CatalogError::InvalidId { .. }));
    }

    #[test]
    fn rejects_tool_use_required_invariant_violation() {
        let bad = r#"
version: "1.1"
patterns:
  - id: P1
    name: "x"
    template: "x"
    eligible:
      siblings: ["claude"]
      tool_use_required: true
    shape:
      kind: "sentence_no_fences"
    calibration: {}
"#;
        let err = OffloadCatalog::from_yaml_str(bad).unwrap_err();
        assert!(matches!(err, CatalogError::ToolUseRequired { .. }));
    }

    #[test]
    fn rejects_dangling_verifier_reference() {
        let bad = r#"
version: "1.1"
patterns:
  - id: P1
    name: "x"
    template: "x"
    eligible:
      siblings: ["claude"]
      tool_use_required: false
    shape:
      kind: "sentence_no_fences"
    verifier:
      enabled: true
      pattern: "PV_does_not_exist"
    calibration: {}
"#;
        let err = OffloadCatalog::from_yaml_str(bad).unwrap_err();
        assert!(matches!(err, CatalogError::DanglingVerifier { .. }));
    }

    #[test]
    fn classify_passthroughs_when_tool_use_present() {
        let cat = OffloadCatalog::from_yaml_str(MIN_VALID_YAML).unwrap();
        let hint = ClassificationHint {
            sibling: "claude",
            pattern_id: Some("P1"),
            tool_use_present: true,
        };
        assert!(cat.classify(hint).is_none());
    }

    #[test]
    fn classify_returns_none_for_unknown_pattern_id() {
        let cat = OffloadCatalog::from_yaml_str(MIN_VALID_YAML).unwrap();
        let hint = ClassificationHint {
            sibling: "claude",
            pattern_id: Some("P999"),
            tool_use_present: false,
        };
        assert!(cat.classify(hint).is_none());
    }

    #[test]
    fn classify_returns_none_for_ineligible_sibling() {
        let cat = OffloadCatalog::from_yaml_str(MIN_VALID_YAML).unwrap();
        let hint = ClassificationHint {
            sibling: "seraph",
            pattern_id: Some("P1"),
            tool_use_present: false,
        };
        assert!(cat.classify(hint).is_none());
    }

    #[test]
    fn classify_returns_pattern_on_match() {
        let cat = OffloadCatalog::from_yaml_str(MIN_VALID_YAML).unwrap();
        let hint = ClassificationHint {
            sibling: "corso",
            pattern_id: Some("P1"),
            tool_use_present: false,
        };
        let p = cat.classify(hint).unwrap();
        assert_eq!(p.id, "P1");
    }

    #[test]
    fn classify_excludes_verifier_patterns() {
        let cat = OffloadCatalog::from_yaml_str(FULL_V11_YAML).unwrap();
        let hint = ClassificationHint {
            sibling: "laex",
            pattern_id: Some("PV_canon"),
            tool_use_present: false,
        };
        // PV_canon has role:"verifier" → classify should refuse to return it
        // even though sibling and pattern_id otherwise match.
        assert!(cat.classify(hint).is_none());
    }

    #[test]
    fn primaries_for_sibling_excludes_verifier_role() {
        let cat = OffloadCatalog::from_yaml_str(FULL_V11_YAML).unwrap();
        let primaries: Vec<&str> = cat
            .primaries_for_sibling("corso")
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(primaries, vec!["P3"]);
        // laex eligible for PV_canon — but PV_canon is role:verifier so excluded
        let laex_primaries: Vec<&str> = cat
            .primaries_for_sibling("laex")
            .map(|p| p.id.as_str())
            .collect();
        assert!(laex_primaries.is_empty());
    }

    #[test]
    fn is_valid_pattern_id_accepts_canonical_forms() {
        assert!(is_valid_pattern_id("P1"));
        assert!(is_valid_pattern_id("P3"));
        assert!(is_valid_pattern_id("PV_canon_compliance"));
        assert!(is_valid_pattern_id("p1_security-audit"));
    }

    #[test]
    fn is_valid_pattern_id_rejects_bad_forms() {
        assert!(!is_valid_pattern_id(""));
        assert!(!is_valid_pattern_id("has spaces"));
        assert!(!is_valid_pattern_id("has/slash"));
        assert!(!is_valid_pattern_id("has.dot"));
        assert!(!is_valid_pattern_id(&"x".repeat(65)));
    }

    #[test]
    fn full_v11_yaml_passes_validation_with_verifier_reference() {
        // FULL_V11_YAML has P3 → verifier.pattern="PV_canon", and PV_canon exists.
        // Should validate cleanly.
        let cat = OffloadCatalog::from_yaml_str(FULL_V11_YAML).unwrap();
        assert_eq!(cat.patterns.len(), 2);
    }

    #[test]
    fn load_from_path_io_error_includes_path() {
        let nonexistent = std::path::Path::new("/tmp/this-catalog-does-not-exist.yaml");
        let err = OffloadCatalog::load_from_path(nonexistent).unwrap_err();
        assert!(matches!(err, CatalogError::Io { .. }));
    }

    #[test]
    fn round_trip_yaml_preserves_fields() {
        let cat = OffloadCatalog::from_yaml_str(FULL_V11_YAML).unwrap();
        let yaml = serde_yaml::to_string(&cat).unwrap();
        let reparsed = OffloadCatalog::from_yaml_str(&yaml).unwrap();
        assert_eq!(reparsed.patterns.len(), 2);
        assert_eq!(reparsed.get("P3").unwrap().id, "P3");
    }

    /// Day-12 seed regression: loads the real helix catalog if present.
    ///
    /// Skipped silently when `$HOME/lightarchitects/soul/helix/user/standards/offload-catalog.yaml`
    /// is absent (e.g. CI / contributor machines without the helix mount). This is the
    /// LASDLC Day 12 acceptance gate for the lightsquad-mesh-offload BUILD.
    #[test]
    fn day12_seed_catalog_loads_validates_and_resolves() {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let Some(home) = home else { return };
        let path = home
            .join("lightarchitects")
            .join("soul")
            .join("helix")
            .join("user")
            .join("standards")
            .join("offload-catalog.yaml");
        if !path.exists() {
            return;
        }

        let cat =
            OffloadCatalog::load_from_path(&path).expect("Day 12 catalog must parse + validate");

        // AC #2: 6 patterns total
        assert_eq!(
            cat.patterns.len(),
            6,
            "expected 6 patterns (5 primary + 1 verifier)"
        );

        // AC #3: validate() OK (load_from_path runs it; redundant assert for clarity)
        cat.validate()
            .expect("Day 12 catalog must pass structural validation");

        // AC #4: corso primaries == [P1, P2, P3] in declared order
        let corso_ids: Vec<&str> = cat
            .primaries_for_sibling("corso")
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(
            corso_ids,
            ["P1", "P2", "P3"],
            "corso primaries must be P1, P2, P3 in declared order"
        );

        // AC #5: laex has no primaries (only the verifier role)
        assert_eq!(
            cat.primaries_for_sibling("laex").count(),
            0,
            "laex must have zero primary patterns (verifier-only role)"
        );

        // AC #6: classify refuses verifier patterns even when sibling/pattern names match
        let hint = ClassificationHint {
            sibling: "corso",
            pattern_id: Some("PV_canon_compliance"),
            tool_use_present: false,
        };
        assert!(
            cat.classify(hint).is_none(),
            "classify must exclude role:verifier patterns"
        );

        // Bonus: P3 has the LAEX verifier wired to PV_canon_compliance
        let p3 = cat.get("P3").expect("P3 must exist");
        let v = p3.verifier.as_ref().expect("P3 must declare a verifier");
        assert!(v.enabled);
        assert_eq!(v.pattern.as_deref(), Some("PV_canon_compliance"));
        assert_eq!(v.max_auto_retries, 1);

        // Bonus: schema version v1.1
        assert_eq!(cat.version, "1.1");
    }

    /// Day-4 anchor-reconciliation regression: every `ContextSource::Canon` in
    /// the real helix-mounted offload-catalog.yaml MUST resolve via the
    /// Day-4 `CanonSource` slicer. Catches drift between catalog YAML and
    /// real canon doc headings.
    ///
    /// Silently skips when the helix mount is absent (CI / contributor
    /// machines without `~/lightarchitects/soul/helix/`).
    #[tokio::test]
    async fn day12_catalog_canon_anchors_all_resolve() {
        use crate::agent::offload::context::{CanonSource, ContextResolver};

        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let Some(home) = home else { return };
        let helix_root = home.join("lightarchitects").join("soul").join("helix");
        let yaml_path = helix_root
            .join("user")
            .join("standards")
            .join("offload-catalog.yaml");
        if !yaml_path.exists() {
            return;
        }
        let cat = OffloadCatalog::load_from_path(&yaml_path).expect("catalog must parse");
        let cs = CanonSource::new(helix_root);

        let mut checked = 0_usize;
        for p in &cat.patterns {
            let Some(ctx) = &p.context_sources else {
                continue;
            };
            let all_sources = ctx.default.iter().chain(ctx.overrides.values().flatten());
            for src in all_sources {
                if matches!(src, ContextSource::Canon { .. }) {
                    let resolved = cs.resolve(src, "claude").await;
                    assert!(
                        resolved.is_ok(),
                        "pattern {:?} canon source {:?} failed to resolve: {:?}",
                        p.id,
                        src,
                        resolved.err()
                    );
                    checked += 1;
                }
            }
        }
        // Sanity: the v1.1 seed declares at least 5 canon sources across P1/P3/P5.
        assert!(
            checked >= 5,
            "expected ≥5 canon sources in catalog, checked {checked}"
        );
    }
}
