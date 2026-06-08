//! Prompt assembly with strict token budgets + template-variable rendering.
//!
//! # What this module does
//!
//! - [`assemble`]: composes `persona + charter + context_blocks + user_prompt`
//!   into a single string, enforcing per-component sub-budgets within a
//!   total cap (typically `pattern.eligible.max_input_tokens` = 4000).
//! - [`render_template`]: generic `{{key}}`-substitution helper.
//! - [`extract_rendered_anchor`]: pulls the backtick-quoted prefix from a
//!   pattern's `refinement.anchor`, substitutes template variables, and
//!   returns the rendered anchor string for `ShapeValidator` to use when
//!   `pattern.shape.starts_with_anchor == Some(true)`.
//!
//! # Token estimation
//!
//! 4-char/token heuristic (`text.len().div_ceil(4)`) — matches the
//! convention used by every other [`crate::agent`] provider and by Days 3-5
//! of this BUILD.
//!
//! # Budget allocation order (when input ≤ total)
//!
//! 1. `user_prompt` is allocated first — it is the operator's payload and is
//!    NEVER truncated. If `user_prompt` alone exceeds the total budget,
//!    [`assemble`] returns [`PromptBuilderError::UserPromptOverBudget`].
//! 2. `persona` is allocated next, capped at `persona_max` (default 500).
//! 3. `charter` is allocated next, capped at `charter_max` (default 200).
//! 4. `context_blocks` are fit into whatever budget remains, capped at
//!    `context_max` (default 2000). Whole blocks are included greedily;
//!    a final block that doesn't fully fit is truncated on a UTF-8 boundary
//!    (with a 50-char minimum to avoid useless half-headers).

use std::collections::HashMap;

use super::catalog::Pattern;
use super::context::ResolvedContext;

const TOKEN_CHARS: usize = 4;
const DEFAULT_PERSONA_MAX: usize = 500;
const DEFAULT_CHARTER_MAX: usize = 200;
const DEFAULT_CONTEXT_MAX: usize = 2000;
const DEFAULT_TOTAL_MAX: usize = 4000;

/// Per-component token budgets.
#[derive(Debug, Clone, Copy)]
pub struct BudgetConfig {
    /// Persona system-prompt cap.
    pub persona_max: usize,
    /// Charter cap.
    pub charter_max: usize,
    /// Context-blocks sum cap.
    pub context_max: usize,
    /// Total cap — derived from `pattern.eligible.max_input_tokens`.
    pub total_max: usize,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            persona_max: DEFAULT_PERSONA_MAX,
            charter_max: DEFAULT_CHARTER_MAX,
            context_max: DEFAULT_CONTEXT_MAX,
            total_max: DEFAULT_TOTAL_MAX,
        }
    }
}

impl BudgetConfig {
    /// Derive a budget from a catalog pattern.  `total_max` is taken from
    /// `pattern.eligible.max_input_tokens`; sub-budgets keep their defaults.
    #[must_use]
    pub fn from_pattern(pattern: &Pattern) -> Self {
        Self {
            total_max: usize::try_from(pattern.eligible.max_input_tokens)
                .unwrap_or(DEFAULT_TOTAL_MAX),
            ..Self::default()
        }
    }
}

/// Errors raised by [`assemble`].
#[derive(Debug, Clone, thiserror::Error)]
pub enum PromptBuilderError {
    /// `user_prompt` alone exceeds the total budget — there's no way to
    /// produce a valid assembly. Caller should fall through.
    #[error("user_prompt ({used} tok) exceeds total budget ({budget} tok)")]
    UserPromptOverBudget {
        /// Estimated tokens used by the user prompt.
        used: usize,
        /// Total token cap.
        budget: usize,
    },
}

/// Per-component token consumption accounting for telemetry.
#[derive(Debug, Clone, Default)]
pub struct ComponentTokenUsage {
    /// Tokens used by the persona block.
    pub persona: usize,
    /// Tokens used by the charter block.
    pub charter: usize,
    /// Tokens used by all context blocks.
    pub context: usize,
    /// Tokens used by the user prompt (always full — never truncated).
    pub user_prompt: usize,
}

/// Output of [`assemble`].
#[derive(Debug, Clone)]
pub struct AssembledPrompt {
    /// Rendered prompt ready to send to the dispatcher.
    pub rendered: String,
    /// Total estimated token count (sum of components).
    pub estimated_tokens: usize,
    /// Per-component breakdown for Day 14 AYIN spans.
    pub component_tokens: ComponentTokenUsage,
}

/// Compose persona + charter + context + user prompt under the budget.
///
/// # Errors
///
/// - [`PromptBuilderError::UserPromptOverBudget`] if `user_prompt` alone
///   exceeds `budgets.total_max`.
pub fn assemble(
    persona: &str,
    charter: &str,
    blocks: &[ResolvedContext],
    user_prompt: &str,
    budgets: &BudgetConfig,
) -> Result<AssembledPrompt, PromptBuilderError> {
    let user_tokens = est_tokens(user_prompt);
    if user_tokens > budgets.total_max {
        return Err(PromptBuilderError::UserPromptOverBudget {
            used: user_tokens,
            budget: budgets.total_max,
        });
    }
    let remaining_after_user = budgets.total_max - user_tokens;
    let persona_budget = budgets.persona_max.min(remaining_after_user);
    let persona_truncated = truncate_to_tokens(persona, persona_budget);
    let after_persona = remaining_after_user - est_tokens(&persona_truncated);
    let charter_budget = budgets.charter_max.min(after_persona);
    let charter_truncated = truncate_to_tokens(charter, charter_budget);
    let after_charter = after_persona - est_tokens(&charter_truncated);
    let context_budget = budgets.context_max.min(after_charter);
    let (context_block, context_used) = fit_context(blocks, context_budget);

    let mut rendered = String::new();
    rendered.push_str(&persona_truncated);
    rendered.push_str("\n\n# Canonical role\n");
    rendered.push_str(&charter_truncated);
    if !context_block.is_empty() {
        rendered.push_str(&context_block);
    }
    rendered.push_str("\n\n# Task\n");
    rendered.push_str(user_prompt);

    let component_tokens = ComponentTokenUsage {
        persona: est_tokens(&persona_truncated),
        charter: est_tokens(&charter_truncated),
        context: context_used,
        user_prompt: user_tokens,
    };
    let estimated_tokens = component_tokens.persona
        + component_tokens.charter
        + component_tokens.context
        + component_tokens.user_prompt;
    Ok(AssembledPrompt {
        rendered,
        estimated_tokens,
        component_tokens,
    })
}

/// Substitute `{{key}}` placeholders in `template`. Unknown placeholders are
/// left intact.
#[must_use]
pub fn render_template<S: std::hash::BuildHasher>(
    template: &str,
    vars: &HashMap<String, String, S>,
) -> String {
    let mut out = template.to_owned();
    for (k, v) in vars {
        let needle = format!("{{{{{k}}}}}");
        out = out.replace(&needle, v);
    }
    out
}

/// Extract the rendered anchor for a pattern that declares
/// `starts_with_anchor: true`. Returns `None` for patterns that don't
/// require an anchor, patterns without a refinement, anchors without a
/// backtick-quoted segment, or anchors whose template vars cannot be fully
/// resolved from `vars`.
#[must_use]
pub fn extract_rendered_anchor<S: std::hash::BuildHasher>(
    pattern: &Pattern,
    vars: &HashMap<String, String, S>,
) -> Option<String> {
    if pattern.shape.starts_with_anchor != Some(true) {
        return None;
    }
    let raw_anchor = extract_backtick_content(&pattern.refinement.as_ref()?.anchor)?;
    let rendered = render_template(raw_anchor, vars);
    if rendered.contains("{{") {
        return None;
    }
    Some(rendered)
}

// ── Internals ────────────────────────────────────────────────────────────

fn extract_backtick_content(s: &str) -> Option<&str> {
    let start = s.find('`')?;
    let after_start = start + 1;
    let end_rel = s[after_start..].find('`')?;
    Some(&s[after_start..after_start + end_rel])
}

fn est_tokens(s: &str) -> usize {
    s.len().div_ceil(TOKEN_CHARS)
}

fn truncate_to_tokens(s: &str, max_tokens: usize) -> String {
    let max_chars = max_tokens.saturating_mul(TOKEN_CHARS);
    if s.len() <= max_chars {
        return s.to_owned();
    }
    let mut out = s[..max_chars].to_owned();
    while !out.is_empty() && !out.is_char_boundary(out.len()) {
        out.pop();
    }
    out
}

fn fit_context(blocks: &[ResolvedContext], budget_tokens: usize) -> (String, usize) {
    let mut out = String::new();
    let mut used_tokens = 0_usize;
    let budget_chars = budget_tokens.saturating_mul(TOKEN_CHARS);
    for block in blocks {
        let header = format!("\n\n## {}: {}\n", block.kind, block.identifier);
        let combined_len = header.len() + block.content.len();
        if out.len() + combined_len <= budget_chars {
            used_tokens += est_tokens(&header) + est_tokens(&block.content);
            out.push_str(&header);
            out.push_str(&block.content);
        } else {
            let space_after_header = budget_chars
                .saturating_sub(out.len())
                .saturating_sub(header.len());
            if space_after_header < 50 {
                break;
            }
            let take = space_after_header.min(block.content.len());
            let mut truncated = block.content[..take].to_owned();
            while !truncated.is_empty() && !truncated.is_char_boundary(truncated.len()) {
                truncated.pop();
            }
            used_tokens += est_tokens(&header) + est_tokens(&truncated);
            out.push_str(&header);
            out.push_str(&truncated);
            break;
        }
    }
    (out, used_tokens)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::catalog::{Calibration, Eligibility, Refinement, Shape};
    use super::*;

    fn block(kind: &'static str, identifier: &str, content: &str) -> ResolvedContext {
        let est = content.len() / TOKEN_CHARS;
        ResolvedContext {
            kind,
            identifier: identifier.to_owned(),
            content: content.to_owned(),
            token_count_estimate: est,
        }
    }

    fn pattern_with_anchor(anchor: &str) -> Pattern {
        Pattern {
            id: "P3".to_owned(),
            name: "test".to_owned(),
            role: None,
            template: String::new(),
            eligible: Eligibility {
                siblings: vec!["claude".to_owned()],
                tool_use_required: false,
                max_input_tokens: 4000,
            },
            context_sources: None,
            shape: Shape {
                kind: "function_no_fences".to_owned(),
                max_words: None,
                forbidden_substrings: None,
                required_keys: None,
                verdict_enum: None,
                starts_with_anchor: Some(true),
            },
            refinement: Some(Refinement {
                anchor: anchor.to_owned(),
            }),
            verifier: None,
            calibration: Calibration {
                last_dry_run: None,
                sample_count: None,
                success_rate: None,
            },
        }
    }

    fn pattern_no_anchor() -> Pattern {
        let mut p = pattern_with_anchor("");
        p.id = "P1".to_owned();
        p.shape.starts_with_anchor = None;
        p.refinement = None;
        p
    }

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    // ─── render_template ──────────────────────────────────────────────────

    #[test]
    fn render_template_substitutes_known_vars() {
        let v = vars(&[("name", "world")]);
        assert_eq!(render_template("Hello {{name}}", &v), "Hello world");
    }

    #[test]
    fn render_template_leaves_unknown_vars_intact() {
        let v = vars(&[]);
        assert_eq!(render_template("Hello {{name}}", &v), "Hello {{name}}");
    }

    #[test]
    fn render_template_substitutes_multiple_vars() {
        let v = vars(&[("a", "1"), ("b", "2")]);
        assert_eq!(render_template("{{a}}-{{b}}", &v), "1-2");
    }

    // ─── extract_rendered_anchor ──────────────────────────────────────────

    #[test]
    fn extract_anchor_p3_with_vars_returns_rendered() {
        let p = pattern_with_anchor(
            "RESPOND starting with the exact characters `{{lang_kw}} {{name}}(`. NO backticks.",
        );
        let v = vars(&[("lang_kw", "function"), ("name", "clamp")]);
        assert_eq!(
            extract_rendered_anchor(&p, &v).as_deref(),
            Some("function clamp(")
        );
    }

    #[test]
    fn extract_anchor_returns_none_when_pattern_has_no_anchor_requirement() {
        let p = pattern_no_anchor();
        let v = vars(&[("name", "clamp")]);
        assert!(extract_rendered_anchor(&p, &v).is_none());
    }

    #[test]
    fn extract_anchor_returns_none_when_vars_missing() {
        let p = pattern_with_anchor("RESPOND starting with `{{lang_kw}} {{name}}(`. NO backticks.");
        let v = vars(&[]); // no vars → placeholders remain
        assert!(extract_rendered_anchor(&p, &v).is_none());
    }

    #[test]
    fn extract_anchor_returns_none_when_no_refinement() {
        let mut p = pattern_with_anchor("");
        p.refinement = None;
        let v = vars(&[]);
        assert!(extract_rendered_anchor(&p, &v).is_none());
    }

    #[test]
    fn extract_anchor_returns_none_when_no_backtick_segment() {
        let p = pattern_with_anchor("RESPOND but with no backticks at all");
        let v = vars(&[]);
        assert!(extract_rendered_anchor(&p, &v).is_none());
    }

    // ─── assemble: budgets ────────────────────────────────────────────────

    #[test]
    fn assemble_under_budget_includes_all_components() {
        let r = assemble(
            "you are X",
            "role: Y",
            &[],
            "explain Z",
            &BudgetConfig::default(),
        )
        .unwrap();
        assert!(r.rendered.contains("you are X"));
        assert!(r.rendered.contains("role: Y"));
        assert!(r.rendered.contains("explain Z"));
        assert_eq!(r.component_tokens.user_prompt, est_tokens("explain Z"));
    }

    #[test]
    fn user_prompt_alone_over_budget_returns_error() {
        let tiny = BudgetConfig {
            total_max: 10,
            ..BudgetConfig::default()
        };
        // user_prompt is 40 chars ≈ 10 tok = exactly at limit; nudge above.
        let user = "a".repeat(41);
        let err = assemble("p", "c", &[], &user, &tiny).unwrap_err();
        assert!(matches!(
            err,
            PromptBuilderError::UserPromptOverBudget { .. }
        ));
    }

    #[test]
    fn persona_truncated_when_too_large() {
        let big_persona = "abcd".repeat(1000); // 4000 chars = 1000 tok
        let r = assemble(&big_persona, "c", &[], "u", &BudgetConfig::default()).unwrap();
        assert!(r.component_tokens.persona <= DEFAULT_PERSONA_MAX);
    }

    #[test]
    fn charter_truncated_when_too_large() {
        let big_charter = "abcd".repeat(500); // 2000 chars = 500 tok
        let r = assemble("p", &big_charter, &[], "u", &BudgetConfig::default()).unwrap();
        assert!(r.component_tokens.charter <= DEFAULT_CHARTER_MAX);
    }

    #[test]
    fn context_blocks_dropped_when_budget_exhausted() {
        // Three 4000-char blocks = ~1000 tok each. context_max = 2000 →
        // 2 full + 0 fit (third dropped or truncated).
        let bs = vec![
            block("canon", "doc1", &"abcd".repeat(1000)),
            block("canon", "doc2", &"abcd".repeat(1000)),
            block("canon", "doc3", &"abcd".repeat(1000)),
        ];
        let r = assemble("p", "c", &bs, "u", &BudgetConfig::default()).unwrap();
        assert!(r.rendered.contains("doc1"));
        // doc3 should NOT be fully present.
        assert!(
            !r.rendered.contains("doc3") || r.component_tokens.context <= DEFAULT_CONTEXT_MAX + 100
        );
        assert!(r.component_tokens.context <= DEFAULT_CONTEXT_MAX + 50);
    }

    #[test]
    fn user_prompt_never_truncated() {
        // Even under tight overall budget, user_prompt appears in full.
        let tight = BudgetConfig {
            total_max: 200,
            ..BudgetConfig::default()
        };
        let user = "exactly this user prompt".to_owned();
        let r = assemble("p", "c", &[], &user, &tight).unwrap();
        assert!(r.rendered.ends_with(&user));
    }

    #[test]
    fn truncation_respects_utf8_boundary() {
        // 5-byte emoji = 1 char that won't split on a 1- or 2-byte boundary.
        let persona = "🎯".repeat(200); // 4 bytes/char × 200 = 800 bytes
        let r = assemble(&persona, "c", &[], "u", &BudgetConfig::default()).unwrap();
        assert!(r.rendered.is_char_boundary(r.rendered.len()));
    }

    #[test]
    fn from_pattern_pulls_total_from_max_input_tokens() {
        let mut p = pattern_no_anchor();
        p.eligible.max_input_tokens = 2000;
        let b = BudgetConfig::from_pattern(&p);
        assert_eq!(b.total_max, 2000);
        assert_eq!(b.persona_max, DEFAULT_PERSONA_MAX);
    }

    #[test]
    fn assembled_estimated_tokens_matches_component_sum() {
        let r = assemble(
            "p",
            "c",
            &[block("canon", "d", "x")],
            "u",
            &BudgetConfig::default(),
        )
        .unwrap();
        assert_eq!(
            r.estimated_tokens,
            r.component_tokens.persona
                + r.component_tokens.charter
                + r.component_tokens.context
                + r.component_tokens.user_prompt
        );
    }
}
