//! Likec4 DSL emitter.
//!
//! Produces a Likec4 v1.x specification from an [`ArchModel`].
//! Output is plain text (not HTML-encoded); callers embedding in HTML must encode.

use crate::model::{ArchLevel, ArchModel, RelationKind};

/// Emits the full Likec4 specification for `model`.
///
/// # Errors
///
/// Currently infallible; returns `Ok` always. [`super::EmitError`] reserved for future
/// encoding gates.
pub fn emit(model: &ArchModel) -> Result<String, super::EmitError> {
    let mut out = String::with_capacity(2048);
    emit_specification(&mut out);
    emit_model_block(model, &mut out);
    emit_views_block(model, &mut out);
    Ok(out)
}

fn emit_specification(out: &mut String) {
    out.push_str("specification {\n");
    out.push_str("  element context\n");
    out.push_str("  element container\n");
    out.push_str("  element component\n");
    out.push_str("  element module\n");
    out.push_str("  element function\n");
    out.push_str("  element dependency\n");
    out.push_str("  element runtime\n");
    out.push_str("  relationship uses\n");
    out.push_str("  relationship implements\n");
    out.push_str("  relationship contains\n");
    out.push_str("}\n\n");
}

fn emit_model_block(model: &ArchModel, out: &mut String) {
    out.push_str("model {\n");
    for node in &model.nodes {
        let kind = level_to_kind(node.level);
        let safe_id = sanitize_id(&node.id);
        let safe_label = escape_likec4_string(&node.label);
        out.push_str(&format!("  {kind} {safe_id} '{safe_label}' {{\n"));
        if let Some(loc) = &node.location {
            out.push_str(&format!("    source '{loc}'\n"));
        }
        for tag in &node.tags {
            out.push_str(&format!("    tag #{}\n", sanitize_id(tag)));
        }
        out.push_str("  }\n");
    }
    out.push('\n');
    for rel in &model.relations {
        let from = sanitize_id(&rel.from);
        let to = sanitize_id(&rel.to);
        let kind = relation_kind_str(rel.kind);
        if let Some(label) = &rel.label {
            out.push_str(&format!("  {from} -[{kind}]-> {to} '{label}'\n"));
        } else {
            out.push_str(&format!("  {from} -[{kind}]-> {to}\n"));
        }
    }
    out.push_str("}\n\n");
}

fn emit_views_block(model: &ArchModel, out: &mut String) {
    out.push_str("views {\n");
    out.push_str("  view index {\n");
    out.push_str("    title 'System Overview'\n");
    out.push_str("    include *\n");
    out.push_str("  }\n");

    // Per-component views for top-level components.
    for node in model
        .nodes
        .iter()
        .filter(|n| n.level == ArchLevel::Component)
        .take(20)
    {
        let safe_id = sanitize_id(&node.id);
        let safe_label = escape_likec4_string(&node.label);
        out.push_str(&format!("  view {safe_id}_view of {safe_id} {{\n"));
        out.push_str(&format!("    title '{safe_label} Detail'\n"));
        out.push_str("    include *, ->\n");
        out.push_str("  }\n");
    }

    out.push_str("}\n");
}

fn level_to_kind(level: ArchLevel) -> &'static str {
    match level {
        ArchLevel::Context => "context",
        ArchLevel::Container => "container",
        ArchLevel::Component => "component",
        ArchLevel::Module => "module",
        ArchLevel::Function => "function",
        ArchLevel::Dependency => "dependency",
        ArchLevel::Runtime => "runtime",
    }
}

fn relation_kind_str(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::Uses => "uses",
        RelationKind::Implements => "implements",
        RelationKind::Contains => "contains",
        RelationKind::Calls => "uses",
        RelationKind::Spawns => "spawns",
    }
}

/// Converts a raw id string to a valid Likec4 identifier.
fn sanitize_id(id: &str) -> String {
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

/// Escapes single-quotes inside a Likec4 string literal.
fn escape_likec4_string(s: &str) -> String {
    s.replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::model::{ArchLevel, ArchModel, ArchNode, ArchRelation, Language, RelationKind};

    fn simple_model() -> ArchModel {
        let mut m = ArchModel::new("test");
        m.nodes.push(ArchNode {
            id: "mod::Foo".into(),
            label: "Foo".into(),
            level: ArchLevel::Component,
            language: Language::Rust,
            location: Some("src/lib.rs:1".into()),
            tags: vec!["rust".into()],
        });
        m.nodes.push(ArchNode {
            id: "dep::std".into(),
            label: "std".into(),
            level: ArchLevel::Dependency,
            language: Language::Rust,
            location: None,
            tags: vec![],
        });
        m.relations.push(ArchRelation {
            from: "mod::Foo".into(),
            to: "dep::std".into(),
            kind: RelationKind::Uses,
            label: None,
        });
        m
    }

    #[test]
    fn contains_specification_block() {
        let out = emit(&simple_model()).unwrap();
        assert!(out.contains("specification {"));
        assert!(out.contains("element component"));
    }

    #[test]
    fn contains_model_node() {
        let out = emit(&simple_model()).unwrap();
        assert!(out.contains("'Foo'"));
        assert!(out.contains("source 'src/lib.rs:1'"));
    }

    #[test]
    fn contains_relation() {
        let out = emit(&simple_model()).unwrap();
        assert!(out.contains("-[uses]->"));
    }

    #[test]
    fn contains_views_block() {
        let out = emit(&simple_model()).unwrap();
        assert!(out.contains("views {"));
        assert!(out.contains("view index {"));
    }

    #[test]
    fn sanitizes_colons_in_ids() {
        let out = emit(&simple_model()).unwrap();
        // "mod::Foo" → "mod__Foo" (colons replaced with underscores)
        assert!(out.contains("mod__Foo"));
    }

    #[test]
    fn empty_model_produces_valid_structure() {
        let m = ArchModel::new("test");
        let out = emit(&m).unwrap();
        assert!(out.contains("specification {"));
        assert!(out.contains("model {"));
        assert!(out.contains("views {"));
    }
}
