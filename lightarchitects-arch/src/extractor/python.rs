//! Python source extractor — classes, functions, imports via tree-sitter (smoke coverage).

use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

use crate::{
    extractor::{ExtractError, ExtractorConfig},
    model::{ArchLevel, ArchNode, ArchRelation, ExtractedFacts, Language, RelationKind},
};

/// Extracts architecture facts from a single Python file.
///
/// Smoke-level coverage (Phase 2): collects class definitions, top-level
/// function definitions, and import statements.
///
/// # Errors
///
/// Returns [`ExtractError::FileTooLarge`] or [`ExtractError::ParseFailed`] on failure.
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
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("tree-sitter-python grammar is always valid");

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ExtractError::ParseFailed {
            path: path_str.clone(),
        })?;

    let mut facts = ExtractedFacts::default();
    let mod_id = module_id(&path_str);

    // ── Query: class definitions ─────────────────────────────────────────────
    let class_query = Query::new(
        &tree_sitter_python::LANGUAGE.into(),
        r#"(class_definition name: (identifier) @name)"#,
    )
    .expect("class query is valid");

    let mut cursor = QueryCursor::new();
    let mut class_matches = cursor.matches(&class_query, tree.root_node(), source.as_bytes());
    while let Some(m) = class_matches.next() {
        for capture in m.captures {
            let name = node_text(capture.node, source);
            facts.nodes.push(ArchNode {
                id: format!("{}::{}", mod_id, name),
                label: name.to_owned(),
                level: ArchLevel::Component,
                language: Language::Python,
                location: Some(node_location(capture.node, &path_str)),
                tags: vec!["python".into()],
            });
        }
    }

    // ── Query: function definitions ──────────────────────────────────────────
    let fn_query = Query::new(
        &tree_sitter_python::LANGUAGE.into(),
        r#"(function_definition name: (identifier) @name)"#,
    )
    .expect("fn query is valid");

    let mut cursor = QueryCursor::new();
    let mut fn_matches = cursor.matches(&fn_query, tree.root_node(), source.as_bytes());
    while let Some(m) = fn_matches.next() {
        for capture in m.captures {
            let name = node_text(capture.node, source);
            facts.nodes.push(ArchNode {
                id: format!("{}::fn::{}", mod_id, name),
                label: name.to_owned(),
                level: ArchLevel::Function,
                language: Language::Python,
                location: Some(node_location(capture.node, &path_str)),
                tags: vec!["python".into(), "fn".into()],
            });
        }
    }

    // ── Query: import statements ─────────────────────────────────────────────
    let import_query = Query::new(
        &tree_sitter_python::LANGUAGE.into(),
        r#"[
            (import_statement   name: (dotted_name) @source)
            (import_from_statement module_name: (dotted_name) @source)
        ]"#,
    )
    .expect("import query is valid");

    let mut cursor = QueryCursor::new();
    let mut import_matches = cursor.matches(&import_query, tree.root_node(), source.as_bytes());
    while let Some(m) = import_matches.next() {
        for capture in m.captures {
            let source_mod = node_text(capture.node, source);
            if source_mod.is_empty() {
                continue;
            }
            let dep_id = format!("dep::py::{}", source_mod.replace('.', "_"));
            if !facts.nodes.iter().any(|n| n.id == dep_id) {
                facts.nodes.push(ArchNode {
                    id: dep_id.clone(),
                    label: source_mod.to_owned(),
                    level: ArchLevel::Dependency,
                    language: Language::Python,
                    location: Some(node_location(capture.node, &path_str)),
                    tags: vec!["python".into(), "import".into()],
                });
            }
            facts.relations.push(ArchRelation {
                from: mod_id.clone(),
                to: dep_id,
                kind: RelationKind::Uses,
                label: None,
            });
        }
    }

    Ok(facts)
}

fn node_text<'a>(node: tree_sitter::Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

fn node_location(node: tree_sitter::Node<'_>, path: &str) -> String {
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
    fn extracts_class() {
        let src = "class Agent:\n    def run(self): pass\n";
        let facts = extract_file(Path::new("test.py"), src, &cfg()).unwrap();
        assert!(facts.nodes.iter().any(|n| n.label == "Agent"));
        assert!(
            facts
                .nodes
                .iter()
                .any(|n| n.level == crate::model::ArchLevel::Component)
        );
    }

    #[test]
    fn extracts_function() {
        let src = "def compute(x):\n    return x + 1\n";
        let facts = extract_file(Path::new("test.py"), src, &cfg()).unwrap();
        assert!(facts.nodes.iter().any(|n| n.label == "compute"));
        assert!(
            facts
                .nodes
                .iter()
                .any(|n| n.level == crate::model::ArchLevel::Function)
        );
    }

    #[test]
    fn extracts_import() {
        let src = "import os\nfrom pathlib import Path\n";
        let facts = extract_file(Path::new("test.py"), src, &cfg()).unwrap();
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
        let src = "#".repeat(2 * 1024 * 1024);
        assert!(matches!(
            extract_file(Path::new("big.py"), &src, &cfg()),
            Err(ExtractError::FileTooLarge { .. })
        ));
    }

    #[test]
    fn empty_file_produces_no_facts() {
        let facts = extract_file(Path::new("empty.py"), "", &cfg()).unwrap();
        assert!(facts.nodes.is_empty());
        assert!(facts.relations.is_empty());
    }
}
