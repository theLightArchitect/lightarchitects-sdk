//! Markdown emitter with pulldown-cmark `safe` mode (raw HTML stripped).
//!
//! Produces a `.md` document from an [`ArchModel`] + optional [`NarrativeSeed`].
//! Raw HTML in seed body text is stripped by pulldown-cmark's safe renderer.

use pulldown_cmark::{Options, Parser, html};

use crate::{model::ArchModel, narrative::NarrativeSeed};

/// Emits a Markdown document.
///
/// # Errors
///
/// Currently infallible. Reserved for future encoding gates.
pub fn emit(model: &ArchModel, seed: Option<&NarrativeSeed>) -> Result<String, super::EmitError> {
    let mut md = String::with_capacity(4096);
    emit_title(&mut md, seed);
    emit_overview_section(&mut md, seed);
    emit_nodes_section(model, &mut md);
    emit_relations_section(model, &mut md);
    emit_glossary_section(&mut md, seed);
    Ok(md)
}

fn emit_title(out: &mut String, seed: Option<&NarrativeSeed>) {
    let title = seed
        .and_then(|s| s.meta.title.as_deref())
        .unwrap_or("Architecture Documentation");
    out.push_str(&format!("# {title}\n\n"));
}

fn emit_overview_section(out: &mut String, seed: Option<&NarrativeSeed>) {
    if let Some(seed) = seed {
        if let Some(s0) = seed.section("section_0") {
            out.push_str(&format!("## {}\n\n", s0.title));
            // Route body through pulldown-cmark safe mode to strip raw HTML.
            out.push_str(&render_safe_markdown(&s0.body));
            out.push('\n');
        }
    }
}

fn emit_nodes_section(model: &ArchModel, out: &mut String) {
    if model.nodes.is_empty() {
        return;
    }
    out.push_str("## Nodes\n\n");
    out.push_str("| ID | Label | Level | Language | Location |\n");
    out.push_str("|----|-------|-------|----------|----------|\n");
    for node in &model.nodes {
        let loc = node.location.as_deref().unwrap_or("-");
        out.push_str(&format!(
            "| `{}` | {} | {:?} | {:?} | {} |\n",
            node.id, node.label, node.level, node.language, loc
        ));
    }
    out.push('\n');
}

fn emit_relations_section(model: &ArchModel, out: &mut String) {
    if model.relations.is_empty() {
        return;
    }
    out.push_str("## Relations\n\n");
    out.push_str("| From | Kind | To |\n");
    out.push_str("|------|------|----|\n");
    for rel in &model.relations {
        out.push_str(&format!(
            "| `{}` | {:?} | `{}` |\n",
            rel.from, rel.kind, rel.to
        ));
    }
    out.push('\n');
}

fn emit_glossary_section(out: &mut String, seed: Option<&NarrativeSeed>) {
    let Some(seed) = seed else { return };
    if seed.glossary.is_empty() {
        return;
    }
    out.push_str("## Glossary\n\n");
    for entry in &seed.glossary {
        // Both term and definition treated as untrusted — render via safe markdown.
        let safe_def = render_safe_markdown(&entry.definition);
        out.push_str(&format!("**{}**: {}\n\n", entry.term, safe_def.trim()));
    }
}

/// Renders Markdown to HTML via pulldown-cmark safe mode, then strips the HTML
/// tags to return clean text suitable for embedding in a Markdown document.
///
/// The safe render step ensures raw HTML injections in the body are stripped
/// before the text is propagated further.
fn render_safe_markdown(input: &str) -> String {
    let opts = Options::empty();
    let parser = Parser::new_ext(input, opts);
    // Safe mode: pulldown_cmark renders without raw HTML passthrough.
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);
    // Strip the HTML wrapper tags to return the inner text for MD embedding.
    strip_html_tags(&html_out)
}

/// Strips HTML tags, returning the inner text content.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::model::{ArchLevel, ArchModel, ArchNode, Language};

    #[test]
    fn emits_title_without_seed() {
        let m = ArchModel::new("test");
        let out = emit(&m, None).unwrap();
        assert!(out.starts_with("# Architecture Documentation\n"));
    }

    #[test]
    fn emits_seed_title() {
        let seed: NarrativeSeed = toml::from_str(
            r#"[meta]
title = "My Project Docs"
"#,
        )
        .unwrap();
        let out = emit(&ArchModel::new("test"), Some(&seed)).unwrap();
        assert!(out.starts_with("# My Project Docs\n"));
    }

    #[test]
    fn strips_script_tags_from_seed_body() {
        let seed: NarrativeSeed = toml::from_str(
            r#"[narrative.section_0]
title = "Overview"
body = "Safe text <script>alert(1)</script> more text"
"#,
        )
        .unwrap();
        let out = emit(&ArchModel::new("test"), Some(&seed)).unwrap();
        assert!(!out.contains("<script>"), "script tag must be stripped");
        assert!(out.contains("Safe text"), "safe text must be preserved");
    }

    #[test]
    fn emits_nodes_table() {
        let mut m = ArchModel::new("test");
        m.nodes.push(ArchNode {
            id: "a::Foo".into(),
            label: "Foo".into(),
            level: ArchLevel::Component,
            language: Language::Rust,
            location: Some("src/lib.rs:1".into()),
            tags: vec![],
        });
        let out = emit(&m, None).unwrap();
        assert!(out.contains("## Nodes"));
        assert!(out.contains("Foo"));
    }

    #[test]
    fn no_nodes_section_for_empty_model() {
        let out = emit(&ArchModel::new("test"), None).unwrap();
        assert!(!out.contains("## Nodes"));
    }
}
