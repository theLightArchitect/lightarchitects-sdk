//! Project identity types — TOML-serializable, slug-validated.
//!
//! All types match the `project.toml` frontmatter schema from Part XXI §XXI.1.
//! `Slug` is a parse-don't-validate newtype: deserialization validates atomically,
//! so downstream code never holds an unvalidated slug.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Top-level TOML document — mirrors `.lightarchitects/project.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    /// Core project identity.
    pub project: Project,
    /// Optional git configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<ProjectGit>,
    /// Agent role assignments.
    #[serde(default)]
    pub agents: ProjectAgents,
}

/// Core project identity fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// Stable UUID v7 minted at init time — never changes on rename.
    pub id: Uuid,
    /// DNS-subdomain slug (RFC 1035) — also the directory name under `~/Projects/`.
    pub slug: Slug,
    /// Human-readable project name.
    pub name: String,
    /// Whether the project has a git remote.
    pub kind: ProjectKind,
    /// UTC timestamp of first `POST /api/projects/init`.
    pub created_at: DateTime<Utc>,
    /// Path to the helix cross-reference marker for this project.
    pub helix_link: PathBuf,
}

/// Project classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectKind {
    /// Plain directory — no git remote configured at init time.
    Folder,
    /// Directory tracked by git.
    GitRepo,
}

impl Default for ProjectKind {
    fn default() -> Self {
        Self::Folder
    }
}

/// Optional git configuration stored alongside the project identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectGit {
    /// Remote URL (e.g. `https://github.com/TheLightArchitects/...`).
    pub remote: String,
    /// Default branch name.
    pub branch: String,
}

/// Agent role assignments for this project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectAgents {
    /// Agent roles actively assigned to this project.
    #[serde(default)]
    pub active: Vec<AgentRole>,
}

/// A squad member role that can be assigned to a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    /// Software engineering tasks — architecture, implementation, review.
    Engineer,
    /// Security assessment, threat modeling, `AppSec` review.
    Security,
    /// Operations — CI/CD, deploy pipelines, observability.
    Ops,
    /// Code quality, standards enforcement, clippy gates.
    Quality,
    /// Knowledge graph, helix enrichment, documentation.
    Knowledge,
    /// Investigation, prior art, dependency auditing.
    Researcher,
    /// Test design, pyramid coverage, property testing.
    Testing,
}

/// Validated project slug — DNS-subdomain compatible (RFC 1035).
///
/// Invariant: the inner string always satisfies `^[a-z0-9][a-z0-9-]{0,62}$`.
/// Constructed only via [`Slug::validate`] or deserialization (which calls it).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Slug(String);

impl Slug {
    /// Validate and construct a `Slug`.
    ///
    /// # Errors
    ///
    /// Returns [`SlugError`] when the input fails RFC 1035 subdomain rules.
    ///
    /// # Panics
    ///
    /// Never panics — the empty check above the `chars().next()` call is exhaustive.
    pub fn validate(s: &str) -> Result<Self, SlugError> {
        if s.is_empty() {
            return Err(SlugError::Empty);
        }
        if s.len() > 63 {
            return Err(SlugError::TooLong(s.len()));
        }
        // Empty case handled above; this is always `Some`.
        let Some(first) = s.chars().next() else {
            return Err(SlugError::Empty);
        };
        if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
            return Err(SlugError::InvalidStart(first));
        }
        for c in s.chars() {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
                return Err(SlugError::InvalidChar(c));
            }
        }
        Ok(Self(s.to_owned()))
    }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Slug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Slug {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::validate(&s).map_err(serde::de::Error::custom)
    }
}

/// Error variants for slug validation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SlugError {
    /// Empty input.
    #[error("slug empty")]
    Empty,
    /// Exceeds 63-character limit.
    #[error("slug too long ({0} chars, max 63)")]
    TooLong(usize),
    /// Contains an invalid character.
    #[error("slug contains invalid character: {0:?}")]
    InvalidChar(char),
    /// First character is not `[a-z0-9]`.
    #[error("slug must start with [a-z0-9], got {0:?}")]
    InvalidStart(char),
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    // Valid slugs — 10 cases per Part XXI §XXI.1 slug test pack
    #[test]
    fn slug_valid_cases() {
        let valid = [
            "foo",
            "a",
            "lightarchitects-sdk",
            "webshell-mcp-host",
            "a1",
            "1foo",
            "foo-bar-baz",
            &"x".repeat(63),
            "x9-y8-z7",
            "helix-of-helices",
        ];
        for s in valid {
            assert!(Slug::validate(s).is_ok(), "expected valid: {s:?}");
        }
    }

    // Invalid slugs — 12 cases per Part XXI §XXI.1 slug test pack
    #[test]
    fn slug_invalid_cases() {
        let invalid: &[&str] = &[
            "",              // empty
            "-foo",          // leading hyphen
            "Foo",           // uppercase
            "foo_bar",       // underscore
            "foo/bar",       // slash
            "..",            // path traversal
            "foo bar",       // space
            &"x".repeat(64), // 64 chars — too long
            "foo!",          // bang
            "foo.bar",       // dot
            "_foo",          // leading underscore
        ];
        for s in invalid {
            assert!(Slug::validate(s).is_err(), "expected invalid: {s:?}");
        }
        // emoji is invalid (non-ASCII)
        assert!(Slug::validate("foo🚀").is_err());
    }

    #[test]
    fn slug_63_chars_is_valid() {
        assert!(Slug::validate(&"x".repeat(63)).is_ok());
    }

    #[test]
    fn slug_64_chars_is_invalid() {
        assert!(Slug::validate(&"x".repeat(64)).is_err());
    }
}
