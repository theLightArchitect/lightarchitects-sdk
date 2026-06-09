//! `LaexSupervisor` — runs the catalog-declared verifier pattern as a second
//! offload to vet the primary output against canon + industry-baseline
//! excerpts, and drives bounded auto-retry on RETRY verdicts.
//!
//! # External contract
//!
//! Callers only ever see [`SupervisorVerdict::Pass`] (output approved) or
//! [`SupervisorVerdict::Hitl`] (escalation required). `RETRY` is an
//! **internal** loop state — the supervisor either retries within the
//! configured `max_auto_retries` budget or escalates to `Hitl` when the
//! budget is exhausted.
//!
//! # Loop algorithm
//!
//! 1. If the primary pattern has no verifier configured (or
//!    `verifier.enabled == false`) → immediate `Pass`.
//! 2. Resolve the verifier pattern from the catalog (must exist with
//!    `role: "verifier"`); on missing → [`SupervisorError::VerifierNotInCatalog`].
//! 3. Build the verifier prompt by substituting `{{primary_output}}`,
//!    `{{canon_excerpts}}`, `{{baseline_excerpts}}` into
//!    `verifier_pattern.template`.
//! 4. Dispatch a second offload call via [`OffloadDispatcher`].
//! 5. Validate the verifier's response shape via [`ShapeValidator`]; on
//!    shape failure → [`SupervisorError::InvalidVerifierOutput`].
//! 6. Parse the JSON verdict `{verdict, reason, amendment_hint}`.
//! 7. Act on the verdict:
//!    - `PASS` → return `Pass { output: primary_output }`.
//!    - `HITL` → return `Hitl { reason, last_output, last_amendment_hint }`.
//!    - `RETRY` and budget remaining → refine via
//!      [`PromptRefiner::refine_after_laex_retry`], re-dispatch the primary
//!      pattern, re-validate primary shape, GOTO 4 with incremented counter.
//!    - `RETRY` and budget exhausted → escalate to `Hitl`.
//!
//! # Day 7 hand-off
//!
//! `SupervisorVerdict::Hitl` is the input to the `hitl_bridge` (Day 7) which
//! emits an `IronclawHitlEscalation` event over the existing supervisor
//! channel. The supervisor itself does NOT touch the bridge — separation of
//! concerns; this module is pure verdict-production.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;

use super::catalog::{OffloadCatalog, Pattern};
use super::refiner::PromptRefiner;
use super::validator::{ShapeValidator, ShapeViolation};

/// External verdict surface — callers see only these two outcomes.
#[derive(Debug, Clone)]
pub enum SupervisorVerdict {
    /// Output approved — return verbatim to the LLM consumer.
    Pass {
        /// The primary offload output (possibly after one or more retries).
        output: String,
    },
    /// Output rejected — escalate via [`super::hitl_bridge`] (Day 7).
    Hitl {
        /// Verifier-reported reason (≤30 words per catalog spec).
        reason: String,
        /// Last primary output produced before escalation.
        last_output: String,
        /// Last amendment hint produced by the verifier (if any).
        last_amendment_hint: Option<String>,
    },
}

/// Errors raised by [`LaexSupervisor::supervise`].
#[derive(Debug, Clone, thiserror::Error)]
pub enum SupervisorError {
    /// Primary pattern's `verifier.pattern` references an id that's missing
    /// from the catalog. Should be caught at catalog load time by
    /// [`OffloadCatalog::validate`]; if it surfaces here, the catalog
    /// invariant was bypassed (e.g. hot-reload mid-flight).
    #[error("verifier pattern {0:?} not declared in catalog")]
    VerifierNotInCatalog(String),
    /// Verifier pattern was found but its `role` field is not `"verifier"`.
    #[error("pattern {0:?} is not a verifier (role mismatch)")]
    NotAVerifierRole(String),
    /// Primary or verifier dispatch failed at the wire level.
    #[error("dispatch failed for pattern {pattern_id:?}: {reason}")]
    Dispatch {
        /// Pattern id that was being dispatched.
        pattern_id: String,
        /// Underlying error message.
        reason: String,
    },
    /// Verifier output did not satisfy its declared shape.
    #[error("verifier produced shape-invalid output: {0}")]
    InvalidVerifierOutput(String),
    /// Verifier returned a `verdict` value outside the allowed enum.
    /// In practice this is caught by [`ShapeValidator`] before parsing,
    /// but the parser keeps the guard as defence-in-depth.
    #[error("verifier returned unknown verdict {0:?}")]
    UnknownVerdict(String),
}

/// Excerpt bundle threaded into the verifier prompt.
#[derive(Debug, Clone, Default)]
pub struct VerifierContext {
    /// Canon excerpts concatenated for `{{canon_excerpts}}`.
    pub canon_excerpts: String,
    /// Industry-baseline excerpts concatenated for `{{baseline_excerpts}}`.
    pub baseline_excerpts: String,
}

/// Single-shot offload dispatcher. Implementors include the production
/// `lightsquad_dispatch_task` wrapper (Day 9-10) and the unit-test mock.
#[async_trait]
pub trait OffloadDispatcher: Send + Sync {
    /// Run one offload call. The pattern is for accounting/observability;
    /// the rendered prompt is what actually goes to the model.
    ///
    /// # Errors
    ///
    /// Returns a free-form description of the wire-level failure.
    async fn dispatch(&self, pattern: &Pattern, rendered_prompt: &str) -> Result<String, String>;
}

/// JSON verdict shape returned by the verifier pattern.
#[derive(Debug, Deserialize)]
struct ParsedVerdict {
    verdict: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    amendment_hint: Option<String>,
}

/// Catalog-aware verifier dispatcher.
pub struct LaexSupervisor {
    catalog: Arc<OffloadCatalog>,
    dispatcher: Arc<dyn OffloadDispatcher>,
}

impl LaexSupervisor {
    /// Construct.
    #[must_use]
    pub fn new(catalog: Arc<OffloadCatalog>, dispatcher: Arc<dyn OffloadDispatcher>) -> Self {
        Self {
            catalog,
            dispatcher,
        }
    }

    /// Run the supervisor loop.
    ///
    /// `primary_output` is the LLM's first attempt at `primary_pattern`.
    /// `primary_prompt` is the rendered prompt that produced it — used to
    /// build the RETRY refinement.
    ///
    /// # Errors
    ///
    /// See [`SupervisorError`].
    pub async fn supervise(
        &self,
        primary_pattern: &Pattern,
        primary_output: String,
        primary_prompt: &str,
        context: &VerifierContext,
    ) -> Result<SupervisorVerdict, SupervisorError> {
        let Some(verifier_cfg) = primary_pattern.verifier.as_ref() else {
            return Ok(SupervisorVerdict::Pass {
                output: primary_output,
            });
        };
        if !verifier_cfg.enabled {
            return Ok(SupervisorVerdict::Pass {
                output: primary_output,
            });
        }
        let Some(verifier_id) = verifier_cfg.pattern.as_deref() else {
            return Ok(SupervisorVerdict::Pass {
                output: primary_output,
            });
        };
        let verifier_pattern = self
            .catalog
            .get(verifier_id)
            .ok_or_else(|| SupervisorError::VerifierNotInCatalog(verifier_id.to_owned()))?;
        if verifier_pattern.role.as_deref() != Some("verifier") {
            return Err(SupervisorError::NotAVerifierRole(verifier_id.to_owned()));
        }

        let max_auto_retries = verifier_cfg.max_auto_retries;
        let mut current_output = primary_output;
        let mut current_prompt = primary_prompt.to_owned();
        let mut retries_used: u8 = 0;

        loop {
            let parsed = self
                .run_verifier(verifier_pattern, &current_output, context)
                .await?;
            match parsed.verdict.as_str() {
                "PASS" => {
                    return Ok(SupervisorVerdict::Pass {
                        output: current_output,
                    });
                }
                "HITL" => {
                    return Ok(SupervisorVerdict::Hitl {
                        reason: parsed.reason,
                        last_output: current_output,
                        last_amendment_hint: parsed.amendment_hint,
                    });
                }
                "RETRY" => {
                    if retries_used >= max_auto_retries {
                        return Ok(SupervisorVerdict::Hitl {
                            reason: format!(
                                "max_auto_retries={max_auto_retries} exhausted; last: {}",
                                parsed.reason
                            ),
                            last_output: current_output,
                            last_amendment_hint: parsed.amendment_hint,
                        });
                    }
                    let refinement = primary_pattern.refinement.as_ref().ok_or_else(|| {
                        SupervisorError::InvalidVerifierOutput(
                            "RETRY verdict but primary pattern has no refinement.anchor".to_owned(),
                        )
                    })?;
                    // S2: cap amendment_hint before injecting into the retry
                    // prompt — model output is untrusted (OWASP LLM05).
                    let capped_hint = cap_amendment_hint(parsed.amendment_hint.as_deref());
                    let refined = PromptRefiner::refine_after_laex_retry(
                        &current_prompt,
                        refinement,
                        capped_hint.as_deref(),
                    );
                    let new_output = self
                        .dispatcher
                        .dispatch(primary_pattern, &refined)
                        .await
                        .map_err(|reason| SupervisorError::Dispatch {
                            pattern_id: primary_pattern.id.clone(),
                            reason,
                        })?;
                    // Validate the retried primary shape; if it still fails,
                    // escalate immediately — no second-level retry.
                    if let Err(viol) =
                        ShapeValidator::validate(&new_output, &primary_pattern.shape, None)
                    {
                        return Ok(SupervisorVerdict::Hitl {
                            reason: format!("retry produced shape-invalid output: {viol}"),
                            last_output: new_output,
                            last_amendment_hint: parsed.amendment_hint,
                        });
                    }
                    current_prompt = refined;
                    current_output = new_output;
                    retries_used = retries_used.saturating_add(1);
                }
                other => return Err(SupervisorError::UnknownVerdict(other.to_owned())),
            }
        }
    }

    async fn run_verifier(
        &self,
        verifier_pattern: &Pattern,
        primary_output: &str,
        context: &VerifierContext,
    ) -> Result<ParsedVerdict, SupervisorError> {
        let prompt = render_verifier_template(
            &verifier_pattern.template,
            primary_output,
            &context.canon_excerpts,
            &context.baseline_excerpts,
        );
        let raw = self
            .dispatcher
            .dispatch(verifier_pattern, &prompt)
            .await
            .map_err(|reason| SupervisorError::Dispatch {
                pattern_id: verifier_pattern.id.clone(),
                reason,
            })?;
        // Shape gate first (catches markdown fence around JSON, missing keys,
        // out-of-enum verdict).
        ShapeValidator::validate(&raw, &verifier_pattern.shape, None).map_err(
            |viol: ShapeViolation| SupervisorError::InvalidVerifierOutput(viol.to_string()),
        )?;
        let parsed: ParsedVerdict = serde_json::from_str(raw.trim())
            .map_err(|e| SupervisorError::InvalidVerifierOutput(e.to_string()))?;
        // Defence in depth — shape gate already restricts the enum, but a
        // catalog without verdict_enum would let unknowns through here.
        match parsed.verdict.as_str() {
            "PASS" | "RETRY" | "HITL" => Ok(parsed),
            other => Err(SupervisorError::UnknownVerdict(other.to_owned())),
        }
    }
}

/// Truncate an `amendment_hint` from model output to ≤50 words before it is
/// injected into a retry prompt (S2 — indirect prompt injection mitigation).
fn cap_amendment_hint(hint: Option<&str>) -> Option<String> {
    let h = hint?;
    let words: Vec<&str> = h.split_whitespace().collect();
    if words.len() <= 50 {
        Some(h.to_owned())
    } else {
        Some(words[..50].join(" "))
    }
}

/// Replace the three known tokens in a verifier-pattern template.
fn render_verifier_template(
    template: &str,
    primary_output: &str,
    canon_excerpts: &str,
    baseline_excerpts: &str,
) -> String {
    template
        .replace("{{primary_output}}", primary_output)
        .replace("{{canon_excerpts}}", canon_excerpts)
        .replace("{{baseline_excerpts}}", baseline_excerpts)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::match_wildcard_for_single_variants
)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::super::catalog::{Calibration, Eligibility, Refinement, Shape, Verifier};
    use super::*;

    /// FIFO mock — pops responses in order; records every call.
    struct MockDispatcher {
        responses: Mutex<VecDeque<Result<String, String>>>,
        calls: Mutex<Vec<(String, String)>>,
    }

    impl MockDispatcher {
        fn new(responses: Vec<Result<String, String>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                calls: Mutex::new(Vec::new()),
            }
        }
        fn calls(&self) -> Vec<(String, String)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl OffloadDispatcher for MockDispatcher {
        async fn dispatch(
            &self,
            pattern: &Pattern,
            rendered_prompt: &str,
        ) -> Result<String, String> {
            self.calls
                .lock()
                .unwrap()
                .push((pattern.id.clone(), rendered_prompt.to_owned()));
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err("mock: no more responses".to_owned()))
        }
    }

    fn primary_pattern_with_verifier(verifier_id: &str, max_auto_retries: u8) -> Pattern {
        Pattern {
            id: "P3".to_owned(),
            name: "Generate fn".to_owned(),
            role: None,
            template: "Write a function".to_owned(),
            eligible: Eligibility {
                siblings: vec!["claude".to_owned()],
                tool_use_required: false,
                max_input_tokens: 4000,
            },
            context_sources: None,
            shape: Shape {
                kind: "function_no_fences".to_owned(),
                max_words: None,
                forbidden_substrings: Some(vec!["```".to_owned()]),
                required_keys: None,
                verdict_enum: None,
                starts_with_anchor: None,
            },
            refinement: Some(Refinement {
                anchor: "NO BACKTICKS.".to_owned(),
            }),
            verifier: Some(Verifier {
                enabled: true,
                pattern: Some(verifier_id.to_owned()),
                escalate_on_fail: Some("AUTO_RETRY".to_owned()),
                max_auto_retries,
            }),
            calibration: Calibration {
                last_dry_run: None,
                sample_count: None,
                success_rate: None,
            },
        }
    }

    fn verifier_pattern() -> Pattern {
        Pattern {
            id: "PV_canon_compliance".to_owned(),
            name: "LAEX verify".to_owned(),
            role: Some("verifier".to_owned()),
            template: "Vet:\n{{primary_output}}\nCanon:\n{{canon_excerpts}}\nBaseline:\n{{baseline_excerpts}}".to_owned(),
            eligible: Eligibility {
                siblings: vec!["laex".to_owned()],
                tool_use_required: false,
                max_input_tokens: 6000,
            },
            context_sources: None,
            shape: Shape {
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
            },
            refinement: None,
            verifier: None,
            calibration: Calibration {
                last_dry_run: None,
                sample_count: None,
                success_rate: None,
            },
        }
    }

    fn catalog_with(primary: Pattern, verifier: Pattern) -> Arc<OffloadCatalog> {
        Arc::new(OffloadCatalog {
            version: "1.1".to_owned(),
            last_calibrated: None,
            default_model: None,
            patterns: vec![primary, verifier],
        })
    }

    #[tokio::test]
    async fn supervise_pass_returns_primary_output() {
        let primary = primary_pattern_with_verifier("PV_canon_compliance", 1);
        let verifier = verifier_pattern();
        let cat = catalog_with(primary.clone(), verifier);
        let disp = Arc::new(MockDispatcher::new(vec![Ok(
            r#"{"verdict":"PASS","reason":"ok","amendment_hint":null}"#.to_owned(),
        )]));
        let sup = LaexSupervisor::new(cat, disp.clone());
        let verdict = sup
            .supervise(
                &primary,
                "function clamp() {}".to_owned(),
                "orig prompt",
                &VerifierContext::default(),
            )
            .await
            .unwrap();
        match verdict {
            SupervisorVerdict::Pass { output } => assert_eq!(output, "function clamp() {}"),
            other => panic!("expected Pass, got {other:?}"),
        }
        assert_eq!(disp.calls().len(), 1);
    }

    #[tokio::test]
    async fn supervise_hitl_returns_hitl_verdict() {
        let primary = primary_pattern_with_verifier("PV_canon_compliance", 1);
        let verifier = verifier_pattern();
        let cat = catalog_with(primary.clone(), verifier);
        let disp = Arc::new(MockDispatcher::new(vec![Ok(
            r#"{"verdict":"HITL","reason":"violates canon §63","amendment_hint":"add validation"}"#
                .to_owned(),
        )]));
        let sup = LaexSupervisor::new(cat, disp);
        let v = sup
            .supervise(
                &primary,
                "function clamp() {}".to_owned(),
                "orig",
                &VerifierContext::default(),
            )
            .await
            .unwrap();
        match v {
            SupervisorVerdict::Hitl {
                reason,
                last_output,
                last_amendment_hint,
            } => {
                assert!(reason.contains("violates"));
                assert_eq!(last_output, "function clamp() {}");
                assert_eq!(last_amendment_hint.as_deref(), Some("add validation"));
            }
            other => panic!("expected Hitl, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn supervise_retry_then_pass() {
        let primary = primary_pattern_with_verifier("PV_canon_compliance", 1);
        let verifier = verifier_pattern();
        let cat = catalog_with(primary.clone(), verifier);
        let disp = Arc::new(MockDispatcher::new(vec![
            // 1st verifier call → RETRY
            Ok(r#"{"verdict":"RETRY","reason":"missing guard","amendment_hint":"check NaN first"}"#.to_owned()),
            // Refined primary re-dispatch → produces clean function
            Ok("function clamp(n) { if (isNaN(n)) return 0; return n; }".to_owned()),
            // 2nd verifier call → PASS
            Ok(r#"{"verdict":"PASS","reason":"now correct","amendment_hint":null}"#.to_owned()),
        ]));
        let sup = LaexSupervisor::new(cat, disp.clone());
        let v = sup
            .supervise(
                &primary,
                "function clamp() {}".to_owned(),
                "orig prompt",
                &VerifierContext::default(),
            )
            .await
            .unwrap();
        match v {
            SupervisorVerdict::Pass { output } => {
                assert!(
                    output.contains("isNaN"),
                    "expected refined output, got {output:?}"
                );
            }
            other => panic!("expected Pass after retry, got {other:?}"),
        }
        let calls = disp.calls();
        assert_eq!(calls.len(), 3);
        // Calls in order: verifier, primary refined, verifier.
        assert_eq!(calls[0].0, "PV_canon_compliance");
        assert_eq!(calls[1].0, "P3");
        assert_eq!(calls[2].0, "PV_canon_compliance");
        // Refined prompt should carry the LAEX hint.
        assert!(calls[1].1.contains("check NaN first"));
    }

    #[tokio::test]
    async fn supervise_retry_exhausts_to_hitl() {
        let primary = primary_pattern_with_verifier("PV_canon_compliance", 1);
        let verifier = verifier_pattern();
        let cat = catalog_with(primary.clone(), verifier);
        // RETRY → primary re-dispatch → RETRY → exhaust
        let disp = Arc::new(MockDispatcher::new(vec![
            Ok(r#"{"verdict":"RETRY","reason":"first miss","amendment_hint":"hint1"}"#.to_owned()),
            Ok("function clamp(n) {}".to_owned()),
            Ok(r#"{"verdict":"RETRY","reason":"still wrong","amendment_hint":"hint2"}"#.to_owned()),
        ]));
        let sup = LaexSupervisor::new(cat, disp);
        let v = sup
            .supervise(
                &primary,
                "function clamp() {}".to_owned(),
                "orig",
                &VerifierContext::default(),
            )
            .await
            .unwrap();
        match v {
            SupervisorVerdict::Hitl {
                reason,
                last_amendment_hint,
                ..
            } => {
                assert!(reason.contains("max_auto_retries=1 exhausted"));
                assert!(reason.contains("still wrong"));
                assert_eq!(last_amendment_hint.as_deref(), Some("hint2"));
            }
            other => panic!("expected Hitl after exhaustion, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn supervise_pattern_without_verifier_passes_through() {
        let mut primary = primary_pattern_with_verifier("PV_canon_compliance", 1);
        primary.verifier = None;
        let verifier = verifier_pattern();
        let cat = catalog_with(primary.clone(), verifier);
        // No dispatcher calls expected.
        let disp = Arc::new(MockDispatcher::new(vec![]));
        let sup = LaexSupervisor::new(cat, disp.clone());
        let v = sup
            .supervise(
                &primary,
                "output".to_owned(),
                "prompt",
                &VerifierContext::default(),
            )
            .await
            .unwrap();
        assert!(matches!(v, SupervisorVerdict::Pass { output } if output == "output"));
        assert_eq!(disp.calls().len(), 0);
    }

    #[tokio::test]
    async fn supervise_disabled_verifier_passes_through() {
        let mut primary = primary_pattern_with_verifier("PV_canon_compliance", 1);
        if let Some(v) = primary.verifier.as_mut() {
            v.enabled = false;
        }
        let verifier = verifier_pattern();
        let cat = catalog_with(primary.clone(), verifier);
        let disp = Arc::new(MockDispatcher::new(vec![]));
        let sup = LaexSupervisor::new(cat, disp.clone());
        let v = sup
            .supervise(
                &primary,
                "output".to_owned(),
                "prompt",
                &VerifierContext::default(),
            )
            .await
            .unwrap();
        assert!(matches!(v, SupervisorVerdict::Pass { .. }));
        assert_eq!(disp.calls().len(), 0);
    }

    #[tokio::test]
    async fn supervise_dangling_verifier_pattern_errors() {
        let primary = primary_pattern_with_verifier("PV_nonexistent", 1);
        let verifier = verifier_pattern();
        let cat = catalog_with(primary.clone(), verifier);
        let disp = Arc::new(MockDispatcher::new(vec![]));
        let sup = LaexSupervisor::new(cat, disp);
        let err = sup
            .supervise(
                &primary,
                "output".to_owned(),
                "prompt",
                &VerifierContext::default(),
            )
            .await
            .unwrap_err();
        match err {
            SupervisorError::VerifierNotInCatalog(id) => assert_eq!(id, "PV_nonexistent"),
            other => panic!("expected VerifierNotInCatalog, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn supervise_invalid_verifier_json_errors() {
        let primary = primary_pattern_with_verifier("PV_canon_compliance", 1);
        let verifier = verifier_pattern();
        let cat = catalog_with(primary.clone(), verifier);
        let disp = Arc::new(MockDispatcher::new(vec![Ok("not json at all".to_owned())]));
        let sup = LaexSupervisor::new(cat, disp);
        let err = sup
            .supervise(
                &primary,
                "output".to_owned(),
                "prompt",
                &VerifierContext::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, SupervisorError::InvalidVerifierOutput(_)));
    }

    #[tokio::test]
    async fn supervise_substitutes_template_tokens() {
        let primary = primary_pattern_with_verifier("PV_canon_compliance", 1);
        let verifier = verifier_pattern();
        let cat = catalog_with(primary.clone(), verifier);
        let disp = Arc::new(MockDispatcher::new(vec![Ok(
            r#"{"verdict":"PASS","reason":"ok","amendment_hint":null}"#.to_owned(),
        )]));
        let sup = LaexSupervisor::new(cat, disp.clone());
        let ctx = VerifierContext {
            canon_excerpts: "CANON_BODY".to_owned(),
            baseline_excerpts: "BASELINE_BODY".to_owned(),
        };
        let _ = sup
            .supervise(&primary, "PRIMARY_OUT".to_owned(), "prompt", &ctx)
            .await
            .unwrap();
        let calls = disp.calls();
        let verifier_prompt = &calls[0].1;
        assert!(verifier_prompt.contains("PRIMARY_OUT"));
        assert!(verifier_prompt.contains("CANON_BODY"));
        assert!(verifier_prompt.contains("BASELINE_BODY"));
        assert!(!verifier_prompt.contains("{{primary_output}}"));
    }

    #[tokio::test]
    async fn supervise_dispatcher_error_propagates() {
        let primary = primary_pattern_with_verifier("PV_canon_compliance", 1);
        let verifier = verifier_pattern();
        let cat = catalog_with(primary.clone(), verifier);
        let disp = Arc::new(MockDispatcher::new(vec![Err("network down".to_owned())]));
        let sup = LaexSupervisor::new(cat, disp);
        let err = sup
            .supervise(
                &primary,
                "output".to_owned(),
                "prompt",
                &VerifierContext::default(),
            )
            .await
            .unwrap_err();
        match err {
            SupervisorError::Dispatch { pattern_id, reason } => {
                assert_eq!(pattern_id, "PV_canon_compliance");
                assert!(reason.contains("network down"));
            }
            other => panic!("expected Dispatch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn supervise_retry_with_shape_invalid_redispatch_escalates() {
        let primary = primary_pattern_with_verifier("PV_canon_compliance", 1);
        let verifier = verifier_pattern();
        let cat = catalog_with(primary.clone(), verifier);
        let disp = Arc::new(MockDispatcher::new(vec![
            // Verifier RETRY
            Ok(r#"{"verdict":"RETRY","reason":"first miss","amendment_hint":"hint"}"#.to_owned()),
            // Refined primary output contains a forbidden fence → shape invalid
            Ok("```js\nfunction clamp(){}\n```".to_owned()),
        ]));
        let sup = LaexSupervisor::new(cat, disp);
        let v = sup
            .supervise(
                &primary,
                "function clamp() {}".to_owned(),
                "orig",
                &VerifierContext::default(),
            )
            .await
            .unwrap();
        match v {
            SupervisorVerdict::Hitl { reason, .. } => {
                assert!(reason.contains("retry produced shape-invalid output"));
            }
            other => panic!("expected Hitl after shape-invalid retry, got {other:?}"),
        }
    }
}
