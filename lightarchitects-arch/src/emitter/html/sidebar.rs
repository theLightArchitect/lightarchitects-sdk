//! HTML sidebar — navigation panel listing all sections.

use crate::{
    model::ArchModel,
    security::encode::{EncodeContext, encode},
};

/// Renders the sidebar `<nav>` element.
///
/// All model-derived text is HTML-encoded via [`encode`].
///
/// # Errors
///
/// Returns [`super::super::EmitError`] if encoding fails.
pub fn render(model: &ArchModel) -> Result<String, super::super::EmitError> {
    let mut out = String::with_capacity(512);
    out.push_str("<nav id=\"sidebar\" aria-label=\"Sections\">\n");
    out.push_str("  <ul>\n");
    out.push_str("    <li><a href=\"#overview\">Overview</a></li>\n");
    out.push_str("    <li><a href=\"#nodes\">Components</a></li>\n");
    out.push_str("    <li><a href=\"#relations\">Relations</a></li>\n");
    out.push_str("    <li><a href=\"#diagrams\">Diagrams</a></li>\n");
    out.push_str("    <li><a href=\"#glossary\">Glossary</a></li>\n");

    for node in model
        .nodes
        .iter()
        .filter(|n| n.level == crate::model::ArchLevel::Component)
        .take(10)
    {
        let label = encode(&node.label, EncodeContext::HtmlText).map_err(|e| {
            super::super::EmitError::Encode {
                field: "node.label".into(),
                reason: e.to_string(),
            }
        })?;
        let id = encode(&node.id, EncodeContext::HtmlAttr).map_err(|e| {
            super::super::EmitError::Encode {
                field: "node.id".into(),
                reason: e.to_string(),
            }
        })?;
        out.push_str(&format!(
            "    <li><a href=\"#node-{id}\">{label}</a></li>\n"
        ));
    }

    out.push_str("  </ul>\n");
    out.push_str("</nav>\n");
    Ok(out)
}
