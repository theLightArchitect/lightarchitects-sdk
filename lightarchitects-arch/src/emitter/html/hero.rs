//! HTML hero — top banner with title, project summary, and stat chips.

use crate::{
    model::ArchModel,
    narrative::NarrativeSeed,
    security::encode::{EncodeContext, encode},
};

/// Renders the hero `<header>` element.
///
/// # Errors
///
/// Returns [`super::super::EmitError`] if encoding fails.
pub fn render(
    model: &ArchModel,
    seed: Option<&NarrativeSeed>,
) -> Result<String, super::super::EmitError> {
    let title = seed
        .and_then(|s| s.meta.title.as_deref())
        .unwrap_or("Architecture Documentation");
    let safe_title =
        encode(title, EncodeContext::HtmlText).map_err(|e| super::super::EmitError::Encode {
            field: "meta.title".into(),
            reason: e.to_string(),
        })?;

    let project = seed
        .and_then(|s| s.meta.project.as_deref())
        .unwrap_or("unknown");
    let safe_project =
        encode(project, EncodeContext::HtmlText).map_err(|e| super::super::EmitError::Encode {
            field: "meta.project".into(),
            reason: e.to_string(),
        })?;

    let node_count = model.nodes.len();
    let rel_count = model.relations.len();
    let component_count = model
        .nodes
        .iter()
        .filter(|n| n.level == crate::model::ArchLevel::Component)
        .count();

    Ok(format!(
        r#"<header id="hero">
  <h1>{safe_title}</h1>
  <p class="project-label">Project: <code>{safe_project}</code></p>
  <div class="stat-chips">
    <span class="chip">{node_count} nodes</span>
    <span class="chip">{component_count} components</span>
    <span class="chip">{rel_count} relations</span>
  </div>
</header>
"#
    ))
}
