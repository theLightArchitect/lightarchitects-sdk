//! Architecture drift verifier (M7).
//!
//! Compares a *planned* [`ArchModel`] against a *current* extracted model,
//! deduplicates findings by content-hash, and applies per-class severity caps
//! so a finding-flood cannot bury a single BLOCKING result.

pub mod diff;
pub mod findings;

use crate::model::{ArchFinding, ArchModel, Severity};
use tracing::instrument;

/// Output of a single verifier run.
#[derive(Debug, Clone)]
pub struct VerifierResult {
    /// All retained findings after dedup + caps.
    pub findings: Vec<ArchFinding>,
    /// Number of duplicate findings dropped.
    pub duplicates_dropped: u32,
    /// Number of findings dropped by per-class or total cap.
    pub capped_dropped: u32,
    /// `true` if any finding at or above `min_severity` is present.
    pub has_blocking: bool,
}

/// Runs the full verifier pipeline against a planned/current model pair.
///
/// Pipeline: `diff` → `dedup` → `apply_caps` → severity check.
/// All stages are instrumented for O-1 AYIN span visibility (H7).
#[instrument(skip_all, fields(planned = %planned.source, current = %current.source))]
pub fn run(
    planned: &ArchModel,
    current: &ArchModel,
    blocking_threshold: Severity,
) -> VerifierResult {
    let raw = diff::diff(planned, current);
    let (deduped, duplicates_dropped) = findings::dedup(raw);
    let (capped, capped_dropped) = findings::apply_caps(deduped);
    let has_blocking = capped.iter().any(|f| f.severity >= blocking_threshold);

    VerifierResult {
        findings: capped,
        duplicates_dropped,
        capped_dropped,
        has_blocking,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::model::{ArchLevel, ArchModel, ArchNode, Language, Severity};

    fn node(id: &str, level: ArchLevel) -> ArchNode {
        ArchNode {
            id: id.to_string(),
            label: id.to_string(),
            level,
            language: Language::Rust,
            location: None,
            tags: vec![],
        }
    }

    #[test]
    fn clean_run_no_findings() {
        let mut m = ArchModel::new("test");
        m.nodes.push(node("a", ArchLevel::Context));
        let result = run(&m, &m, Severity::High);
        assert!(result.findings.is_empty());
        assert!(!result.has_blocking);
    }

    #[test]
    fn removed_node_triggers_blocking() {
        let mut planned = ArchModel::new("test");
        planned.nodes.push(node("svc-a", ArchLevel::Context));
        let current = ArchModel::new("test");
        let result = run(&planned, &current, Severity::High);
        assert!(!result.findings.is_empty());
        assert!(result.has_blocking);
    }

    #[test]
    fn duplicates_are_deduplicated() {
        let mut planned = ArchModel::new("test");
        planned.nodes.push(node("svc-a", ArchLevel::Context));
        planned.nodes.push(node("svc-b", ArchLevel::Context));
        let current = ArchModel::new("test");
        let result = run(&planned, &current, Severity::High);
        // 2 unique removals; dedup should pass both (no dupes here), 0 dropped
        assert_eq!(result.duplicates_dropped, 0);
        assert_eq!(result.findings.len(), 2);
    }

    #[test]
    fn cap_counts_are_reported() {
        // Flood with 20 struct-level removals — cap at 10 per class
        let mut planned = ArchModel::new("test");
        for i in 0..20 {
            planned
                .nodes
                .push(node(&format!("mod_{i}"), ArchLevel::Module));
        }
        let current = ArchModel::new("test");
        let result = run(&planned, &current, Severity::High);
        assert!(result.findings.len() <= findings::CAP_PER_CLASS);
        assert!(result.capped_dropped > 0);
    }
}
