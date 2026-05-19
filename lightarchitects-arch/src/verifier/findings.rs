//! Per-class severity caps and dedup-by-hash (M7 fold).
//!
//! Guards against finding-flood DOS: a 1000-trivial-drift input cannot
//! bury a single BLOCKING finding when caps are applied severity-first.

use crate::model::{ArchFinding, FindingClass};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// Maximum findings retained per class after capping.
pub const CAP_PER_CLASS: usize = 10;
/// Absolute maximum findings returned from a single run.
pub const CAP_TOTAL: usize = 50;

/// Deduplicates findings by content hash of `(class, node_id, description)`.
///
/// Returns `(deduped_findings, dropped_count)`.
#[must_use]
pub fn dedup(findings: Vec<ArchFinding>) -> (Vec<ArchFinding>, u32) {
    let original_len = findings.len();
    let mut seen: HashSet<u64> = HashSet::with_capacity(original_len);
    let deduped: Vec<ArchFinding> = findings
        .into_iter()
        .filter(|f| seen.insert(finding_hash(f)))
        .collect();
    let dropped = (original_len - deduped.len()) as u32;
    (deduped, dropped)
}

/// Applies per-class and total caps, severity-sorted so BLOCKING findings survive.
///
/// Returns `(capped_findings, dropped_count)`.
#[must_use]
pub fn apply_caps(mut findings: Vec<ArchFinding>) -> (Vec<ArchFinding>, u32) {
    // Sort descending by severity so Critical/High findings survive the cap.
    findings.sort_by(|a, b| b.severity.cmp(&a.severity));

    let original_len = findings.len();
    let mut class_counts: HashMap<FindingClass, usize> = HashMap::new();
    let mut result: Vec<ArchFinding> = Vec::with_capacity(findings.len().min(CAP_TOTAL));

    for finding in findings {
        if result.len() >= CAP_TOTAL {
            break;
        }
        let count = class_counts.entry(finding.class).or_insert(0);
        if *count < CAP_PER_CLASS {
            *count += 1;
            result.push(finding);
        }
    }

    let dropped = (original_len - result.len()) as u32;
    (result, dropped)
}

fn finding_hash(f: &ArchFinding) -> u64 {
    let mut hasher = DefaultHasher::new();
    f.class.hash(&mut hasher);
    f.node_id.hash(&mut hasher);
    f.description.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::model::{ArchFinding, FindingClass, Severity};

    fn make_finding(class: FindingClass, node_id: &str, severity: Severity) -> ArchFinding {
        ArchFinding {
            id: format!("{class:?}-{node_id}"),
            class,
            severity,
            node_id: node_id.to_string(),
            description: format!("finding for {node_id}"),
            remediation: None,
        }
    }

    #[test]
    fn dedup_removes_identical_class_node_description() {
        let f1 = make_finding(FindingClass::ArchDrift, "a", Severity::High);
        let f2 = make_finding(FindingClass::ArchDrift, "a", Severity::High);
        let (deduped, dropped) = dedup(vec![f1, f2]);
        assert_eq!(deduped.len(), 1);
        assert_eq!(dropped, 1);
    }

    #[test]
    fn dedup_keeps_different_nodes() {
        let f1 = make_finding(FindingClass::ArchDrift, "a", Severity::High);
        let f2 = make_finding(FindingClass::ArchDrift, "b", Severity::High);
        let (deduped, dropped) = dedup(vec![f1, f2]);
        assert_eq!(deduped.len(), 2);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn cap_limits_per_class() {
        let findings: Vec<ArchFinding> = (0..20)
            .map(|i| make_finding(FindingClass::ArchDrift, &format!("node_{i}"), Severity::Low))
            .collect();
        let (capped, dropped) = apply_caps(findings);
        assert!(capped.len() <= CAP_PER_CLASS);
        assert_eq!(dropped, 10);
    }

    #[test]
    fn cap_total_enforced() {
        let findings: Vec<ArchFinding> = (0..60)
            .map(|i| {
                let class = if i % 2 == 0 {
                    FindingClass::ArchDrift
                } else {
                    FindingClass::Complexity
                };
                make_finding(class, &format!("node_{i}"), Severity::Low)
            })
            .collect();
        let (capped, _) = apply_caps(findings);
        assert!(capped.len() <= CAP_TOTAL);
    }

    #[test]
    fn high_severity_survives_cap_over_low() {
        let mut findings: Vec<ArchFinding> = (0..15)
            .map(|i| make_finding(FindingClass::ArchDrift, &format!("low_{i}"), Severity::Low))
            .collect();
        findings.push(make_finding(
            FindingClass::ArchDrift,
            "critical_one",
            Severity::Critical,
        ));
        let (capped, _) = apply_caps(findings);
        assert!(
            capped.iter().any(|f| f.severity == Severity::Critical),
            "Critical finding must survive the cap"
        );
    }
}
