//! HTML body sections — per-section function dispatch table (E-1 fold).
//!
//! Each section is a named function. The dispatch map in [`render_all`] explicitly
//! lists every section, making additions and deletions visible in diffs.

use crate::{
    model::{ArchLevel, ArchModel, RelationKind},
    narrative::NarrativeSeed,
    security::encode::{EncodeContext, encode},
};

/// Renders all body sections in document order.
///
/// # Errors
///
/// Returns [`super::super::EmitError`] if any section fails encoding.
pub fn render_all(
    model: &ArchModel,
    seed: Option<&NarrativeSeed>,
    skeleton_only: bool,
) -> Result<String, super::super::EmitError> {
    let mut out = String::with_capacity(8192);
    out.push_str(&render_overview(seed, skeleton_only)?);
    out.push_str(&render_nodes(model)?);
    out.push_str(&render_relations(model)?);
    out.push_str(&render_diagrams(model)?);
    out.push_str(&render_glossary(seed, skeleton_only)?);
    Ok(out)
}

fn render_overview(
    seed: Option<&NarrativeSeed>,
    skeleton_only: bool,
) -> Result<String, super::super::EmitError> {
    let mut out = String::with_capacity(512);
    out.push_str("<section id=\"overview\">\n  <h2>Overview</h2>\n");
    if !skeleton_only {
        if let Some(body) = seed.and_then(|s| s.section("section_0")).map(|s| &s.body) {
            let safe = encode(body, EncodeContext::HtmlText).map_err(|e| {
                super::super::EmitError::Encode {
                    field: "section_0.body".into(),
                    reason: e.to_string(),
                }
            })?;
            out.push_str(&format!("  <p>{safe}</p>\n"));
        }
    }
    out.push_str("</section>\n");
    Ok(out)
}

fn render_nodes(model: &ArchModel) -> Result<String, super::super::EmitError> {
    let mut out = String::with_capacity(2048);
    out.push_str("<section id=\"nodes\">\n  <h2>Components</h2>\n");
    out.push_str("  <table>\n    <thead><tr><th>ID</th><th>Label</th><th>Level</th><th>Location</th></tr></thead>\n    <tbody>\n");
    for node in &model.nodes {
        let safe_id = encode(&node.id, EncodeContext::HtmlText).map_err(|e| {
            super::super::EmitError::Encode {
                field: "node.id".into(),
                reason: e.to_string(),
            }
        })?;
        let safe_label = encode(&node.label, EncodeContext::HtmlText).map_err(|e| {
            super::super::EmitError::Encode {
                field: "node.label".into(),
                reason: e.to_string(),
            }
        })?;
        let level = format!("{:?}", node.level);
        let loc = node.location.as_deref().unwrap_or("-");
        let safe_loc =
            encode(loc, EncodeContext::HtmlText).map_err(|e| super::super::EmitError::Encode {
                field: "node.location".into(),
                reason: e.to_string(),
            })?;
        let anchor_id = encode(&node.id, EncodeContext::HtmlAttr).map_err(|e| {
            super::super::EmitError::Encode {
                field: "node.id (attr)".into(),
                reason: e.to_string(),
            }
        })?;
        out.push_str(&format!(
            "      <tr id=\"node-{anchor_id}\"><td><code>{safe_id}</code></td><td>{safe_label}</td><td>{level}</td><td>{safe_loc}</td></tr>\n"
        ));
    }
    out.push_str("    </tbody>\n  </table>\n</section>\n");
    Ok(out)
}

fn render_relations(model: &ArchModel) -> Result<String, super::super::EmitError> {
    let mut out = String::with_capacity(1024);
    out.push_str("<section id=\"relations\">\n  <h2>Relations</h2>\n");
    out.push_str("  <table>\n    <thead><tr><th>From</th><th>Kind</th><th>To</th></tr></thead>\n    <tbody>\n");
    for rel in &model.relations {
        let from = encode(&rel.from, EncodeContext::HtmlText).map_err(|e| {
            super::super::EmitError::Encode {
                field: "rel.from".into(),
                reason: e.to_string(),
            }
        })?;
        let to = encode(&rel.to, EncodeContext::HtmlText).map_err(|e| {
            super::super::EmitError::Encode {
                field: "rel.to".into(),
                reason: e.to_string(),
            }
        })?;
        let kind = relation_badge(rel.kind);
        out.push_str(&format!(
            "      <tr><td><code>{from}</code></td><td>{kind}</td><td><code>{to}</code></td></tr>\n"
        ));
    }
    out.push_str("    </tbody>\n  </table>\n</section>\n");
    Ok(out)
}

/// Renders a Mermaid diagram block for each top-level component.
fn render_diagrams(model: &ArchModel) -> Result<String, super::super::EmitError> {
    let mut out = String::with_capacity(4096);
    out.push_str("<section id=\"diagrams\">\n  <h2>Diagrams</h2>\n");

    // System overview diagram
    let overview_mermaid = crate::emitter::mermaid::emit(model)
        .map_err(|e| super::super::EmitError::Cmd(e.to_string()))?;
    out.push_str("  <figure>\n    <figcaption>System Overview</figcaption>\n");
    out.push_str("    <pre class=\"mermaid\">\n");
    out.push_str(&html_escape_code(&overview_mermaid));
    out.push_str("    </pre>\n  </figure>\n");

    // Per-component sub-diagrams (up to 10).
    for node in model
        .nodes
        .iter()
        .filter(|n| n.level == ArchLevel::Component)
        .take(10)
    {
        let safe_label = encode(&node.label, EncodeContext::HtmlText).map_err(|e| {
            super::super::EmitError::Encode {
                field: "node.label".into(),
                reason: e.to_string(),
            }
        })?;

        // Build a sub-model containing only this node and its direct relations.
        let sub = submodel_for(model, &node.id);
        let sub_mermaid = crate::emitter::mermaid::emit(&sub)
            .map_err(|e| super::super::EmitError::Cmd(e.to_string()))?;

        out.push_str(&format!(
            "  <figure>\n    <figcaption>{safe_label}</figcaption>\n"
        ));
        out.push_str("    <pre class=\"mermaid\">\n");
        out.push_str(&html_escape_code(&sub_mermaid));
        out.push_str("    </pre>\n  </figure>\n");
    }

    out.push_str("</section>\n");
    Ok(out)
}

fn render_glossary(
    seed: Option<&NarrativeSeed>,
    skeleton_only: bool,
) -> Result<String, super::super::EmitError> {
    let mut out = String::with_capacity(512);
    out.push_str("<section id=\"glossary\">\n  <h2>Glossary</h2>\n");
    if !skeleton_only {
        if let Some(seed) = seed {
            out.push_str("  <dl>\n");
            for entry in &seed.glossary {
                let term = encode(&entry.term, EncodeContext::HtmlText).map_err(|e| {
                    super::super::EmitError::Encode {
                        field: "glossary.term".into(),
                        reason: e.to_string(),
                    }
                })?;
                let def = encode(&entry.definition, EncodeContext::HtmlText).map_err(|e| {
                    super::super::EmitError::Encode {
                        field: "glossary.definition".into(),
                        reason: e.to_string(),
                    }
                })?;
                out.push_str(&format!("    <dt>{term}</dt>\n    <dd>{def}</dd>\n"));
            }
            out.push_str("  </dl>\n");
        }
    }
    out.push_str("</section>\n");
    Ok(out)
}

fn relation_badge(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::Uses => "<span class=\"badge uses\">uses</span>",
        RelationKind::Implements => "<span class=\"badge implements\">implements</span>",
        RelationKind::Contains => "<span class=\"badge contains\">contains</span>",
        RelationKind::Calls => "<span class=\"badge calls\">calls</span>",
        RelationKind::Spawns => "<span class=\"badge calls\">spawns</span>",
    }
}

/// Builds a minimal [`ArchModel`] containing `node_id` and its direct neighbours.
fn submodel_for(model: &ArchModel, node_id: &str) -> ArchModel {
    use crate::model::{ArchModel, ArchNode, ArchRelation};
    let mut sub = ArchModel::new("submodel");
    let connected_ids: std::collections::HashSet<&str> = model
        .relations
        .iter()
        .filter(|r| r.from == node_id || r.to == node_id)
        .flat_map(|r| [r.from.as_str(), r.to.as_str()])
        .collect();

    for n in model
        .nodes
        .iter()
        .filter(|n| connected_ids.contains(n.id.as_str()))
    {
        sub.nodes.push(ArchNode {
            id: n.id.clone(),
            label: n.label.clone(),
            level: n.level,
            language: n.language,
            location: n.location.clone(),
            tags: n.tags.clone(),
        });
    }
    for r in model
        .relations
        .iter()
        .filter(|r| r.from == node_id || r.to == node_id)
    {
        sub.relations.push(ArchRelation {
            from: r.from.clone(),
            to: r.to.clone(),
            kind: r.kind,
            label: r.label.clone(),
        });
    }
    sub
}

/// Escapes `<`, `>`, `&` in code blocks — these are already safe Mermaid syntax,
/// but must be entity-escaped inside `<pre>` elements.
fn html_escape_code(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
