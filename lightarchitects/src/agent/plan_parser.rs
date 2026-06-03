//! LASDLC plan file parser — extracts phase/wave/task structure from validated plans.
//!
//! ## Two-path extraction
//!
//! 1. **YAML path** — `phase_set:` key in frontmatter `extra` (structured machine-written).
//! 2. **Markdown path** — `### Phase N` section headers with `- **Wave N.M**:` bullets
//!    (human-authored plans in the canonical LASDLC style).
//!
//! Returns [`ParserError::NotLasdlcCompliant`] if neither path yields at least one phase,
//! or if mandatory validation predicates fail.

use regex::Regex;
use serde::Deserialize;
use std::sync::OnceLock;

use crate::helix::ingestion::frontmatter;

// ────────────────────────────────────────────────────────────────────────────
// Public types
// ────────────────────────────────────────────────────────────────────────────

/// A parsed representation of a LASDLC build plan.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPlan {
    /// Plan codename from frontmatter (`codename:` field).
    pub codename: Option<String>,
    /// Template version string (e.g. `"2.8.2"`).
    pub template_version: String,
    /// Canonical hierarchy tier (`"SMALL"`, `"MEDIUM"`, or `"LARGE"`).
    pub canonical_hierarchy: String,
    /// Northstar text from `northstar_lineage.northstar_text`.
    pub northstar_text: Option<String>,
    /// Phrases from `northstar_lineage.shipped_means_5_conditions`.
    pub shipped_means: Vec<String>,
    /// Ordered phase list with wave sub-structure.
    pub phases: Vec<PlanPhase>,
}

/// A single LASDLC phase with its constituent waves.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanPhase {
    /// Phase number (1-based).
    pub number: u32,
    /// Phase name / title (e.g. `"Backend Foundations"`).
    pub name: String,
    /// Wave list within this phase.
    pub waves: Vec<PlanWave>,
}

/// A wave within a LASDLC phase.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanWave {
    /// Wave identifier of form `N.M` (e.g. wave 2 of phase 3 → `"3.2"`).
    pub id: String,
    /// Task prompts extracted from the wave description bullet.
    pub tasks: Vec<String>,
}

/// Errors returned by [`LasdlcPlanParser::parse`].
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ParserError {
    /// Plan does not meet LASDLC structural requirements.
    #[error("not LASDLC-compliant: {0}")]
    NotLasdlcCompliant(String),

    /// No `---` frontmatter block was found at all.
    #[error("frontmatter missing or unparseable")]
    FrontmatterMissing,

    /// Template version present but below the minimum required.
    #[error("template version too old: found {found}, minimum 2.8.0")]
    VersionTooOld {
        /// The version string that was found in the frontmatter.
        found: String,
    },

    /// `validation_status` is not `VALIDATED`.
    #[error("validation status not VALIDATED: found '{found}'")]
    ValidationStatusInvalid {
        /// The status string that was found in the frontmatter.
        found: String,
    },

    /// Frontmatter was parseable but no phase data could be extracted.
    #[error("no phases found via YAML or Markdown path")]
    NoPhasesFound,
}

// ────────────────────────────────────────────────────────────────────────────
// Parser
// ────────────────────────────────────────────────────────────────────────────

/// Stateless LASDLC plan parser.
///
/// Call [`LasdlcPlanParser::parse`] as a pure function. Does **not** do any
/// I/O — pass the plan file content as a `&str`.
pub struct LasdlcPlanParser;

impl LasdlcPlanParser {
    /// Parse a LASDLC plan from its raw markdown content.
    ///
    /// # Errors
    ///
    /// - [`ParserError::FrontmatterMissing`] — no `---` block found
    /// - [`ParserError::VersionTooOld`] — `lasdlc_template_version < 2.8.0`
    /// - [`ParserError::ValidationStatusInvalid`] — `validation_status != VALIDATED`
    /// - [`ParserError::NoPhasesFound`] — neither YAML nor Markdown path yielded phases
    /// - [`ParserError::NotLasdlcCompliant`] — any other structural defect
    pub fn parse(content: &str) -> Result<ParsedPlan, ParserError> {
        // ── 1. Extract frontmatter ───────────────────────────────────────────
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Err(ParserError::FrontmatterMissing);
        }
        let (fm, body) = frontmatter::parse(content);

        // If extra is empty and codename/validation_status aren't present via
        // known fields, the frontmatter parse silently returned defaults — treat
        // that as a missing frontmatter block.
        if fm.extra.is_empty() && fm.title.is_none() {
            return Err(ParserError::FrontmatterMissing);
        }

        // ── 2. Validate template version ────────────────────────────────────
        let version = fm
            .extra
            .get("lasdlc_template_version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_matches('"');
        if version.is_empty() {
            return Err(ParserError::NotLasdlcCompliant(
                "lasdlc_template_version missing from frontmatter".into(),
            ));
        }
        if !version_at_least(version, (2, 8, 0)) {
            return Err(ParserError::VersionTooOld {
                found: version.to_owned(),
            });
        }

        // ── 3. Validate status ───────────────────────────────────────────────
        let status = fm
            .extra
            .get("validation_status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_matches('"');
        if status != "VALIDATED" {
            return Err(ParserError::ValidationStatusInvalid {
                found: status.to_owned(),
            });
        }

        // ── 4. Extract common metadata ───────────────────────────────────────
        let codename = fm
            .extra
            .get("codename")
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        let canonical_hierarchy = fm
            .extra
            .get("canonical_hierarchy")
            .and_then(|v| v.as_str())
            .unwrap_or("MEDIUM")
            .to_owned();

        let northstar_text = fm
            .extra
            .get("northstar_lineage")
            .and_then(|nl| nl.get("northstar_text"))
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        // shipped_means_5_conditions — may be an array of strings or objects
        let shipped_means = extract_shipped_means(&fm.extra);

        // ── 5. Phase extraction — try YAML path first, then Markdown ─────────
        let phases = if let Some(yaml_phases) = try_yaml_path(&fm.extra) {
            yaml_phases
        } else if let Some(md_phases) = try_markdown_path(body) {
            md_phases
        } else {
            return Err(ParserError::NoPhasesFound);
        };

        Ok(ParsedPlan {
            codename,
            template_version: version.to_owned(),
            canonical_hierarchy,
            northstar_text,
            shipped_means,
            phases,
        })
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Version comparison helpers
// ────────────────────────────────────────────────────────────────────────────

/// Returns `true` if `version` represents a version ≥ `(major, minor, patch)`.
///
/// Only the numeric prefix `major.minor.patch` is compared; pre-release
/// or build-metadata suffixes are ignored.
fn version_at_least(version: &str, (req_maj, req_min, req_pat): (u32, u32, u32)) -> bool {
    let parts: Vec<u32> = version
        .split('.')
        .take(3)
        .map(|s| s.parse::<u32>().unwrap_or(0))
        .collect();
    let (maj, min, pat) = (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    );
    (maj, min, pat) >= (req_maj, req_min, req_pat)
}

// ────────────────────────────────────────────────────────────────────────────
// shipped_means extraction
// ────────────────────────────────────────────────────────────────────────────

fn extract_shipped_means(
    extra: &std::collections::HashMap<String, serde_json::Value>,
) -> Vec<String> {
    let Some(nl) = extra.get("northstar_lineage") else {
        return Vec::new();
    };
    let Some(sm) = nl.get("shipped_means_5_conditions") else {
        return Vec::new();
    };
    match sm {
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        serde_json::Value::String(s) => s
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_owned)
            .collect(),
        serde_json::Value::Object(_) => {
            // Object form: collect all string values
            sm.as_object()
                .map(|obj| {
                    obj.values()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Path A — YAML phase_set in frontmatter extra
// ────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct YamlPhase {
    number: Option<u32>,
    name: Option<String>,
    waves: Option<Vec<YamlWave>>,
}

#[derive(Deserialize)]
struct YamlWave {
    id: Option<String>,
    tasks: Option<Vec<String>>,
}

#[allow(clippy::cast_possible_truncation)]
fn try_yaml_path(
    extra: &std::collections::HashMap<String, serde_json::Value>,
) -> Option<Vec<PlanPhase>> {
    let phase_set = extra.get("phase_set")?;
    let phases: Vec<YamlPhase> = serde_json::from_value(phase_set.clone()).ok()?;
    if phases.is_empty() {
        return None;
    }
    let result = phases
        .into_iter()
        .enumerate()
        .map(|(i, yp)| PlanPhase {
            number: yp.number.unwrap_or(i as u32 + 1),
            name: yp.name.unwrap_or_else(|| format!("Phase {}", i + 1)),
            waves: yp
                .waves
                .unwrap_or_default()
                .into_iter()
                .enumerate()
                .map(|(wi, yw)| PlanWave {
                    id: yw.id.unwrap_or_else(|| format!("{}.{}", i + 1, wi + 1)),
                    tasks: yw.tasks.unwrap_or_default(),
                })
                .collect(),
        })
        .collect();
    Some(result)
}

// ────────────────────────────────────────────────────────────────────────────
// Path B — Markdown phase/wave extraction
// ────────────────────────────────────────────────────────────────────────────

#[allow(clippy::expect_used)]
fn phase_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Matches: `### Phase 2 — Backend Foundations` or `## Phase 2: ...`
        // Captures: group 1 = phase number, group 2 = remainder of title
        Regex::new(r"^#{2,3} Phase (\d+)[^a-zA-Z0-9]*(.*)$").expect("static regex")
    })
}

#[allow(clippy::expect_used)]
fn wave_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Matches: `- **Wave 2.1**: implement ...` (with optional leading whitespace)
        // Captures: group 1 = wave id (e.g. "2.1"), group 2 = task description
        Regex::new(r"^\s*-\s+\*\*Wave\s+(\d+\.\d+)\*\*[:\s]*(.*)$").expect("static regex")
    })
}

#[allow(clippy::cast_possible_truncation)]
fn try_markdown_path(body: &str) -> Option<Vec<PlanPhase>> {
    let phase_re = phase_regex();
    let wave_re = wave_regex();

    let mut phases: Vec<PlanPhase> = Vec::new();
    let mut current_phase: Option<PlanPhase> = None;

    for line in body.lines() {
        if let Some(caps) = phase_re.captures(line) {
            // Commit previous phase
            if let Some(prev) = current_phase.take() {
                phases.push(prev);
            }
            let number: u32 = caps[1].parse().unwrap_or(phases.len() as u32 + 1);
            let raw_name = caps[2].trim();
            // Strip trailing gate/criteria annotations like "(Canon XLI design inputs)"
            let name = raw_name.trim_end_matches([')', ']']).trim().to_owned();
            current_phase = Some(PlanPhase {
                number,
                name,
                waves: Vec::new(),
            });
        } else if let Some(ref mut phase) = current_phase {
            if let Some(caps) = wave_re.captures(line) {
                let wave_id = caps[1].to_owned();
                let task_text = caps[2].trim().to_owned();
                // Check if this wave ID already exists (duplicate lines); append task
                if let Some(existing) = phase.waves.iter_mut().find(|w| w.id == wave_id) {
                    if !task_text.is_empty() {
                        existing.tasks.push(task_text);
                    }
                } else {
                    let tasks = if task_text.is_empty() {
                        Vec::new()
                    } else {
                        vec![task_text]
                    };
                    phase.waves.push(PlanWave { id: wave_id, tasks });
                }
            }
        }
    }

    if let Some(last) = current_phase {
        phases.push(last);
    }

    if phases.is_empty() {
        None
    } else {
        Some(phases)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const VALIDATED_PLAN: &str = r#"---
project: test-project
codename: test-build
lasdlc_template_version: "2.8.2"
canonical_hierarchy: MEDIUM
validation_status: VALIDATED
northstar_lineage:
  northstar_text: "Test Northstar"
  shipped_means_5_conditions:
    - "condition 1"
    - "condition 2"
---

## Overview

This is the plan body.

### Phase 1 — Architecture

Architecture phase description.

- **Wave 1.1**: author diagrams

### Phase 2 — Implementation

- **Wave 2.1**: implement backend
- **Wave 2.2**: implement frontend
"#;

    const OLD_VERSION_PLAN: &str = r#"---
codename: old-build
lasdlc_template_version: "2.7.0"
validation_status: VALIDATED
canonical_hierarchy: SMALL
---
"#;

    const DRAFT_PLAN: &str = r#"---
codename: draft-build
lasdlc_template_version: "2.8.2"
validation_status: draft
canonical_hierarchy: MEDIUM
---

### Phase 1 — Architecture

- **Wave 1.1**: author diagrams
"#;

    const NO_FRONTMATTER: &str = r"# Just a heading

No frontmatter here.

### Phase 1 — Architecture
";

    #[test]
    fn happy_path_markdown_phases_parsed() {
        let result = LasdlcPlanParser::parse(VALIDATED_PLAN);
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        let plan = result.unwrap();
        assert_eq!(plan.codename.as_deref(), Some("test-build"));
        assert_eq!(plan.template_version, "2.8.2");
        assert_eq!(plan.canonical_hierarchy, "MEDIUM");
        assert_eq!(plan.northstar_text.as_deref(), Some("Test Northstar"));
        assert_eq!(plan.shipped_means, vec!["condition 1", "condition 2"]);
        assert_eq!(plan.phases.len(), 2);
        assert_eq!(plan.phases[0].number, 1);
        assert_eq!(plan.phases[0].waves.len(), 1);
        assert_eq!(plan.phases[0].waves[0].id, "1.1");
        assert_eq!(plan.phases[1].waves.len(), 2);
        assert_eq!(plan.phases[1].waves[0].id, "2.1");
        assert_eq!(plan.phases[1].waves[1].id, "2.2");
    }

    #[test]
    fn happy_path_yaml_phase_set_parsed() {
        let content = r#"---
codename: yaml-build
lasdlc_template_version: "2.8.0"
validation_status: VALIDATED
canonical_hierarchy: SMALL
phase_set:
  - number: 1
    name: "Architecture"
    waves:
      - id: "1.1"
        tasks: ["author diagrams"]
  - number: 2
    name: "Build"
    waves:
      - id: "2.1"
        tasks: ["implement"]
---

Body content here.
"#;
        let plan = LasdlcPlanParser::parse(content).expect("should parse");
        assert_eq!(plan.phases.len(), 2);
        assert_eq!(plan.phases[0].name, "Architecture");
        assert_eq!(plan.phases[0].waves[0].tasks, vec!["author diagrams"]);
    }

    #[test]
    fn error_missing_frontmatter() {
        let err = LasdlcPlanParser::parse(NO_FRONTMATTER).unwrap_err();
        assert!(
            matches!(err, ParserError::FrontmatterMissing),
            "expected FrontmatterMissing, got: {err:?}"
        );
    }

    #[test]
    fn error_version_too_old() {
        let err = LasdlcPlanParser::parse(OLD_VERSION_PLAN).unwrap_err();
        assert!(
            matches!(err, ParserError::VersionTooOld { .. }),
            "expected VersionTooOld, got: {err:?}"
        );
    }

    #[test]
    fn error_validation_status_draft() {
        let err = LasdlcPlanParser::parse(DRAFT_PLAN).unwrap_err();
        assert!(
            matches!(err, ParserError::ValidationStatusInvalid { .. }),
            "expected ValidationStatusInvalid, got: {err:?}"
        );
    }

    #[test]
    fn error_no_phases_found() {
        let content = r#"---
lasdlc_template_version: "2.8.2"
validation_status: VALIDATED
canonical_hierarchy: SMALL
---

## Overview

No phase headers here.
"#;
        let err = LasdlcPlanParser::parse(content).unwrap_err();
        assert!(
            matches!(err, ParserError::NoPhasesFound),
            "expected NoPhasesFound, got: {err:?}"
        );
    }

    #[test]
    fn error_phase_headers_no_wave_structure() {
        // Phases present but no - **Wave N.M**: bullets → phases parsed with empty wave list.
        // This should succeed (phases exist), just with empty waves.
        let content = r#"---
lasdlc_template_version: "2.8.2"
validation_status: VALIDATED
canonical_hierarchy: SMALL
---

### Phase 1 — Architecture

Architecture text without wave bullets.
"#;
        // Phases ARE found (the Phase 1 header), waves are just empty.
        // This is still LASDLC-compliant — a phase with no defined waves is valid.
        let plan = LasdlcPlanParser::parse(content).expect("should parse");
        assert_eq!(plan.phases.len(), 1);
        assert_eq!(plan.phases[0].waves.len(), 0);
    }

    #[test]
    fn version_comparison_boundary() {
        assert!(version_at_least("2.8.0", (2, 8, 0)));
        assert!(version_at_least("2.8.1", (2, 8, 0)));
        assert!(version_at_least("2.9.0", (2, 8, 0)));
        assert!(version_at_least("3.0.0", (2, 8, 0)));
        assert!(!version_at_least("2.7.9", (2, 8, 0)));
        assert!(!version_at_least("2.7.0", (2, 8, 0)));
        assert!(!version_at_least("1.99.99", (2, 8, 0)));
    }
}

#[cfg(test)]
mod property_tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        /// Parsing arbitrary string as a plan MUST never panic.
        /// Result is either Ok or one of the known error variants.
        #[test]
        fn random_input_never_panics(input in ".*") {
            let result = LasdlcPlanParser::parse(&input);
            // Just ensure it doesn't panic and returns a known variant.
            let _: Result<_, ParserError> = result;
        }

        /// Parsing valid frontmatter prefix with arbitrary body never panics.
        #[test]
        fn valid_frontmatter_prefix_random_body_never_panics(
            version in prop_oneof![
                Just("2.8.0"), Just("2.8.2"), Just("3.0.0"), Just("1.0.0"), Just("2.7.9")
            ],
            status in prop_oneof![
                Just("VALIDATED"), Just("draft"), Just("in-progress"), Just("")
            ],
            body in ".*"
        ) {
            let content = format!(
                "---\nlasdlc_template_version: \"{version}\"\nvalidation_status: {status}\ncanonical_hierarchy: MEDIUM\n---\n{body}"
            );
            let result = LasdlcPlanParser::parse(&content);
            // Ensure no panic; validate error consistency.
            if status != "VALIDATED" {
                assert!(
                    matches!(result, Err(ParserError::ValidationStatusInvalid { .. }))
                        || !version_at_least(version, (2, 8, 0)) && matches!(result, Err(ParserError::VersionTooOld { .. })),
                    "expected validation or version error for non-VALIDATED status"
                );
            }
        }
    }
}
