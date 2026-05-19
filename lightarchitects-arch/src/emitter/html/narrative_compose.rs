//! Narrative composition — merges architect-authored seed sections into the skeleton.
//!
//! Span events are emitted at each `pick_section` call (O-3 fold) so AYIN can
//! observe which sections are populated vs skeleton-only.

use crate::{
    narrative::NarrativeSeed,
    security::encode::{EncodeContext, encode},
};

/// Renders a named narrative section from `seed`, HTML-encoding all text (S-4 fold).
///
/// Returns `None` if the section is not present in the seed.
///
/// # Errors
///
/// Returns [`super::super::EmitError`] if encoding fails.
#[tracing::instrument(skip(seed), fields(section_id = %section_id))]
pub fn pick_section(
    section_id: &str,
    seed: Option<&NarrativeSeed>,
) -> Result<Option<String>, super::super::EmitError> {
    let Some(seed) = seed else {
        tracing::trace!(section_id, populated = false, "no seed");
        return Ok(None);
    };
    let Some(section) = seed.section(section_id) else {
        tracing::trace!(section_id, populated = false, "section absent in seed");
        return Ok(None);
    };

    tracing::trace!(section_id, populated = true, "section found");

    let safe_title = encode(&section.title, EncodeContext::HtmlText).map_err(|e| {
        super::super::EmitError::Encode {
            field: format!("{section_id}.title"),
            reason: e.to_string(),
        }
    })?;
    // Body is untrusted: HTML-encode the raw text (S-4).
    let safe_body = encode(&section.body, EncodeContext::HtmlText).map_err(|e| {
        super::super::EmitError::Encode {
            field: format!("{section_id}.body"),
            reason: e.to_string(),
        }
    })?;

    Ok(Some(format!(
        "<section class=\"narrative\" id=\"{section_id}\">\n  <h3>{safe_title}</h3>\n  <p>{safe_body}</p>\n</section>\n"
    )))
}

/// Renders all narrative sections from `seed` in key-sorted order.
///
/// # Errors
///
/// Returns [`super::super::EmitError`] if any section fails encoding.
pub fn render_all_sections(seed: &NarrativeSeed) -> Result<String, super::super::EmitError> {
    let mut out = String::new();
    let mut keys: Vec<&str> = seed.narrative.keys().map(String::as_str).collect();
    keys.sort_unstable();
    for key in keys {
        if let Some(html) = pick_section(key, Some(seed))? {
            out.push_str(&html);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn seed_with_xss() -> NarrativeSeed {
        toml::from_str(
            r#"[narrative.section_0]
title = "Test <script>alert(1)</script>"
body = "Body <img src=x onerror=alert(1)>"
"#,
        )
        .unwrap()
    }

    #[test]
    fn pick_section_returns_none_without_seed() {
        let result = pick_section("section_0", None).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn pick_section_returns_none_for_missing_key() {
        let seed: NarrativeSeed = toml::from_str("").unwrap();
        let result = pick_section("section_0", Some(&seed)).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn pick_section_encodes_xss_in_title() {
        let seed = seed_with_xss();
        let html = pick_section("section_0", Some(&seed)).unwrap().unwrap();
        assert!(!html.contains("<script>"), "XSS in title must be encoded");
        assert!(html.contains("&lt;script&gt;") || html.contains("Test "));
    }

    #[test]
    fn pick_section_encodes_xss_in_body() {
        let seed = seed_with_xss();
        let html = pick_section("section_0", Some(&seed)).unwrap().unwrap();
        // HtmlText encoding converts '<' → '&lt;' breaking tag execution.
        // The attribute name text may still appear but the tag cannot fire.
        assert!(!html.contains("<img"), "raw img tag must not appear");
        assert!(
            html.contains("&lt;img"),
            "img tag must be entity-encoded in output"
        );
    }
}
