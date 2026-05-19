//! Rust source extractor — structs, fns, impl blocks, use paths via tree-sitter.

use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Parser, Query, QueryCursor};

use crate::{
    extractor::{ExtractError, ExtractorConfig},
    model::{ArchLevel, ArchNode, ArchRelation, ExtractedFacts, Language, RelationKind},
};

/// Extracts architecture facts from a single Rust source file.
///
/// Walks the tree-sitter parse tree once and collects:
/// - `struct`/`enum`/`trait` definitions → [`ArchLevel::Component`] nodes
/// - top-level `fn` items → [`ArchLevel::Function`] nodes
/// - `impl` blocks → [`RelationKind::Implements`] edges (type → trait)
/// - `use` paths → [`ArchLevel::Dependency`] nodes + [`RelationKind::Uses`] edges
///
/// # Errors
///
/// Returns [`ExtractError::FileTooLarge`] or [`ExtractError::ParseFailed`] on failure.
/// I/O errors are surfaced as [`ExtractError::Io`].
pub fn extract_file(
    path: &Path,
    source: &str,
    config: &ExtractorConfig,
) -> Result<ExtractedFacts, ExtractError> {
    let path_str = path.display().to_string();
    if source.len() > config.max_file_bytes {
        return Err(ExtractError::FileTooLarge {
            path: path_str,
            size: source.len(),
        });
    }

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("tree-sitter-rust grammar is always valid");

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ExtractError::ParseFailed {
            path: path_str.clone(),
        })?;

    let mut facts = ExtractedFacts::default();

    // ── Query: named type declarations (struct / enum / trait) ──────────────
    let type_query = Query::new(
        &tree_sitter_rust::LANGUAGE.into(),
        r#"[
            (struct_item name: (type_identifier) @name)
            (enum_item   name: (type_identifier) @name)
            (trait_item  name: (type_identifier) @name)
        ]"#,
    )
    .expect("type query is valid");

    let mut cursor = QueryCursor::new();
    let mut type_matches = cursor.matches(&type_query, tree.root_node(), source.as_bytes());
    while let Some(m) = type_matches.next() {
        for capture in m.captures {
            let name = node_text(capture.node, source);
            let location = node_location(capture.node, &path_str);
            facts.nodes.push(ArchNode {
                id: format!("{}::{}", module_id(&path_str), name),
                label: name.to_owned(),
                level: ArchLevel::Component,
                language: Language::Rust,
                location: Some(location),
                tags: vec!["rust".into()],
            });
        }
    }

    // ── Query: top-level fn items ────────────────────────────────────────────
    let fn_query = Query::new(
        &tree_sitter_rust::LANGUAGE.into(),
        r#"(function_item name: (identifier) @name)"#,
    )
    .expect("fn query is valid");

    let mut cursor = QueryCursor::new();
    let mut fn_matches = cursor.matches(&fn_query, tree.root_node(), source.as_bytes());
    while let Some(m) = fn_matches.next() {
        for capture in m.captures {
            let name = node_text(capture.node, source);
            let location = node_location(capture.node, &path_str);
            facts.nodes.push(ArchNode {
                id: format!("{}::fn::{}", module_id(&path_str), name),
                label: name.to_owned(),
                level: ArchLevel::Function,
                language: Language::Rust,
                location: Some(location),
                tags: vec!["rust".into(), "fn".into()],
            });
        }
    }

    // ── Query: impl blocks → Implements edges ───────────────────────────────
    let impl_query = Query::new(
        &tree_sitter_rust::LANGUAGE.into(),
        r#"(impl_item
              trait: (type_identifier) @trait_name
              type:  (type_identifier) @type_name)"#,
    )
    .expect("impl query is valid");

    let mut cursor = QueryCursor::new();
    let mod_id = module_id(&path_str);
    let mut impl_matches = cursor.matches(&impl_query, tree.root_node(), source.as_bytes());
    while let Some(m) = impl_matches.next() {
        if m.captures.len() >= 2 {
            let trait_name = node_text(m.captures[0].node, source);
            let type_name = node_text(m.captures[1].node, source);
            facts.relations.push(ArchRelation {
                from: format!("{}::{}", mod_id, type_name),
                to: format!("{}::{}", mod_id, trait_name),
                kind: RelationKind::Implements,
                label: None,
            });
        }
    }

    // ── Query: use declarations → Uses edges ────────────────────────────────
    extract_use_paths(tree.root_node(), source, &path_str, &mut facts);

    Ok(facts)
}

/// Recursively extracts `use` paths from the AST.
fn extract_use_paths(node: Node<'_>, source: &str, path_str: &str, facts: &mut ExtractedFacts) {
    if node.kind() == "use_declaration" {
        if let Some(path_node) = node.child_by_field_name("argument") {
            let use_path = node_text(path_node, source).to_owned();
            if !use_path.is_empty() {
                let dep_id = format!("dep::{}", use_path.replace("::", "."));
                if !facts.nodes.iter().any(|n| n.id == dep_id) {
                    facts.nodes.push(ArchNode {
                        id: dep_id.clone(),
                        label: use_path.clone(),
                        level: ArchLevel::Dependency,
                        language: Language::Rust,
                        location: Some(node_location(path_node, path_str)),
                        tags: vec!["rust".into(), "use".into()],
                    });
                }
                facts.relations.push(ArchRelation {
                    from: module_id(path_str).to_owned(),
                    to: dep_id,
                    kind: RelationKind::Uses,
                    label: None,
                });
            }
        }
    }
    let mut walker = node.walk();
    for child in node.children(&mut walker) {
        extract_use_paths(child, source, path_str, facts);
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

fn node_location(node: Node<'_>, path: &str) -> String {
    format!("{}:{}", path, node.start_position().row + 1)
}

fn module_id(path: &str) -> String {
    path.replace(['/', '\\', '.'], "::").replace("-", "_")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn cfg() -> ExtractorConfig {
        ExtractorConfig::default()
    }

    #[test]
    fn extracts_struct() {
        let src = "pub struct Foo { x: u32 }";
        let facts = extract_file(std::path::Path::new("test.rs"), src, &cfg()).unwrap();
        assert!(facts.nodes.iter().any(|n| n.label == "Foo"));
        assert!(
            facts
                .nodes
                .iter()
                .any(|n| n.level == crate::model::ArchLevel::Component)
        );
    }

    #[test]
    fn extracts_enum() {
        let src = "pub enum Color { Red, Green, Blue }";
        let facts = extract_file(std::path::Path::new("test.rs"), src, &cfg()).unwrap();
        assert!(facts.nodes.iter().any(|n| n.label == "Color"));
    }

    #[test]
    fn extracts_fn() {
        let src = "fn compute(x: u32) -> u32 { x + 1 }";
        let facts = extract_file(std::path::Path::new("test.rs"), src, &cfg()).unwrap();
        assert!(facts.nodes.iter().any(|n| n.label == "compute"));
        assert!(
            facts
                .nodes
                .iter()
                .any(|n| n.level == crate::model::ArchLevel::Function)
        );
    }

    #[test]
    fn extracts_trait() {
        let src = "pub trait Processor { fn process(&self); }";
        let facts = extract_file(std::path::Path::new("test.rs"), src, &cfg()).unwrap();
        assert!(facts.nodes.iter().any(|n| n.label == "Processor"));
    }

    #[test]
    fn extracts_impl_relation() {
        let src = "struct Bar; trait Zap {} impl Zap for Bar {}";
        let facts = extract_file(std::path::Path::new("test.rs"), src, &cfg()).unwrap();
        assert!(
            facts
                .relations
                .iter()
                .any(|r| r.kind == crate::model::RelationKind::Implements)
        );
    }

    #[test]
    fn extracts_use_path() {
        let src = "use std::collections::HashMap;";
        let facts = extract_file(std::path::Path::new("test.rs"), src, &cfg()).unwrap();
        assert!(
            facts
                .nodes
                .iter()
                .any(|n| n.level == crate::model::ArchLevel::Dependency)
        );
        assert!(
            facts
                .relations
                .iter()
                .any(|r| r.kind == crate::model::RelationKind::Uses)
        );
    }

    #[test]
    fn rejects_oversized_file() {
        let src = "x".repeat(2 * 1024 * 1024);
        let result = extract_file(std::path::Path::new("big.rs"), &src, &cfg());
        assert!(matches!(result, Err(ExtractError::FileTooLarge { .. })));
    }

    #[test]
    fn empty_file_produces_no_facts() {
        let facts = extract_file(std::path::Path::new("empty.rs"), "", &cfg()).unwrap();
        assert!(facts.nodes.is_empty());
        assert!(facts.relations.is_empty());
    }
}
