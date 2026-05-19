//! Narrative seed — architect-authored content merged with extracted facts.
//!
//! A `narrative-seed.toml` file provides human-authored sections, glossary
//! terms, and source anchors.  The emitter merges this content with the
//! deterministic skeleton; it never invents narrative.
//!
//! All text fields are treated as untrusted (S-4 fold): callers must route
//! them through [`crate::security::encode`] before inserting into HTML output.

use serde::{Deserialize, Serialize};

/// Top-level narrative seed, parsed from `narrative-seed.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NarrativeSeed {
    /// Seed metadata.
    #[serde(default)]
    pub meta: SeedMeta,
    /// Named narrative sections, keyed by section id (e.g. `"section_0"`).
    #[serde(default)]
    pub narrative: std::collections::BTreeMap<String, NarrativeSection>,
    /// Glossary entries.
    #[serde(default)]
    pub glossary: Vec<GlossaryEntry>,
}

/// Seed metadata block.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeedMeta {
    /// Document title (untrusted; HTML-encode before use).
    pub title: Option<String>,
    /// Semantic version of the seed file.
    pub version: Option<String>,
    /// Project this seed belongs to.
    pub project: Option<String>,
}

/// A single narrative section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeSection {
    /// Display title for this section (untrusted).
    pub title: String,
    /// Body text in Markdown (untrusted; stripped of raw HTML via pulldown-cmark safe mode).
    pub body: String,
    /// Provenance anchor — either a file/line ref or `architect_assertion`.
    #[serde(default = "default_anchor")]
    pub source_anchor: SourceAnchor,
}

/// Provenance declaration for narrative content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SourceAnchor {
    /// Content derived from a source file at specific lines.
    FileRef {
        /// Source file path.
        file: String,
        /// Line range `[start, end]`.
        lines: [u32; 2],
    },
    /// Content asserted by the architect directly (no source derivation).
    AssertionStr(String),
}

impl Default for SourceAnchor {
    fn default() -> Self {
        Self::AssertionStr("architect_assertion".into())
    }
}

fn default_anchor() -> SourceAnchor {
    SourceAnchor::default()
}

/// A glossary term + definition pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlossaryEntry {
    /// Term (untrusted; HTML-encode before display).
    pub term: String,
    /// Definition (untrusted; HTML-encode before display).
    pub definition: String,
    /// Provenance.
    #[serde(default = "default_anchor")]
    pub source_anchor: SourceAnchor,
}

impl NarrativeSeed {
    /// Parses a `NarrativeSeed` from TOML text.
    ///
    /// # Errors
    ///
    /// Returns a [`toml::de::Error`] if the TOML is malformed.
    pub fn from_toml(input: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(input)
    }

    /// Returns the narrative section for `id`, if present.
    #[must_use]
    pub fn section(&self, id: &str) -> Option<&NarrativeSection> {
        self.narrative.get(id)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const SAMPLE_TOML: &str = r#"
[meta]
title = "Architecture Documentation"
version = "1.0"
project = "lightarchitects-arch"

[narrative.section_0]
title = "Overview"
body = "This crate extracts architecture facts from source code."
source_anchor = "architect_assertion"

[[glossary]]
term = "Component"
definition = "A top-level named type (struct, class, interface)."
source_anchor = "architect_assertion"
"#;

    #[test]
    fn parses_meta() {
        let seed = NarrativeSeed::from_toml(SAMPLE_TOML).unwrap();
        assert_eq!(
            seed.meta.title.as_deref(),
            Some("Architecture Documentation")
        );
        assert_eq!(seed.meta.version.as_deref(), Some("1.0"));
    }

    #[test]
    fn parses_narrative_section() {
        let seed = NarrativeSeed::from_toml(SAMPLE_TOML).unwrap();
        let s0 = seed.section("section_0").unwrap();
        assert_eq!(s0.title, "Overview");
        assert!(s0.body.contains("extracts architecture facts"));
    }

    #[test]
    fn parses_glossary() {
        let seed = NarrativeSeed::from_toml(SAMPLE_TOML).unwrap();
        assert_eq!(seed.glossary.len(), 1);
        assert_eq!(seed.glossary[0].term, "Component");
    }

    #[test]
    fn empty_toml_produces_defaults() {
        let seed = NarrativeSeed::from_toml("").unwrap();
        assert!(seed.narrative.is_empty());
        assert!(seed.glossary.is_empty());
    }
}
