//! HTML emitter — full document coordinator.
//!
//! Assembles sidebar + hero + sections + footer into a complete HTML5 document.
//! All model-derived text is HTML-encoded before insertion (H2 fold).

pub mod footer;
pub mod hero;
pub mod narrative_compose;
pub mod sections;
pub mod sidebar;

use crate::{model::ArchModel, narrative::NarrativeSeed};

/// Emits a complete HTML5 document for `model`.
///
/// When `skeleton_only` is `true`, narrative seed sections are omitted — only
/// the deterministic skeleton (node tables, relation tables, diagrams) is rendered.
///
/// # Errors
///
/// Returns [`super::EmitError`] if encoding of any model field fails.
#[tracing::instrument(skip_all, fields(skeleton_only))]
pub fn emit(
    model: &ArchModel,
    seed: Option<&NarrativeSeed>,
    skeleton_only: bool,
) -> Result<String, super::EmitError> {
    let title = seed
        .and_then(|s| s.meta.title.as_deref())
        .unwrap_or("Architecture Documentation");

    let sidebar_html = sidebar::render(model)?;
    let hero_html = hero::render(model, seed)?;
    let sections_html = sections::render_all(model, seed, skeleton_only)?;
    let footer_html = footer::render(skeleton_only);

    let mermaid_script = mermaid_init_script();

    Ok(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{title}</title>
  <meta http-equiv="Content-Security-Policy"
        content="default-src 'self'; script-src 'nonce-arch-mermaid'; style-src 'self' 'unsafe-inline'">
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; display: flex; }}
    #sidebar {{ width: 220px; min-height: 100vh; background: #1a1a2e; color: #eee; padding: 1rem; }}
    #sidebar a {{ color: #90caf9; text-decoration: none; }}
    main {{ flex: 1; padding: 2rem; }}
    table {{ border-collapse: collapse; width: 100%; margin-bottom: 1rem; }}
    th, td {{ border: 1px solid #ddd; padding: 0.5rem; text-align: left; }}
    th {{ background: #f4f4f4; }}
    .chip {{ background: #e3f2fd; border-radius: 12px; padding: 2px 10px; margin-right: 4px; font-size: 0.85em; }}
    .badge {{ border-radius: 4px; padding: 1px 6px; font-size: 0.8em; color: #fff; }}
    .badge.uses {{ background: #1976d2; }}
    .badge.implements {{ background: #388e3c; }}
    .badge.contains {{ background: #7b1fa2; }}
    .badge.calls {{ background: #f57c00; }}
    .badge.extends {{ background: #5d4037; }}
    pre.mermaid {{ background: #fafafa; border: 1px solid #eee; padding: 1rem; overflow-x: auto; }}
    section {{ margin-bottom: 2rem; }}
    footer {{ border-top: 1px solid #eee; padding: 1rem; color: #888; font-size: 0.85em; }}
    .stat-chips {{ margin-top: 0.5rem; }}
  </style>
</head>
<body>
{sidebar_html}
<main>
{hero_html}
{sections_html}
{footer_html}
</main>
{mermaid_script}
</body>
</html>
"#
    ))
}

fn mermaid_init_script() -> String {
    // nonce matches the CSP header above.
    r#"<script nonce="arch-mermaid" type="module">
  import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs';
  mermaid.initialize({ startOnLoad: true, securityLevel: 'strict' });
</script>"#
        .into()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::model::{ArchLevel, ArchModel, ArchNode, Language};

    fn model_with_component() -> ArchModel {
        let mut m = ArchModel::new("test");
        m.nodes.push(ArchNode {
            id: "test::Foo".into(),
            label: "Foo".into(),
            level: ArchLevel::Component,
            language: Language::Rust,
            location: Some("src/lib.rs:1".into()),
            tags: vec![],
        });
        m
    }

    #[test]
    fn emits_valid_html5_doctype() {
        let out = emit(&ArchModel::new("test"), None, false).unwrap();
        assert!(out.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn contains_security_level_strict() {
        let out = emit(&model_with_component(), None, false).unwrap();
        assert!(
            out.contains("securityLevel: 'strict'"),
            "Mermaid securityLevel: strict must appear in HTML output"
        );
    }

    #[test]
    fn contains_csp_header() {
        let out = emit(&ArchModel::new("test"), None, false).unwrap();
        assert!(out.contains("Content-Security-Policy"));
    }

    #[test]
    fn contains_sidebar_nav() {
        let out = emit(&ArchModel::new("test"), None, false).unwrap();
        assert!(out.contains("<nav id=\"sidebar\""));
    }

    #[test]
    fn contains_diagrams_section() {
        let out = emit(&model_with_component(), None, false).unwrap();
        assert!(out.contains("id=\"diagrams\""));
    }

    #[test]
    fn skeleton_only_omits_narrative() {
        let seed: NarrativeSeed = toml::from_str(
            r#"[narrative.section_0]
title = "Overview"
body = "UNIQUE_NARRATIVE_MARKER"
"#,
        )
        .unwrap();
        let skeleton = emit(&ArchModel::new("test"), Some(&seed), true).unwrap();
        let full = emit(&ArchModel::new("test"), Some(&seed), false).unwrap();
        assert!(
            !skeleton.contains("UNIQUE_NARRATIVE_MARKER"),
            "skeleton must not contain narrative"
        );
        assert!(
            full.contains("UNIQUE_NARRATIVE_MARKER"),
            "full must contain narrative"
        );
    }

    #[test]
    fn xss_in_node_label_is_encoded() {
        let mut m = ArchModel::new("test");
        m.nodes.push(ArchNode {
            id: "xss".into(),
            label: "<script>alert(1)</script>".into(),
            level: ArchLevel::Component,
            language: Language::Rust,
            location: None,
            tags: vec![],
        });
        let out = emit(&m, None, false).unwrap();
        assert!(
            !out.contains("<script>alert(1)</script>"),
            "XSS in node label must be encoded"
        );
    }
}
