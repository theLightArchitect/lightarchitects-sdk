//! TypeScript source extractor — classes, interfaces, imports via tree-sitter.

use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

use crate::{
    extractor::{ExtractError, ExtractorConfig},
    model::{ArchLevel, ArchNode, ArchRelation, ExtractedFacts, Language, RelationKind},
};

/// Extracts architecture facts from a single TypeScript (or TSX) file.
///
/// Collects:
/// - `class` declarations → [`ArchLevel::Component`] nodes
/// - `interface` declarations → [`ArchLevel::Component`] nodes
/// - `import` statements → [`ArchLevel::Dependency`] nodes + [`RelationKind::Uses`] edges
///
/// # Errors
///
/// Returns [`ExtractError::FileTooLarge`] or [`ExtractError::ParseFailed`] on failure.
#[tracing::instrument(skip_all, fields(lang = "typescript", file = %path.display()))]
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
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("tree-sitter-typescript grammar is always valid");

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ExtractError::ParseFailed {
            path: path_str.clone(),
        })?;

    let mut facts = ExtractedFacts::default();
    let mod_id = module_id(&path_str);

    // ── Query: class and interface declarations ──────────────────────────────
    let type_query = Query::new(
        &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        r#"[
            (class_declaration     name: (type_identifier) @name)
            (interface_declaration name: (type_identifier) @name)
        ]"#,
    )
    .expect("type query is valid");

    let mut cursor = QueryCursor::new();
    let mut type_matches = cursor.matches(&type_query, tree.root_node(), source.as_bytes());
    while let Some(m) = type_matches.next() {
        for capture in m.captures {
            let name = node_text(capture.node, source);
            facts.nodes.push(ArchNode {
                id: format!("{}::{}", mod_id, name),
                label: name.to_owned(),
                level: ArchLevel::Component,
                language: Language::TypeScript,
                location: Some(node_location(capture.node, &path_str)),
                tags: vec!["typescript".into()],
            });
        }
    }

    // ── Query: import statements ─────────────────────────────────────────────
    let import_query = Query::new(
        &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        r#"(import_statement source: (string (string_fragment) @source))"#,
    )
    .expect("import query is valid");

    let mut cursor = QueryCursor::new();
    let mut import_matches = cursor.matches(&import_query, tree.root_node(), source.as_bytes());
    while let Some(m) = import_matches.next() {
        for capture in m.captures {
            let source_path = node_text(capture.node, source);
            if source_path.is_empty() {
                continue;
            }
            let dep_id = format!("dep::ts::{}", sanitize_import(source_path));
            if !facts.nodes.iter().any(|n| n.id == dep_id) {
                facts.nodes.push(ArchNode {
                    id: dep_id.clone(),
                    label: source_path.to_owned(),
                    level: ArchLevel::Dependency,
                    language: Language::TypeScript,
                    location: Some(node_location(capture.node, &path_str)),
                    tags: vec!["typescript".into(), "import".into()],
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

    tracing::debug!(
        nodes = facts.nodes.len(),
        relations = facts.relations.len(),
        "typescript extraction complete"
    );
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

fn sanitize_import(s: &str) -> String {
    s.replace(['/', '.', '@', '-'], "_")
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
        let src = "class MyService { run() {} }";
        let facts = extract_file(Path::new("test.ts"), src, &cfg()).unwrap();
        assert!(facts.nodes.iter().any(|n| n.label == "MyService"));
        assert!(
            facts
                .nodes
                .iter()
                .any(|n| n.level == crate::model::ArchLevel::Component)
        );
    }

    #[test]
    fn extracts_interface() {
        let src = "interface Store { get(key: string): string; }";
        let facts = extract_file(Path::new("test.ts"), src, &cfg()).unwrap();
        assert!(facts.nodes.iter().any(|n| n.label == "Store"));
    }

    #[test]
    fn extracts_import() {
        let src = r#"import { foo } from "react";"#;
        let facts = extract_file(Path::new("test.ts"), src, &cfg()).unwrap();
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
    fn deduplicates_import_nodes() {
        let src = r#"
import { a } from "lodash";
import { b } from "lodash";
"#;
        let facts = extract_file(Path::new("test.ts"), src, &cfg()).unwrap();
        let dep_count = facts
            .nodes
            .iter()
            .filter(|n| n.level == crate::model::ArchLevel::Dependency)
            .count();
        assert_eq!(dep_count, 1, "lodash should appear only once as a dep node");
    }

    #[test]
    fn rejects_oversized_file() {
        let src = "x".repeat(2 * 1024 * 1024);
        assert!(matches!(
            extract_file(Path::new("big.ts"), &src, &cfg()),
            Err(ExtractError::FileTooLarge { .. })
        ));
    }

    #[test]
    fn empty_file_produces_no_facts() {
        let facts = extract_file(Path::new("empty.ts"), "", &cfg()).unwrap();
        assert!(facts.nodes.is_empty());
        assert!(facts.relations.is_empty());
    }
}
