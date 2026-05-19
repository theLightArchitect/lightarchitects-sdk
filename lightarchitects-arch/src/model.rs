//! Core data model for the architecture intelligence pipeline.
//!
//! The model follows the C4 abstraction hierarchy extended with two additional levels:
//!
//! | Level | Name | Granularity |
//! |-------|------|-------------|
//! | L0 | Context | System-to-system relationships |
//! | L1 | Container | Process/binary/service boundaries |
//! | L2 | Component | Module / crate / package |
//! | L3 | Module | Sub-module / namespace |
//! | L4 | Function | Function / method |
//! | L5 | Dependency | External crate / package ref |
//! | L6 | Runtime | Process spawn / IPC channel |

use serde::{Deserialize, Serialize};

/// The C4+ abstraction level of an [`ArchNode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchLevel {
    /// L0 — context: system-to-system.
    Context,
    /// L1 — container: process or service boundary.
    Container,
    /// L2 — component: crate or package.
    Component,
    /// L3 — module: sub-module or namespace.
    Module,
    /// L4 — function: individual function or method.
    Function,
    /// L5 — dependency: external crate or npm package reference.
    Dependency,
    /// L6 — runtime: process spawn or IPC channel.
    Runtime,
}

/// A node in the architecture model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchNode {
    /// Stable, fully-qualified identifier (e.g. `lightarchitects::soul::HelixStore`).
    pub id: String,
    /// Human-readable label used in diagrams.
    pub label: String,
    /// Abstraction level.
    pub level: ArchLevel,
    /// Language the node was extracted from.
    pub language: Language,
    /// Optional source location (`file:line`).
    pub location: Option<String>,
    /// Free-form tags for diagram filtering.
    pub tags: Vec<String>,
}

/// A directed relation between two [`ArchNode`]s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchRelation {
    /// Source node identifier.
    pub from: String,
    /// Target node identifier.
    pub to: String,
    /// Relation kind.
    pub kind: RelationKind,
    /// Optional label shown on diagram edges.
    pub label: Option<String>,
}

/// The kind of a directed relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// Compile-time `use` / `import` dependency.
    Uses,
    /// Runtime invocation or message passing.
    Calls,
    /// Implementation of a trait or interface.
    Implements,
    /// Composition (struct contains field of type).
    Contains,
    /// Cross-process spawn.
    Spawns,
}

/// A quality or security finding produced by the verifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchFinding {
    /// Unique finding identifier (e.g. `SEC-001`).
    pub id: String,
    /// Finding class.
    pub class: FindingClass,
    /// Severity level.
    pub severity: Severity,
    /// Affected node identifier.
    pub node_id: String,
    /// Human-readable description.
    pub description: String,
    /// Suggested remediation (optional).
    pub remediation: Option<String>,
}

/// Classification of an [`ArchFinding`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingClass {
    /// Architectural drift from the declared diagram.
    ArchDrift,
    /// Security concern (command injection, path traversal, etc.).
    Security,
    /// Complexity violation (cyclomatic > 10, fn > 60 lines).
    Complexity,
    /// Missing public documentation.
    Documentation,
    /// Dependency cycle.
    Cycle,
    /// Unreachable / dead code path.
    DeadCode,
}

/// Finding severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational — no action required.
    Info,
    /// Low — good-to-fix.
    Low,
    /// Medium — should fix before merge.
    Medium,
    /// High — blocks merge per Agents Playbook §7.
    High,
    /// Critical — immediate security risk; blocks gate.
    Critical,
}

/// Programming language of an extracted node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Rust source.
    Rust,
    /// TypeScript source.
    TypeScript,
    /// Python source.
    Python,
    /// Unknown / unsupported.
    Unknown,
}

/// Raw extraction results before normalization into an [`ArchModel`].
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ExtractedFacts {
    /// Nodes found during extraction.
    pub nodes: Vec<ArchNode>,
    /// Relations inferred during extraction.
    pub relations: Vec<ArchRelation>,
    /// Parse errors or skipped files.
    pub warnings: Vec<String>,
}

/// The top-level architecture model — nodes + relations + findings.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ArchModel {
    /// Crate or project identifier this model was extracted from.
    pub source: String,
    /// Semantic version of the extracted source (if available).
    pub version: Option<String>,
    /// All architecture nodes.
    pub nodes: Vec<ArchNode>,
    /// All directed relations.
    pub relations: Vec<ArchRelation>,
    /// Findings from the verifier phase.
    pub findings: Vec<ArchFinding>,
}

impl ArchModel {
    /// Creates an empty model for `source`.
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            ..Default::default()
        }
    }

    /// Returns all findings at or above `min_severity`.
    #[must_use]
    pub fn findings_at_least(&self, min_severity: Severity) -> Vec<&ArchFinding> {
        self.findings
            .iter()
            .filter(|f| f.severity >= min_severity)
            .collect()
    }

    /// Returns `true` if any finding at or above `min_severity` is present.
    #[must_use]
    pub fn has_blocking_findings(&self, min_severity: Severity) -> bool {
        self.findings.iter().any(|f| f.severity >= min_severity)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn make_finding(severity: Severity) -> ArchFinding {
        ArchFinding {
            id: "TEST-001".into(),
            class: FindingClass::Security,
            severity,
            node_id: "foo".into(),
            description: "test".into(),
            remediation: None,
        }
    }

    #[test]
    fn findings_at_least_filters_correctly() {
        let mut model = ArchModel::new("test-crate");
        model.findings.push(make_finding(Severity::Low));
        model.findings.push(make_finding(Severity::High));
        model.findings.push(make_finding(Severity::Critical));
        assert_eq!(model.findings_at_least(Severity::High).len(), 2);
        assert_eq!(model.findings_at_least(Severity::Critical).len(), 1);
        assert_eq!(model.findings_at_least(Severity::Info).len(), 3);
    }

    #[test]
    fn has_blocking_findings_true_when_present() {
        let mut model = ArchModel::new("test-crate");
        model.findings.push(make_finding(Severity::Critical));
        assert!(model.has_blocking_findings(Severity::High));
    }

    #[test]
    fn has_blocking_findings_false_when_absent() {
        let mut model = ArchModel::new("test-crate");
        model.findings.push(make_finding(Severity::Low));
        assert!(!model.has_blocking_findings(Severity::High));
    }

    #[test]
    fn arch_level_ordering() {
        assert!(ArchLevel::Context < ArchLevel::Container);
        assert!(ArchLevel::Container < ArchLevel::Component);
        assert!(ArchLevel::Function < ArchLevel::Dependency);
    }
}
