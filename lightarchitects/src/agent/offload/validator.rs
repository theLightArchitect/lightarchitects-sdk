//! `ShapeValidator` — mechanical predicate checker for LLM outputs against
//! the catalog-declared [`super::catalog::Shape`].
//!
//! Stateless dispatcher; first-failure exit. Mechanical only (LÆX handles
//! semantic verification — Day 6).

use serde_json::Value;

use super::catalog::Shape;

/// Stateless predicate dispatcher.
pub struct ShapeValidator;

/// Failure modes returned by [`ShapeValidator::validate`].
#[derive(Debug, Clone, thiserror::Error)]
pub enum ShapeViolation {
    /// Output contains a triple-backtick markdown fence.
    #[error("output contains forbidden markdown fence")]
    MarkdownFenceForbidden,
    /// Word count exceeds `shape.max_words`.
    #[error("word count {actual} exceeds max {max}")]
    WordCountExceeded {
        /// Maximum allowed.
        max: u32,
        /// Actual count.
        actual: usize,
    },
    /// Output contains a non-fence forbidden substring.
    #[error("output contains forbidden substring {0:?}")]
    ForbiddenSubstring(String),
    /// Output failed to parse as JSON, or top-level was not an object.
    #[error("output is not a valid JSON object: {0}")]
    NotJsonObject(String),
    /// A `required_keys` entry was absent.
    #[error("required JSON key {0:?} missing")]
    RequiredJsonKeyMissing(String),
    /// The `verdict` value was not in `shape.verdict_enum`.
    #[error("verdict {got:?} not in allowed enum {allowed:?}")]
    VerdictNotInEnum {
        /// Verdict observed.
        got: String,
        /// Allowed values.
        allowed: Vec<String>,
    },
    /// Output did not begin with the caller-supplied anchor string.
    #[error("output does not start with expected anchor {expected:?}")]
    AnchorPrefixMissing {
        /// Expected leading characters.
        expected: String,
    },
    /// `shape.starts_with_anchor == Some(true)` but no anchor was supplied — wiring bug.
    #[error("shape requires starts_with_anchor=true but no anchor was provided")]
    AnchorRequiredButMissing,
    /// `shape.kind` is not recognised by this validator.
    #[error("unknown shape kind {0:?}")]
    UnknownShapeKind(String),
}

impl ShapeValidator {
    /// Apply universal + kind-specific predicates. First violation wins.
    ///
    /// # Errors
    ///
    /// See [`ShapeViolation`] variants.
    pub fn validate(
        output: &str,
        shape: &Shape,
        starts_with_anchor: Option<&str>,
    ) -> Result<(), ShapeViolation> {
        if let Some(forbidden) = &shape.forbidden_substrings {
            for sub in forbidden {
                if output.contains(sub.as_str()) {
                    if sub == "```" {
                        return Err(ShapeViolation::MarkdownFenceForbidden);
                    }
                    return Err(ShapeViolation::ForbiddenSubstring(sub.clone()));
                }
            }
        }

        if let Some(max) = shape.max_words {
            let actual = output.split_whitespace().count();
            if actual > max as usize {
                return Err(ShapeViolation::WordCountExceeded { max, actual });
            }
        }

        if shape.starts_with_anchor == Some(true) {
            let Some(anchor) = starts_with_anchor else {
                return Err(ShapeViolation::AnchorRequiredButMissing);
            };
            if !output.trim_start().starts_with(anchor) {
                return Err(ShapeViolation::AnchorPrefixMissing {
                    expected: anchor.to_owned(),
                });
            }
        }

        match shape.kind.as_str() {
            "json_object" => validate_json_object(output, shape)?,
            "sentence_no_fences" | "enumeration_lines" | "function_no_fences"
            | "markdown_section" => {}
            other => return Err(ShapeViolation::UnknownShapeKind(other.to_owned())),
        }
        Ok(())
    }
}

fn validate_json_object(output: &str, shape: &Shape) -> Result<(), ShapeViolation> {
    let trimmed = output.trim();
    let value: Value =
        serde_json::from_str(trimmed).map_err(|e| ShapeViolation::NotJsonObject(e.to_string()))?;
    let obj = value
        .as_object()
        .ok_or_else(|| ShapeViolation::NotJsonObject("top-level not an object".to_owned()))?;
    if let Some(keys) = &shape.required_keys {
        for key in keys {
            if !obj.contains_key(key) {
                return Err(ShapeViolation::RequiredJsonKeyMissing(key.clone()));
            }
        }
    }
    if let Some(enum_vals) = &shape.verdict_enum {
        let verdict = obj
            .get("verdict")
            .and_then(Value::as_str)
            .ok_or_else(|| ShapeViolation::RequiredJsonKeyMissing("verdict".to_owned()))?;
        if !enum_vals.iter().any(|v| v == verdict) {
            return Err(ShapeViolation::VerdictNotInEnum {
                got: verdict.to_owned(),
                allowed: enum_vals.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn shape_sentence_no_fences() -> Shape {
        Shape {
            kind: "sentence_no_fences".to_owned(),
            max_words: Some(50),
            forbidden_substrings: Some(vec!["```".to_owned()]),
            required_keys: None,
            verdict_enum: None,
            starts_with_anchor: None,
        }
    }

    fn shape_function_no_fences() -> Shape {
        Shape {
            kind: "function_no_fences".to_owned(),
            max_words: None,
            forbidden_substrings: Some(vec!["```".to_owned()]),
            required_keys: None,
            verdict_enum: None,
            starts_with_anchor: Some(true),
        }
    }

    fn shape_json_p4() -> Shape {
        Shape {
            kind: "json_object".to_owned(),
            max_words: None,
            forbidden_substrings: Some(vec!["```".to_owned()]),
            required_keys: Some(vec![
                "p50".to_owned(),
                "p95".to_owned(),
                "p99".to_owned(),
                "mean".to_owned(),
                "min".to_owned(),
                "max".to_owned(),
            ]),
            verdict_enum: None,
            starts_with_anchor: None,
        }
    }

    fn shape_json_verifier() -> Shape {
        Shape {
            kind: "json_object".to_owned(),
            max_words: None,
            forbidden_substrings: Some(vec!["```".to_owned()]),
            required_keys: Some(vec![
                "verdict".to_owned(),
                "reason".to_owned(),
                "amendment_hint".to_owned(),
            ]),
            verdict_enum: Some(vec![
                "PASS".to_owned(),
                "RETRY".to_owned(),
                "HITL".to_owned(),
            ]),
            starts_with_anchor: None,
        }
    }

    fn shape_markdown() -> Shape {
        Shape {
            kind: "markdown_section".to_owned(),
            max_words: Some(200),
            forbidden_substrings: Some(vec!["<commentary>".to_owned()]),
            required_keys: None,
            verdict_enum: None,
            starts_with_anchor: None,
        }
    }

    #[test]
    fn accepts_valid_sentence_p1() {
        let s = shape_sentence_no_fences();
        assert!(ShapeValidator::validate("Returns n clamped to [lo, hi].", &s, None).is_ok());
    }

    #[test]
    fn rejects_markdown_fence() {
        let s = shape_sentence_no_fences();
        let r = ShapeValidator::validate("```js\nreturn 1\n```", &s, None);
        assert!(matches!(r, Err(ShapeViolation::MarkdownFenceForbidden)));
    }

    #[test]
    fn rejects_word_count_over_max() {
        let s = shape_sentence_no_fences();
        let long = "word ".repeat(60);
        let r = ShapeValidator::validate(&long, &s, None);
        match r {
            Err(ShapeViolation::WordCountExceeded { max: 50, actual }) => {
                assert_eq!(actual, 60);
            }
            other => panic!("expected WordCountExceeded, got {other:?}"),
        }
    }

    #[test]
    fn rejects_custom_forbidden_substring() {
        let s = shape_markdown();
        let r = ShapeValidator::validate("## S\n<commentary>oops</commentary>", &s, None);
        match r {
            Err(ShapeViolation::ForbiddenSubstring(sub)) => assert_eq!(sub, "<commentary>"),
            other => panic!("expected ForbiddenSubstring, got {other:?}"),
        }
    }

    #[test]
    fn accepts_valid_json_with_all_required_keys() {
        let s = shape_json_p4();
        let body = r#"{"p50":1,"p95":2,"p99":3,"mean":1.5,"min":0.5,"max":3.0}"#;
        assert!(ShapeValidator::validate(body, &s, None).is_ok());
    }

    #[test]
    fn rejects_malformed_json() {
        let s = shape_json_p4();
        let r = ShapeValidator::validate("not json {{", &s, None);
        assert!(matches!(r, Err(ShapeViolation::NotJsonObject(_))));
    }

    #[test]
    fn rejects_json_array() {
        let s = shape_json_p4();
        let r = ShapeValidator::validate("[1, 2, 3]", &s, None);
        assert!(matches!(r, Err(ShapeViolation::NotJsonObject(_))));
    }

    #[test]
    fn rejects_missing_required_key() {
        let s = shape_json_p4();
        let body = r#"{"p50":1,"p95":2,"p99":3,"mean":1.5,"min":0.5}"#;
        match ShapeValidator::validate(body, &s, None) {
            Err(ShapeViolation::RequiredJsonKeyMissing(k)) => assert_eq!(k, "max"),
            other => panic!("expected RequiredJsonKeyMissing(max), got {other:?}"),
        }
    }

    #[test]
    fn accepts_valid_verdict_enum() {
        let s = shape_json_verifier();
        let body = r#"{"verdict":"PASS","reason":"ok","amendment_hint":null}"#;
        assert!(ShapeValidator::validate(body, &s, None).is_ok());
    }

    #[test]
    fn rejects_out_of_enum_verdict() {
        let s = shape_json_verifier();
        let body = r#"{"verdict":"MAYBE","reason":"x","amendment_hint":null}"#;
        match ShapeValidator::validate(body, &s, None) {
            Err(ShapeViolation::VerdictNotInEnum { got, allowed }) => {
                assert_eq!(got, "MAYBE");
                assert_eq!(allowed, vec!["PASS", "RETRY", "HITL"]);
            }
            other => panic!("expected VerdictNotInEnum, got {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_anchor_when_required() {
        let s = shape_function_no_fences();
        let r = ShapeValidator::validate("function clamp(n) {}", &s, None);
        assert!(matches!(r, Err(ShapeViolation::AnchorRequiredButMissing)));
    }

    #[test]
    fn accepts_when_anchor_matches_start() {
        let s = shape_function_no_fences();
        let r = ShapeValidator::validate(
            "function clamp(n, lo, hi) { return Math.min(hi, Math.max(lo, n)); }",
            &s,
            Some("function clamp("),
        );
        assert!(r.is_ok(), "expected Ok, got {r:?}");
    }

    #[test]
    fn rejects_when_anchor_does_not_match_start() {
        let s = shape_function_no_fences();
        let r = ShapeValidator::validate(
            "const clamp = (n, lo, hi) => n",
            &s,
            Some("function clamp("),
        );
        match r {
            Err(ShapeViolation::AnchorPrefixMissing { expected }) => {
                assert_eq!(expected, "function clamp(");
            }
            other => panic!("expected AnchorPrefixMissing, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_shape_kind() {
        let s = Shape {
            kind: "bogus_kind".to_owned(),
            max_words: None,
            forbidden_substrings: None,
            required_keys: None,
            verdict_enum: None,
            starts_with_anchor: None,
        };
        match ShapeValidator::validate("hi", &s, None) {
            Err(ShapeViolation::UnknownShapeKind(k)) => assert_eq!(k, "bogus_kind"),
            other => panic!("expected UnknownShapeKind, got {other:?}"),
        }
    }

    #[test]
    fn fence_check_runs_before_json_parse() {
        let s = shape_json_p4();
        let body = "```\n{\"p50\":1}\n```";
        assert!(matches!(
            ShapeValidator::validate(body, &s, None),
            Err(ShapeViolation::MarkdownFenceForbidden)
        ));
    }

    #[test]
    fn accepts_p5_markdown_under_word_budget() {
        let s = shape_markdown();
        assert!(ShapeValidator::validate("## Title\nA short readme section.", &s, None).is_ok());
    }
}
