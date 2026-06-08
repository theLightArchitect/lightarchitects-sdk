//! `PromptRefiner` — builds the **single retry prompt** the offload provider
//! re-dispatches to `lightsquad_dispatch_task` after a shape-validation or
//! LÆX-verifier failure.
//!
//! Two failure sources, two refinement paths:
//!
//! 1. **Shape failure** ([`PromptRefiner::refine_after_shape_failure`]) —
//!    the validator caught a mechanical violation (markdown fence, missing
//!    JSON key, anchor drift, etc.). The retry prompt prepends the catalog's
//!    `refinement.anchor` directive + a one-line description of the violation.
//!
//! 2. **LÆX RETRY verdict** ([`PromptRefiner::refine_after_laex_retry`]) —
//!    the supervisor returned `{"verdict":"RETRY","amendment_hint":"..."}`.
//!    The retry prompt prepends `refinement.anchor` + the LÆX hint when
//!    available, fall through to anchor-only otherwise.
//!
//! Bounded retry semantics live in [`super::laex_supervisor`] (Day 6) — this
//! module is the pure-function string builder.

use super::catalog::Refinement;
use super::validator::ShapeViolation;

/// Stateless prompt builder. Unit struct for discoverability.
pub struct PromptRefiner;

impl PromptRefiner {
    /// Compose a retry prompt after a shape-validation failure.
    ///
    /// Format:
    ///
    /// ```text
    /// {refinement.anchor}
    ///
    /// Prior attempt failed validation: {violation}
    ///
    /// Original request:
    /// {original_prompt}
    /// ```
    #[must_use]
    pub fn refine_after_shape_failure(
        original_prompt: &str,
        refinement: &Refinement,
        violation: &ShapeViolation,
    ) -> String {
        format!(
            "{anchor}\n\nPrior attempt failed validation: {violation}\n\nOriginal request:\n{original_prompt}",
            anchor = refinement.anchor,
            violation = violation,
        )
    }

    /// Compose a retry prompt after a LÆX `RETRY` verdict.
    ///
    /// When `amendment_hint` is `Some`, includes the verifier's guidance.
    /// When `None` (verifier returned RETRY without a hint), the prompt
    /// falls back to anchor-only refinement.
    #[must_use]
    pub fn refine_after_laex_retry(
        original_prompt: &str,
        refinement: &Refinement,
        amendment_hint: Option<&str>,
    ) -> String {
        match amendment_hint {
            Some(hint) if !hint.trim().is_empty() => format!(
                "{anchor}\n\nLÆX-verifier feedback: {hint}\n\nOriginal request:\n{original_prompt}",
                anchor = refinement.anchor,
                hint = hint.trim(),
            ),
            _ => format!(
                "{anchor}\n\nOriginal request:\n{original_prompt}",
                anchor = refinement.anchor,
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn refinement_p3() -> Refinement {
        Refinement {
            anchor: "RESPOND starting with the exact characters `function clamp(`. NO backticks."
                .to_owned(),
        }
    }

    #[test]
    fn shape_failure_includes_anchor_violation_and_original() {
        let r = refinement_p3();
        let v = ShapeViolation::MarkdownFenceForbidden;
        let prompt = PromptRefiner::refine_after_shape_failure(
            "Write a JavaScript function clamp(n, lo, hi).",
            &r,
            &v,
        );
        assert!(prompt.starts_with("RESPOND starting"));
        assert!(prompt.contains("forbidden markdown fence"));
        assert!(prompt.contains("Original request"));
        assert!(prompt.contains("Write a JavaScript function clamp"));
    }

    #[test]
    fn shape_failure_word_count_violation_message_renders() {
        let r = refinement_p3();
        let v = ShapeViolation::WordCountExceeded {
            max: 50,
            actual: 73,
        };
        let prompt = PromptRefiner::refine_after_shape_failure("original", &r, &v);
        assert!(prompt.contains("73"));
        assert!(prompt.contains("50"));
    }

    #[test]
    fn laex_retry_with_hint_includes_hint() {
        let r = refinement_p3();
        let prompt = PromptRefiner::refine_after_laex_retry(
            "original prompt",
            &r,
            Some("Function must check for NaN before clamping."),
        );
        assert!(prompt.starts_with("RESPOND starting"));
        assert!(prompt.contains("LÆX-verifier feedback"));
        assert!(prompt.contains("check for NaN"));
        assert!(prompt.contains("Original request"));
        assert!(prompt.contains("original prompt"));
    }

    #[test]
    fn laex_retry_without_hint_omits_hint_section() {
        let r = refinement_p3();
        let prompt = PromptRefiner::refine_after_laex_retry("original prompt", &r, None);
        assert!(prompt.starts_with("RESPOND starting"));
        assert!(!prompt.contains("LÆX-verifier feedback"));
        assert!(prompt.contains("Original request"));
        assert!(prompt.contains("original prompt"));
    }

    #[test]
    fn laex_retry_empty_hint_treated_as_none() {
        let r = refinement_p3();
        let prompt = PromptRefiner::refine_after_laex_retry("original prompt", &r, Some("   "));
        assert!(!prompt.contains("LÆX-verifier feedback"));
    }

    #[test]
    fn anchor_directive_appears_first() {
        let r = refinement_p3();
        let v = ShapeViolation::AnchorPrefixMissing {
            expected: "function clamp(".to_owned(),
        };
        let prompt = PromptRefiner::refine_after_shape_failure("orig", &r, &v);
        // The catalog's anchor directive must precede the diagnostic.
        let anchor_pos = prompt.find("RESPOND starting").unwrap();
        let diagnostic_pos = prompt.find("Prior attempt failed").unwrap();
        assert!(anchor_pos < diagnostic_pos);
    }

    #[test]
    fn shape_failure_handles_required_key_violation() {
        let r = refinement_p3();
        let v = ShapeViolation::RequiredJsonKeyMissing("max".to_owned());
        let prompt = PromptRefiner::refine_after_shape_failure("orig", &r, &v);
        assert!(prompt.contains("max"));
        assert!(prompt.contains("missing"));
    }
}
