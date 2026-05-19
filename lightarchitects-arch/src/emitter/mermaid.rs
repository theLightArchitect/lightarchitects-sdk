//! Mermaid diagram emitter with `securityLevel: 'strict'` enforced.
//!
//! Every output begins with the YAML front-matter block that sets strict mode.
//! Node labels are sanitized to prevent injection into the diagram syntax.

use crate::model::{ArchLevel, ArchModel, RelationKind};

const SECURITY_PREAMBLE: &str = "---\nconfig:\n  securityLevel: strict\n---\n";

/// Emits a Mermaid `graph TD` diagram for `model`.
///
/// All node IDs and labels are sanitized before insertion. The output always
/// begins with the `securityLevel: strict` YAML front-matter block.
///
/// # Errors
///
/// Currently infallible. Reserved for future encoding gates.
pub fn emit(model: &ArchModel) -> Result<String, super::EmitError> {
    let mut out = String::with_capacity(2048);
    out.push_str(SECURITY_PREAMBLE);
    out.push_str("graph TD\n");
    emit_nodes(model, &mut out);
    emit_edges(model, &mut out);
    Ok(out)
}

fn emit_nodes(model: &ArchModel, out: &mut String) {
    for node in &model.nodes {
        let id = mermaid_id(&node.id);
        let label = mermaid_label(&node.label);
        let shape = level_shape(node.level);
        match shape {
            NodeShape::Round => out.push_str(&format!("  {id}({label})\n")),
            NodeShape::Box => out.push_str(&format!("  {id}[{label}]\n")),
            NodeShape::Cylinder => out.push_str(&format!("  {id}[({label})]\n")),
            NodeShape::Hexagon => out.push_str(&format!("  {id}{{{{{label}}}}}\n")),
        }
    }
}

fn emit_edges(model: &ArchModel, out: &mut String) {
    for rel in &model.relations {
        let from = mermaid_id(&rel.from);
        let to = mermaid_id(&rel.to);
        let arrow = relation_arrow(rel.kind);
        if let Some(label) = &rel.label {
            let safe_label = mermaid_label(label);
            out.push_str(&format!("  {from} {arrow}|{safe_label}| {to}\n"));
        } else {
            out.push_str(&format!("  {from} {arrow} {to}\n"));
        }
    }
}

enum NodeShape {
    Round,
    Box,
    Cylinder,
    Hexagon,
}

fn level_shape(level: ArchLevel) -> NodeShape {
    match level {
        ArchLevel::Context | ArchLevel::Container => NodeShape::Round,
        ArchLevel::Component | ArchLevel::Module => NodeShape::Box,
        ArchLevel::Function => NodeShape::Hexagon,
        ArchLevel::Dependency | ArchLevel::Runtime => NodeShape::Cylinder,
    }
}

fn relation_arrow(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::Uses | RelationKind::Calls => "-->",
        RelationKind::Implements | RelationKind::Spawns => "-.->",
        RelationKind::Contains => "--o",
    }
}

/// Converts a raw id to a valid Mermaid node identifier (alphanumeric + underscore).
fn mermaid_id(id: &str) -> String {
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

/// Sanitizes a label for safe insertion inside Mermaid node syntax.
/// Strips characters that would break Mermaid parsing or allow XSS in rendered SVG.
fn mermaid_label(label: &str) -> String {
    label
        .chars()
        .filter(|&c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.' | ':' | '/'))
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::model::{ArchLevel, ArchModel, ArchNode, ArchRelation, Language, RelationKind};

    fn node(id: &str, label: &str, level: ArchLevel) -> ArchNode {
        ArchNode {
            id: id.into(),
            label: label.into(),
            level,
            language: Language::Rust,
            location: None,
            tags: vec![],
        }
    }

    #[test]
    fn always_emits_security_preamble() {
        let m = ArchModel::new("test");
        let out = emit(&m).unwrap();
        assert!(
            out.starts_with(SECURITY_PREAMBLE),
            "output must start with securityLevel: strict preamble"
        );
    }

    #[test]
    fn emits_graph_td() {
        let m = ArchModel::new("test");
        let out = emit(&m).unwrap();
        assert!(out.contains("graph TD"));
    }

    #[test]
    fn emits_component_as_box() {
        let mut m = ArchModel::new("test");
        m.nodes.push(node("Foo", "Foo", ArchLevel::Component));
        let out = emit(&m).unwrap();
        assert!(out.contains("Foo[Foo]"));
    }

    #[test]
    fn emits_dependency_as_cylinder() {
        let mut m = ArchModel::new("test");
        m.nodes.push(node("std", "std", ArchLevel::Dependency));
        let out = emit(&m).unwrap();
        assert!(out.contains("std[(std)]"));
    }

    #[test]
    fn emits_uses_edge() {
        let mut m = ArchModel::new("test");
        m.nodes.push(node("A", "A", ArchLevel::Component));
        m.nodes.push(node("B", "B", ArchLevel::Dependency));
        m.relations.push(ArchRelation {
            from: "A".into(),
            to: "B".into(),
            kind: RelationKind::Uses,
            label: None,
        });
        let out = emit(&m).unwrap();
        assert!(out.contains("A --> B"));
    }

    #[test]
    fn xss_payload_stripped_from_label() {
        let mut m = ArchModel::new("test");
        m.nodes.push(node(
            "XSS",
            "<script>alert(1)</script>",
            ArchLevel::Component,
        ));
        let out = emit(&m).unwrap();
        assert!(
            !out.contains("<script>"),
            "XSS payload must not appear in mermaid output"
        );
    }
}
