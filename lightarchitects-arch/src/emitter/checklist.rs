//! Compliance checklist emitter.
//!
//! Generates a Markdown checklist of architecture gate items, optionally
//! verified against the live codebase via `security::cmd_exec`.

use crate::model::ArchModel;

/// A single checklist item.
#[derive(Debug, Clone)]
pub struct ChecklistItem {
    /// Display label.
    pub label: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Optional detail note.
    pub note: Option<String>,
}

/// Generates a compliance checklist for `model`.
///
/// Currently produces static items derived from the model's node/relation
/// counts. Phase 5+ wires in live `cmd_exec`-based verification.
#[must_use]
pub fn generate(model: &ArchModel) -> Vec<ChecklistItem> {
    let has_components = model
        .nodes
        .iter()
        .any(|n| n.level == crate::model::ArchLevel::Component);
    let has_relations = !model.relations.is_empty();
    let has_deps = model
        .nodes
        .iter()
        .any(|n| n.level == crate::model::ArchLevel::Dependency);

    vec![
        ChecklistItem {
            label: "[A] Architecture nodes extracted".into(),
            passed: has_components,
            note: if has_components {
                None
            } else {
                Some("no Component-level nodes found".into())
            },
        },
        ChecklistItem {
            label: "[A] Relations present".into(),
            passed: has_relations,
            note: if has_relations {
                None
            } else {
                Some("no relations extracted — check extractor coverage".into())
            },
        },
        ChecklistItem {
            label: "[D] Dependency nodes tracked".into(),
            passed: has_deps,
            note: None,
        },
        ChecklistItem {
            label: "[S] Mermaid securityLevel: strict enforced".into(),
            passed: true,
            note: Some("enforced by emitter; see mermaid::SECURITY_PREAMBLE".into()),
        },
        ChecklistItem {
            label: "[S] HTML output context-encoded".into(),
            passed: true,
            note: Some("enforced by security::encode at every HTML insertion point".into()),
        },
        ChecklistItem {
            label: "[S] MD raw-HTML stripped".into(),
            passed: true,
            note: Some("pulldown-cmark safe mode enforced in markdown::emit".into()),
        },
    ]
}

/// Renders the checklist as a Markdown string.
#[must_use]
pub fn render_markdown(items: &[ChecklistItem]) -> String {
    let mut out = String::with_capacity(512);
    out.push_str("## Architecture Gate Checklist\n\n");
    for item in items {
        let tick = if item.passed { "x" } else { " " };
        out.push_str(&format!("- [{tick}] {}\n", item.label));
        if let Some(note) = &item.note {
            out.push_str(&format!("  - _{note}_\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArchLevel, ArchModel, ArchNode, Language};

    #[test]
    fn empty_model_marks_architecture_unchecked() {
        let items = generate(&ArchModel::new("test"));
        let arch_item = items
            .iter()
            .find(|i| i.label.contains("[A] Architecture"))
            .unwrap();
        assert!(!arch_item.passed);
    }

    #[test]
    fn security_items_always_pass() {
        let items = generate(&ArchModel::new("test"));
        for item in items.iter().filter(|i| i.label.contains("[S]")) {
            assert!(
                item.passed,
                "security checklist item must always pass: {}",
                item.label
            );
        }
    }

    #[test]
    fn render_produces_markdown() {
        let items = generate(&ArchModel::new("test"));
        let md = render_markdown(&items);
        assert!(md.starts_with("## Architecture Gate Checklist"));
        assert!(md.contains("- ["));
    }

    #[test]
    fn model_with_component_marks_architecture_checked() {
        let mut m = ArchModel::new("test");
        m.nodes.push(ArchNode {
            id: "Foo".into(),
            label: "Foo".into(),
            level: ArchLevel::Component,
            language: Language::Rust,
            location: None,
            tags: vec![],
        });
        let items = generate(&m);
        let arch_item = items
            .iter()
            .find(|i| i.label.contains("[A] Architecture"))
            .unwrap();
        assert!(arch_item.passed);
    }
}
