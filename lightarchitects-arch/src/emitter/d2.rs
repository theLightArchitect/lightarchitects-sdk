//! D2 diagram emitter.
//!
//! Produces a D2 (Declarative Diagramming) text file from an [`ArchModel`].
//! Labels are sanitized; D2 does not support arbitrary HTML in node labels.

use crate::model::{ArchModel, RelationKind};

/// Emits a D2 diagram for `model`.
///
/// # Errors
///
/// Currently infallible. Reserved for future encoding gates.
pub fn emit(model: &ArchModel) -> Result<String, super::EmitError> {
    let mut out = String::with_capacity(1024);
    out.push_str("direction: right\n\n");
    emit_nodes(model, &mut out);
    emit_edges(model, &mut out);
    Ok(out)
}

fn emit_nodes(model: &ArchModel, out: &mut String) {
    for node in &model.nodes {
        let id = d2_id(&node.id);
        let label = d2_label(&node.label);
        out.push_str(&format!("{id}: {label}\n"));
    }
    if !model.nodes.is_empty() {
        out.push('\n');
    }
}

fn emit_edges(model: &ArchModel, out: &mut String) {
    for rel in &model.relations {
        let from = d2_id(&rel.from);
        let to = d2_id(&rel.to);
        let conn = relation_connector(rel.kind);
        if let Some(label) = &rel.label {
            let safe = d2_label(label);
            out.push_str(&format!("{from} {conn} {to}: {safe}\n"));
        } else {
            let verb = relation_verb(rel.kind);
            out.push_str(&format!("{from} {conn} {to}: {verb}\n"));
        }
    }
}

fn relation_connector(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::Uses | RelationKind::Calls => "->",
        RelationKind::Implements | RelationKind::Spawns => "-->",
        RelationKind::Contains => "<->",
    }
}

fn relation_verb(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::Uses => "uses",
        RelationKind::Implements => "implements",
        RelationKind::Contains => "contains",
        RelationKind::Calls => "calls",
        RelationKind::Spawns => "spawns",
    }
}

/// Converts a raw id to a valid D2 identifier.
fn d2_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Sanitizes a label for D2 node/edge text.
fn d2_label(label: &str) -> String {
    label
        .chars()
        .filter(|&c| c != '"' && c != '\'' && c != '{' && c != '}')
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::model::{ArchLevel, ArchModel, ArchNode, ArchRelation, Language, RelationKind};

    #[test]
    fn direction_header_present() {
        let out = emit(&ArchModel::new("test")).unwrap();
        assert!(out.starts_with("direction: right"));
    }

    #[test]
    fn emits_node() {
        let mut m = ArchModel::new("test");
        m.nodes.push(ArchNode {
            id: "Foo".into(),
            label: "Foo".into(),
            level: ArchLevel::Component,
            language: Language::Rust,
            location: None,
            tags: vec![],
        });
        let out = emit(&m).unwrap();
        assert!(out.contains("Foo: Foo"));
    }

    #[test]
    fn emits_edge_with_verb() {
        let mut m = ArchModel::new("test");
        m.relations.push(ArchRelation {
            from: "A".into(),
            to: "B".into(),
            kind: RelationKind::Uses,
            label: None,
        });
        let out = emit(&m).unwrap();
        assert!(out.contains("A -> B: uses"));
    }

    #[test]
    fn sanitizes_braces_in_label() {
        let mut m = ArchModel::new("test");
        m.nodes.push(ArchNode {
            id: "x".into(),
            label: "foo{bar}".into(),
            level: ArchLevel::Component,
            language: Language::Rust,
            location: None,
            tags: vec![],
        });
        let out = emit(&m).unwrap();
        assert!(!out.contains('{'));
    }
}
