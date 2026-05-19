//! Structural diff between a planned and a current [`ArchModel`].
//!
//! Findings are severity-graded: removals are higher-severity than additions
//! because a removed element means a contract was broken, while an addition
//! means the diagram is merely stale.

use crate::model::{ArchFinding, ArchLevel, ArchModel, FindingClass, Severity};
use std::collections::HashSet;
use tracing::instrument;

/// Diffs `planned` vs `current` and returns [`ArchFinding`]s for each divergence.
///
/// Child spans are emitted per check class (H7 observability requirement).
#[instrument(skip_all, fields(planned = %planned.source, current = %current.source))]
pub fn diff(planned: &ArchModel, current: &ArchModel) -> Vec<ArchFinding> {
    let mut findings = Vec::new();
    findings.extend(route_diff(planned, current));
    findings.extend(struct_diff(planned, current));
    findings.extend(dep_diff(planned, current));
    findings.extend(relation_diff(planned, current));
    findings
}

/// Context/Container level drift — broken system/service boundaries.
#[instrument(skip_all)]
fn route_diff(planned: &ArchModel, current: &ArchModel) -> Vec<ArchFinding> {
    let planned_ids: HashSet<&str> = planned
        .nodes
        .iter()
        .filter(|n| matches!(n.level, ArchLevel::Context | ArchLevel::Container))
        .map(|n| n.id.as_str())
        .collect();

    let current_ids: HashSet<&str> = current
        .nodes
        .iter()
        .filter(|n| matches!(n.level, ArchLevel::Context | ArchLevel::Container))
        .map(|n| n.id.as_str())
        .collect();

    let mut out = Vec::new();

    for id in planned_ids.difference(&current_ids) {
        out.push(ArchFinding {
            id: format!("DRIFT-ROUTE-{id}"),
            class: FindingClass::ArchDrift,
            severity: Severity::High,
            node_id: id.to_string(),
            description: format!(
                "planned node '{id}' (Context/Container) is absent in current model"
            ),
            remediation: Some("re-run extractor or update diagram to reflect removal".into()),
        });
    }

    for id in current_ids.difference(&planned_ids) {
        out.push(ArchFinding {
            id: format!("DRIFT-ROUTE-NEW-{id}"),
            class: FindingClass::ArchDrift,
            severity: Severity::Medium,
            node_id: id.to_string(),
            description: format!("current node '{id}' (Context/Container) is not in planned model"),
            remediation: Some("add node to architecture diagram and re-validate".into()),
        });
    }

    out
}

/// Component/Module/Function level drift — structural changes within services.
#[instrument(skip_all)]
fn struct_diff(planned: &ArchModel, current: &ArchModel) -> Vec<ArchFinding> {
    let planned_ids: HashSet<&str> = planned
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                n.level,
                ArchLevel::Component | ArchLevel::Module | ArchLevel::Function
            )
        })
        .map(|n| n.id.as_str())
        .collect();

    let current_ids: HashSet<&str> = current
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                n.level,
                ArchLevel::Component | ArchLevel::Module | ArchLevel::Function
            )
        })
        .map(|n| n.id.as_str())
        .collect();

    let mut out = Vec::new();

    for id in planned_ids.difference(&current_ids) {
        out.push(ArchFinding {
            id: format!("DRIFT-STRUCT-{id}"),
            class: FindingClass::ArchDrift,
            severity: Severity::High,
            node_id: id.to_string(),
            description: format!(
                "planned node '{id}' (Component/Module/Function) is absent in current model"
            ),
            remediation: Some("verify deletion was intentional and update diagram".into()),
        });
    }

    for id in current_ids.difference(&planned_ids) {
        out.push(ArchFinding {
            id: format!("DRIFT-STRUCT-NEW-{id}"),
            class: FindingClass::ArchDrift,
            severity: Severity::Low,
            node_id: id.to_string(),
            description: format!(
                "current node '{id}' (Component/Module/Function) not in planned model"
            ),
            remediation: Some("add to architecture diagram".into()),
        });
    }

    out
}

/// Dependency level drift — external crate/package additions or removals.
#[instrument(skip_all)]
fn dep_diff(planned: &ArchModel, current: &ArchModel) -> Vec<ArchFinding> {
    let planned_ids: HashSet<&str> = planned
        .nodes
        .iter()
        .filter(|n| n.level == ArchLevel::Dependency)
        .map(|n| n.id.as_str())
        .collect();

    let current_ids: HashSet<&str> = current
        .nodes
        .iter()
        .filter(|n| n.level == ArchLevel::Dependency)
        .map(|n| n.id.as_str())
        .collect();

    let mut out = Vec::new();

    for id in planned_ids.difference(&current_ids) {
        out.push(ArchFinding {
            id: format!("DRIFT-DEP-{id}"),
            class: FindingClass::ArchDrift,
            severity: Severity::Medium,
            node_id: id.to_string(),
            description: format!("planned dependency '{id}' is no longer present"),
            remediation: Some("confirm removal was intentional; update dependency diagram".into()),
        });
    }

    for id in current_ids.difference(&planned_ids) {
        out.push(ArchFinding {
            id: format!("DRIFT-DEP-NEW-{id}"),
            class: FindingClass::ArchDrift,
            severity: Severity::Low,
            node_id: id.to_string(),
            description: format!("new dependency '{id}' is not in planned model"),
            remediation: Some("run sonatype safety check and add to architecture diagram".into()),
        });
    }

    out
}

/// Relation drift — edges added or removed between nodes.
#[instrument(skip_all)]
fn relation_diff(planned: &ArchModel, current: &ArchModel) -> Vec<ArchFinding> {
    let planned_edges: HashSet<(&str, &str)> = planned
        .relations
        .iter()
        .map(|r| (r.from.as_str(), r.to.as_str()))
        .collect();

    let current_edges: HashSet<(&str, &str)> = current
        .relations
        .iter()
        .map(|r| (r.from.as_str(), r.to.as_str()))
        .collect();

    let mut out = Vec::new();

    for (from, to) in planned_edges.difference(&current_edges) {
        out.push(ArchFinding {
            id: format!("DRIFT-REL-{from}-{to}"),
            class: FindingClass::ArchDrift,
            severity: Severity::Medium,
            node_id: from.to_string(),
            description: format!("planned relation '{from}' → '{to}' is absent in current model"),
            remediation: Some("verify relation removal was intentional; update diagram".into()),
        });
    }

    for (from, to) in current_edges.difference(&planned_edges) {
        out.push(ArchFinding {
            id: format!("DRIFT-REL-NEW-{from}-{to}"),
            class: FindingClass::ArchDrift,
            severity: Severity::Info,
            node_id: from.to_string(),
            description: format!("new relation '{from}' → '{to}' not in planned model"),
            remediation: Some("add relation to architecture diagram".into()),
        });
    }

    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::model::{ArchLevel, ArchModel, ArchNode, Language, RelationKind};

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

    fn relation(from: &str, to: &str) -> crate::model::ArchRelation {
        crate::model::ArchRelation {
            from: from.to_string(),
            to: to.to_string(),
            kind: RelationKind::Uses,
            label: None,
        }
    }

    #[test]
    fn no_diff_on_identical_models() {
        let mut m = ArchModel::new("test");
        m.nodes.push(node("a", ArchLevel::Context));
        let findings = diff(&m, &m);
        assert!(findings.is_empty());
    }

    #[test]
    fn removed_context_node_is_high() {
        let mut planned = ArchModel::new("test");
        planned.nodes.push(node("svc-a", ArchLevel::Context));
        let current = ArchModel::new("test");
        let findings = diff(&planned, &current);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::High);
    }

    #[test]
    fn added_context_node_is_medium() {
        let planned = ArchModel::new("test");
        let mut current = ArchModel::new("test");
        current.nodes.push(node("svc-new", ArchLevel::Context));
        let findings = diff(&planned, &current);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Medium);
    }

    #[test]
    fn removed_dependency_is_medium() {
        let mut planned = ArchModel::new("test");
        planned.nodes.push(node("serde", ArchLevel::Dependency));
        let current = ArchModel::new("test");
        let findings = diff(&planned, &current);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Medium);
    }

    #[test]
    fn added_relation_is_info() {
        let planned = ArchModel::new("test");
        let mut current = ArchModel::new("test");
        current.relations.push(relation("a", "b"));
        let findings = diff(&planned, &current);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Info);
    }

    #[test]
    fn removed_relation_is_medium() {
        let mut planned = ArchModel::new("test");
        planned.relations.push(relation("a", "b"));
        let current = ArchModel::new("test");
        let findings = diff(&planned, &current);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Medium);
    }
}
