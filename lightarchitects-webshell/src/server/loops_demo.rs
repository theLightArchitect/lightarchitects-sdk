//! LA loop strategy demo — 17 strategies running live via local Ollama.
//!
//! Routes:
//!   GET /loops-demo          → HTML page
//!   GET /api/loops/demo      → SSE stream (?strategy=<name>&query=<text>)
//!   GET /loops-demo.js       → client JavaScript

use std::{collections::HashMap, convert::Infallible, sync::Arc};

use crate::server::AppState;
use async_trait::async_trait;
use axum::{
    extract::{Query, State},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::{StreamExt, stream};
use tokio::time::Duration;

use lightarchitects::agent::loops::cove::{Claim, VerificationQuestion};
use lightarchitects::agent::loops::critique_refine::{CritiqueExecutor, CritiqueState};
use lightarchitects::agent::loops::reflexion::ReflexionReview;
use lightarchitects::agent::loops::{
    AchExecutor, AchState, AchStrategy, BcraExecutor, BcraStrategy, Budget, BuildStrategy,
    CoVeExecutor, CoVeState, CoVeStrategy, DrainExecutor, DrainStrategy, EnrichStrategy,
    EnsembleStrategy, EvidenceRef, GateStrategy, InvestigationTaskTree, IttExecutor, IttStrategy,
    LoopError, LoopRunner, LoopState, MultiPassExecutor, MultiPassState, MultiPassVerifyStrategy,
    Outcome, Prediction, ReActExecutor, ReActPrompt, ReActStep, ReActStrategy, RedTeamExecutor,
    RedTeamState, RedTeamStrategy, ReflexionEntry, ReflexionExecutor, ReflexionLoopState,
    ReflexionState, ReflexionStrategy, ScopeGovernorStrategy, ScrumStrategy, SecureStrategy,
    StepContext, TestResult, TestType, VerificationResult, VerificationStatus, VerifiedClaim,
};
use lightarchitects::agent::openai_compat::OpenAICompatProvider;
use lightarchitects::agent::{AgentRequest, ChainContext, LlmAgentProvider, ProviderEvent};

// ── Shared demo executor ──────────────────────────────────────────────────────

#[derive(Clone)]
struct DemoExec {
    provider: Arc<OpenAICompatProvider>,
    model: String,
    /// SSE sender — every TextDelta event from the streaming provider becomes
    /// one `{"type":"delta","text":"…"}` frame so the browser can render
    /// token-by-token output without waiting for a phase to finish. `None`
    /// would mean "swallow chunks", but the demo always provides a channel.
    tx: tokio::sync::mpsc::Sender<String>,
}

impl DemoExec {
    fn new(
        base_url: String,
        api_key: String,
        model: String,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<Self, String> {
        tracing::info!(
            target: "loops_demo",
            base_url = %base_url,
            model = %model,
            "DemoExec routing through LiteLLM proxy"
        );
        let provider = OpenAICompatProvider::for_litellm(Some(base_url), api_key, model.clone())
            .map_err(|e| {
                tracing::error!(target: "loops_demo", error = %e, "LiteLLM provider construction failed");
                e
            })?;
        Ok(Self {
            provider: Arc::new(provider),
            model,
            tx,
        })
    }

    async fn ask(&self, prompt: &str) -> Result<String, LoopError> {
        let req = AgentRequest {
            sibling_identity: "loop-demo".to_owned(),
            user_prompt: prompt.to_owned(),
            schema: None,
            allowed_tools: vec![],
            max_turns: 1,
            max_budget_usd: 1.0,
            model_hint: Some(self.model.clone()),
            parent_span_id: None,
            chain_origin: None,
            chain_depth: 0,
            aud: None,
            conversation_history: vec![],
            tool_definitions: vec![],
        };
        let san = req
            .sanitize()
            .map_err(|e| LoopError::StepFailed(format!("sanitize: {e}")))?;
        // Use spawn_streaming so we can forward each token to the SSE channel
        // as it arrives. The Strategy still gets the collected String — the
        // streaming is purely a UX side effect for the browser.
        let mut stream = self
            .provider
            .spawn_streaming(san)
            .await
            .map_err(|e| LoopError::StepFailed(format!("provider: {e}")))?;
        let mut collected = String::new();
        while let Some(event) = stream.next().await {
            if let ProviderEvent::TextDelta { text, .. } = event {
                collected.push_str(&text);
                // Best-effort forward; if the SSE receiver dropped (client
                // disconnected) we still finish collecting so the Strategy
                // gets a coherent final string.
                let frame = serde_json::json!({"type":"delta","text": text}).to_string();
                let _ = self.tx.send(frame).await;
            }
        }
        Ok(collected)
    }
}

// ── ReActExecutor ─────────────────────────────────────────────────────────────

#[async_trait]
impl ReActExecutor for DemoExec {
    async fn step(&self, prompt: &ReActPrompt, _ctx: &StepContext) -> Result<ReActStep, LoopError> {
        let text = self
            .ask(&format!(
                "ReAct step for query: '{}'. Give one-sentence Thought and Action.",
                prompt.query
            ))
            .await?;
        Ok(ReActStep {
            thought: format!("Reasoning about: {}", prompt.query),
            action: "search".to_owned(),
            observation: text,
            result: None,
            phase: lightarchitects::agent::loops::ReActPhase::Scan,
        })
    }
}

// ── BcraExecutor ──────────────────────────────────────────────────────────────

#[async_trait]
impl BcraExecutor for DemoExec {
    async fn map(&self, _ctx: &StepContext) -> Result<Vec<String>, LoopError> {
        Ok(vec!["asset-A".to_owned(), "asset-B".to_owned()])
    }

    async fn pull(&self, assets: &[String], _ctx: &StepContext) -> Result<Vec<String>, LoopError> {
        let text = self
            .ask(&format!(
                "List 3 threat vectors for assets: {}",
                assets.join(", ")
            ))
            .await?;
        Ok(vec![
            text,
            "supply-chain-risk".to_owned(),
            "privilege-escalation".to_owned(),
        ])
    }

    async fn score(&self, _threats: &[String], _ctx: &StepContext) -> Result<f64, LoopError> {
        Ok(0.42)
    }

    async fn research(
        &self,
        threats: &[String],
        _score: f64,
        _ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError> {
        let text = self
            .ask(&format!(
                "One-sentence evidence for threat: {}",
                threats.first().map(String::as_str).unwrap_or("unknown")
            ))
            .await?;
        Ok(vec![text])
    }

    async fn prove(
        &self,
        evidence: &[String],
        _ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError> {
        Ok(evidence.to_vec())
    }

    async fn declare(
        &self,
        state: &lightarchitects::agent::loops::BcraState,
        _ctx: &StepContext,
    ) -> Result<String, LoopError> {
        let text = self
            .ask(&format!(
                "2-sentence risk declaration. Blast score: {:.2}.",
                state.blast_score
            ))
            .await?;
        Ok(text)
    }
}

// ── DrainExecutor ─────────────────────────────────────────────────────────────

#[async_trait]
impl DrainExecutor for DemoExec {
    async fn next_item(
        &self,
        queue: &[String],
        _ctx: &StepContext,
    ) -> Result<Option<String>, LoopError> {
        Ok(queue.first().cloned())
    }

    async fn process(&self, item: &str, _ctx: &StepContext) -> Result<bool, LoopError> {
        let _text = self.ask(&format!("Process queue item: {item}")).await?;
        Ok(true)
    }

    async fn is_empty(
        &self,
        state: &lightarchitects::agent::loops::DrainState,
        _ctx: &StepContext,
    ) -> Result<bool, LoopError> {
        Ok(state.queue.is_empty())
    }
}

// ── MultiPassExecutor ─────────────────────────────────────────────────────────

#[async_trait]
impl MultiPassExecutor for DemoExec {
    async fn verify_pass(
        &self,
        n: u32,
        subject: &str,
        _ctx: &StepContext,
    ) -> Result<(bool, String), LoopError> {
        let note = self
            .ask(&format!(
                "One-line verdict for verification pass #{n}: {subject}"
            ))
            .await?;
        Ok((true, note))
    }

    async fn aggregate(
        &self,
        results: &[bool],
        notes: &[String],
        _ctx: &StepContext,
    ) -> Result<String, LoopError> {
        let passed = results.iter().filter(|&&b| b).count();
        Ok(format!(
            "{}/{} passes succeeded. {}",
            passed,
            results.len(),
            notes.join("; ")
        ))
    }
}

// ── CritiqueExecutor ──────────────────────────────────────────────────────────

#[async_trait]
impl CritiqueExecutor for DemoExec {
    async fn theorize(&self, draft: &str, _ctx: &StepContext) -> Result<Vec<String>, LoopError> {
        let text = self
            .ask(&format!("Give 2 short critiques of: {draft}"))
            .await?;
        Ok(vec![text, "Consider expanding scope.".to_owned()])
    }

    async fn verify(
        &self,
        draft: &str,
        critiques: &[String],
        _ctx: &StepContext,
    ) -> Result<String, LoopError> {
        self.ask(&format!(
            "Improve draft based on critiques.\nDraft: {draft}\nCritiques: {}",
            critiques.join(", ")
        ))
        .await
    }

    async fn close(&self, draft: &str, _ctx: &StepContext) -> Result<String, LoopError> {
        self.ask(&format!("Finalize in one sentence: {draft}"))
            .await
    }
}

// ── ReflexionExecutor ─────────────────────────────────────────────────────────

#[async_trait]
impl ReflexionExecutor for DemoExec {
    async fn generate(
        &self,
        case_id: &str,
        context: &str,
        _ctx: &StepContext,
    ) -> Result<ReflexionEntry, LoopError> {
        let text = self
            .ask(&format!("Root cause for case '{case_id}': {context}"))
            .await?;
        Ok(ReflexionEntry {
            case_id: case_id.to_owned(),
            state: ReflexionState::Provisional,
            new_patterns: vec!["pattern-A".to_owned()],
            applied_knowledge: vec![],
            root_cause: Some(text),
            improvements: vec!["improvement-1".to_owned()],
            confidence: 0.6,
        })
    }

    async fn review(
        &self,
        entry: &ReflexionEntry,
        _ctx: &StepContext,
    ) -> Result<ReflexionReview, LoopError> {
        let _text = self
            .ask(&format!("Should case '{}' be promoted?", entry.case_id))
            .await?;
        Ok(ReflexionReview {
            should_promote: true,
            improvements: vec!["verified by review".to_owned()],
            confidence_delta: 0.2,
        })
    }
}

// ── CoVeExecutor ──────────────────────────────────────────────────────────────

#[async_trait]
impl CoVeExecutor for DemoExec {
    async fn extract_claims(
        &self,
        input: &str,
        _ctx: &StepContext,
    ) -> Result<Vec<Claim>, LoopError> {
        let text = self
            .ask(&format!("Extract one verifiable claim from: {input}"))
            .await?;
        Ok(vec![
            Claim {
                text,
                source: "demo-input".to_owned(),
                category: lightarchitects::agent::loops::cove::ClaimCategory::Factual,
            },
            Claim {
                text: "Secondary claim from demo extraction.".to_owned(),
                source: "demo-synthesis".to_owned(),
                category: lightarchitects::agent::loops::cove::ClaimCategory::Causal,
            },
        ])
    }

    async fn plan_verification(
        &self,
        claims: &[Claim],
        _ctx: &StepContext,
    ) -> Result<Vec<VerificationQuestion>, LoopError> {
        Ok(claims
            .iter()
            .enumerate()
            .map(|(i, c)| VerificationQuestion {
                question: format!("Is this claim accurate? '{}'", c.text),
                claim_index: i,
                evidence_source: "demo-source".to_owned(),
                expected_format: "yes/no with rationale".to_owned(),
            })
            .collect())
    }

    async fn verify(
        &self,
        claims: &[Claim],
        questions: &[VerificationQuestion],
        _ctx: &StepContext,
    ) -> Result<Vec<VerifiedClaim>, LoopError> {
        let mut out = Vec::new();
        for (i, claim) in claims.iter().enumerate() {
            let q = questions
                .get(i)
                .map(|q| q.question.as_str())
                .unwrap_or("no question");
            let answer = self.ask(&format!("Answer briefly: {q}")).await?;
            out.push(VerifiedClaim {
                claim: claim.clone(),
                questions: questions
                    .get(i)
                    .cloned()
                    .map(|q| vec![q])
                    .unwrap_or_default(),
                status: VerificationStatus::Verified,
                evidence: answer,
                confidence: 0.8,
            });
        }
        Ok(out)
    }
}

// ── RedTeamExecutor ───────────────────────────────────────────────────────────

#[async_trait]
impl RedTeamExecutor for DemoExec {
    async fn hydrate(&self, scope: &str, _ctx: &StepContext) -> Result<Vec<String>, LoopError> {
        Ok(vec![
            format!("control-anchor-1 for {scope}"),
            "access-control".to_owned(),
        ])
    }

    async fn surface(
        &self,
        scope: &str,
        anchors: &[String],
        _ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError> {
        let text = self
            .ask(&format!(
                "List 3 attack surface entries for scope '{}' with anchors: {}",
                scope,
                anchors.join(", ")
            ))
            .await?;
        Ok(vec![
            text,
            "input-validation-bypass".to_owned(),
            "auth-token-leak".to_owned(),
        ])
    }

    async fn probe(
        &self,
        surface: &[String],
        _ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError> {
        let text = self
            .ask(&format!(
                "Probe attack surface: {}",
                surface.first().map(String::as_str).unwrap_or("none")
            ))
            .await?;
        Ok(vec![text])
    }

    async fn chain(&self, findings: &[String], _ctx: &StepContext) -> Result<String, LoopError> {
        self.ask(&format!(
            "One-sentence exploit chain from: {}",
            findings.join(", ")
        ))
        .await
    }

    async fn verdict(&self, state: &RedTeamState, _ctx: &StepContext) -> Result<String, LoopError> {
        self.ask(&format!(
            "2-sentence security verdict for scope '{}'.",
            state.scope
        ))
        .await
    }
}

// ── AchExecutor ───────────────────────────────────────────────────────────────

#[async_trait]
impl AchExecutor for DemoExec {
    async fn generate_hypotheses(
        &self,
        query: &str,
        _ctx: &StepContext,
    ) -> Result<Vec<String>, LoopError> {
        let text = self
            .ask(&format!("Generate 3 competing hypotheses for: {query}"))
            .await?;
        Ok(vec![
            format!("H1: {}", &text[..text.len().min(80)]),
            "H2: Alternative explanation".to_owned(),
            "H3: Null hypothesis — coincidence".to_owned(),
        ])
    }

    async fn build_predictions(
        &self,
        hypotheses: &[String],
        _ctx: &StepContext,
    ) -> Result<Vec<Vec<Prediction>>, LoopError> {
        Ok(hypotheses
            .iter()
            .map(|_h| {
                vec![Prediction {
                    claim: "evidence-present if hypothesis is true".to_owned(),
                    test_type: TestType::PatternPresence,
                    result: None,
                }]
            })
            .collect())
    }

    async fn score_predictions(
        &self,
        _hypotheses: &[String],
        predictions: &[Vec<Prediction>],
        _ctx: &StepContext,
    ) -> Result<Vec<Vec<Prediction>>, LoopError> {
        Ok(predictions
            .iter()
            .map(|ps| {
                ps.iter()
                    .map(|p| Prediction {
                        result: Some(TestResult::Confirmed("demo evidence".to_owned())),
                        ..p.clone()
                    })
                    .collect()
            })
            .collect())
    }
}

// ── IttExecutor ───────────────────────────────────────────────────────────────

#[async_trait]
impl IttExecutor for DemoExec {
    async fn expand(
        &self,
        node_id: &str,
        hypothesis: &str,
        _ctx: &StepContext,
    ) -> Result<Vec<(String, f64)>, LoopError> {
        let text = self
            .ask(&format!(
                "Expand '{hypothesis}' (node {node_id}) into 2 sub-hypotheses."
            ))
            .await?;
        Ok(vec![
            (format!("sub-H-A: {}", &text[..text.len().min(60)]), 0.7),
            ("sub-H-B: alternative branch".to_owned(), 0.4),
        ])
    }

    async fn collect_evidence(
        &self,
        node_id: &str,
        hypothesis: &str,
        _ctx: &StepContext,
    ) -> Result<Vec<EvidenceRef>, LoopError> {
        let _text = self
            .ask(&format!(
                "Collect evidence for '{hypothesis}' (node {node_id})."
            ))
            .await?;
        Ok(vec![EvidenceRef {
            id: format!("ev-{node_id}-1"),
            path: "/demo/evidence.log".to_owned(),
            description: "demo evidence entry".to_owned(),
            collected_by: lightarchitects::agent::loops::QPhase::Scan,
        }])
    }

    async fn verify_hypothesis(
        &self,
        _node_id: &str,
        hypothesis: &str,
        _ctx: &StepContext,
    ) -> Result<VerificationResult, LoopError> {
        Ok(VerificationResult {
            confirmed: true,
            evidence_ids: vec!["ev-demo-1".to_owned()],
            confidence: 0.8,
            conclusion: format!("Hypothesis verified: {hypothesis}"),
        })
    }
}

// ── Strategy runner helper ────────────────────────────────────────────────────

async fn run_strategy_sse(
    name: &str,
    query: String,
    base_url: String,
    api_key: String,
    model: String,
    tx: tokio::sync::mpsc::Sender<String>,
) {
    async fn send(tx: &tokio::sync::mpsc::Sender<String>, msg: String) {
        let _ = tx.send(msg).await;
    }

    let exec = match DemoExec::new(base_url.clone(), api_key.clone(), model.clone(), tx.clone()) {
        Ok(e) => e,
        Err(err) => {
            let _ = tx
                .send(
                    serde_json::json!({
                        "type":"error",
                        "text": format!("LiteLLM provider construction failed: {err}")
                    })
                    .to_string(),
                )
                .await;
            return;
        }
    };
    let chain = ChainContext::default();
    let budget = Budget::new(5, f64::MAX);

    macro_rules! step_msg {
        ($turn:expr, $text:expr) => {
            serde_json::json!({"type":"step","turn":$turn,"text":$text}).to_string()
        };
    }
    macro_rules! done_msg {
        ($text:expr) => {
            serde_json::json!({"type":"done","text":$text}).to_string()
        };
    }
    macro_rules! err_msg {
        ($text:expr) => {
            serde_json::json!({"type":"error","text":$text}).to_string()
        };
    }

    match name {
        // ── L2 strategies ──────────────────────────────────────────────────
        "gate" => {
            let mut s = LoopRunner::new(GateStrategy::new(), budget).run(
                LoopState::new(query),
                chain,
                None,
            );
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(step.turn, format!("phase {} in progress", st.phase)),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(&tx, done_msg!(out.summary.clone())).await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "scope_governor" => {
            let mut s = LoopRunner::new(ScopeGovernorStrategy::new(), budget).run(
                LoopState::new(query),
                chain,
                None,
            );
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(step.turn, format!("scope gate {} checked", st.phase)),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(&tx, done_msg!(out.summary.clone())).await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "build" => {
            let mut s = LoopRunner::new(BuildStrategy::new(), budget).run(
                LoopState::new(query),
                chain,
                None,
            );
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(step.turn, format!("build phase {}", st.phase)),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(&tx, done_msg!(out.summary.clone())).await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused — HITL required")).await;
                            return;
                        }
                    },
                }
            }
        }

        "secure" => {
            let mut s = LoopRunner::new(SecureStrategy::new(), budget).run(
                LoopState::new(query),
                chain,
                None,
            );
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(step.turn, format!("security phase {}", st.phase)),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(&tx, done_msg!(out.summary.clone())).await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "scrum" => {
            let mut s = LoopRunner::new(ScrumStrategy::review(), budget).run(
                LoopState::new(query),
                chain,
                None,
            );
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(step.turn, format!("SCRUM round {}", st.phase)),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(&tx, done_msg!(out.summary.clone())).await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "enrich" => {
            let mut s = LoopRunner::new(EnrichStrategy::new(), budget).run(
                LoopState::new(query),
                chain,
                None,
            );
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(step.turn, format!("enrich layer {}", st.phase)),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(&tx, done_msg!(out.summary.clone())).await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        // ── L0 strategies ──────────────────────────────────────────────────
        "react" => {
            let mut s = LoopRunner::new(ReActStrategy::new(exec), budget).run(
                ReActPrompt::new(query, 3),
                chain,
                None,
            );
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(
                                    step.turn,
                                    format!("phase {} — {} steps", st.phase, st.steps.len())
                                ),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            let obs = out
                                .steps
                                .last()
                                .map(|s| s.observation.as_str())
                                .unwrap_or("done");
                            send(
                                &tx,
                                done_msg!(format!(
                                    "ReAct complete — {}",
                                    &obs[..obs.len().min(150)]
                                )),
                            )
                            .await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "bcra" => {
            let mut s = LoopRunner::new(BcraStrategy::new(exec), budget).run(
                lightarchitects::agent::loops::BcraState::new(),
                chain,
                None,
            );
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(step.turn, format!("BCRA phase {:?}", st.phase)),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(
                                &tx,
                                done_msg!(format!(
                                    "Blast score: {:.2} — {}",
                                    out.blast_score,
                                    &out.declaration[..out.declaration.len().min(150)]
                                )),
                            )
                            .await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "multipass" => {
            let mut s = LoopRunner::new(MultiPassVerifyStrategy::new(exec), budget).run(
                MultiPassState::new(query, 3),
                chain,
                None,
            );
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(
                                    step.turn,
                                    format!("pass {}/{}", st.pass_index, st.max_passes)
                                ),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(
                                &tx,
                                done_msg!(format!(
                                    "{}/{} passed — {}",
                                    out.passes_passed,
                                    out.passes_run,
                                    &out.verdict[..out.verdict.len().min(120)]
                                )),
                            )
                            .await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "drain" => {
            let items: Vec<String> = query
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            let items = if items.is_empty() {
                vec![
                    "task-1".to_owned(),
                    "task-2".to_owned(),
                    "task-3".to_owned(),
                ]
            } else {
                items
            };
            let init = lightarchitects::agent::loops::DrainState::new(items);
            let mut s = LoopRunner::new(DrainStrategy::new(exec), budget).run(init, chain, None);
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(step.turn, format!("{} items remaining", st.queue.len())),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(
                                &tx,
                                done_msg!(format!(
                                    "Drained {}/{} items ({:.0}%)",
                                    out.processed_count,
                                    out.processed_count + out.failed_count,
                                    out.drain_fraction * 100.0
                                )),
                            )
                            .await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "critique_refine" => {
            let mut s = LoopRunner::new(
                lightarchitects::agent::loops::CritiqueRefineStrategy::new(exec, 2),
                budget,
            )
            .run(CritiqueState::new(query), chain, None);
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(
                                    step.turn,
                                    format!("round {} ({:?})", st.rounds, st.phase)
                                ),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(
                                &tx,
                                done_msg!(format!("Final: {}", &out[..out.len().min(200)])),
                            )
                            .await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "reflexion" => {
            let init = ReflexionLoopState::new("demo-case", query.clone(), 3);
            let mut s =
                LoopRunner::new(ReflexionStrategy::new(exec), budget).run(init, chain, None);
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(
                                    step.turn,
                                    format!("round {}/{}", st.round, st.max_rounds)
                                ),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            let root = out.root_cause.as_deref().unwrap_or("n/a");
                            send(
                                &tx,
                                done_msg!(format!(
                                    "State: {:?} — root cause: {}",
                                    out.state,
                                    &root[..root.len().min(150)]
                                )),
                            )
                            .await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "cove" => {
            let init = CoVeState::new(query);
            let mut s = LoopRunner::new(CoVeStrategy::new(exec), budget).run(init, chain, None);
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(step.turn, format!("CoVe phase {:?}", st.phase)),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(
                                &tx,
                                done_msg!(format!(
                                    "{} claims — {}/{} verified",
                                    out.claims.len(),
                                    out.verified_count,
                                    out.claims.len()
                                )),
                            )
                            .await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "red_team" => {
            let init = RedTeamState::new(query.clone());
            let mut s = LoopRunner::new(RedTeamStrategy::new(exec), budget).run(init, chain, None);
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(step.turn, format!("red-team phase {:?}", st.phase)),
                            )
                            .await;
                        }
                        Outcome::Halt(ref out) => {
                            send(
                                &tx,
                                done_msg!(format!(
                                    "Verdict: {}",
                                    &out.verdict[..out.verdict.len().min(200)]
                                )),
                            )
                            .await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "ach" => {
            let init = AchState::new(query, 2);
            let mut s = LoopRunner::new(AchStrategy::new(exec), budget).run(init, chain, None);
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            send(
                                &tx,
                                step_msg!(step.turn, format!("ACH phase {:?}", st.phase)),
                            )
                            .await;
                        }
                        Outcome::Halt(ref tests) => {
                            let top = tests
                                .first()
                                .map(|t| t.hypothesis.as_str())
                                .unwrap_or("n/a");
                            send(
                                &tx,
                                done_msg!(format!(
                                    "Top hypothesis: {}",
                                    &top[..top.len().min(120)]
                                )),
                            )
                            .await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "itt" => {
            let init = InvestigationTaskTree::new("demo-case", query.clone());
            let mut s = LoopRunner::new(IttStrategy::new(exec), budget).run(init, chain, None);
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref tree) => {
                            send(
                                &tx,
                                step_msg!(
                                    step.turn,
                                    format!("{} unexplored nodes", tree.unexplored.len())
                                ),
                            )
                            .await;
                        }
                        Outcome::Halt(ref tree) => {
                            let top = tree
                                .top_hypothesis()
                                .map(|n| n.hypothesis.as_str())
                                .unwrap_or("n/a");
                            send(
                                &tx,
                                done_msg!(format!(
                                    "Top hypothesis: {}",
                                    &top[..top.len().min(120)]
                                )),
                            )
                            .await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        "ensemble" => {
            let exec2 =
                match DemoExec::new(base_url.clone(), api_key.clone(), model.clone(), tx.clone()) {
                    Ok(e) => e,
                    Err(err) => {
                        send(&tx, err_msg!(format!("ensemble exec2: {err}"))).await;
                        return;
                    }
                };
            let branches = vec![ReActStrategy::new(exec), ReActStrategy::new(exec2)];
            let strategy = EnsembleStrategy::new(branches);
            let q2 = query.clone();
            let init =
                strategy.initial_state(vec![ReActPrompt::new(query, 3), ReActPrompt::new(q2, 3)]);
            let mut s = LoopRunner::new(strategy, budget).run(init, chain, None);
            while let Some(r) = s.next().await {
                match r {
                    Err(e) => {
                        send(&tx, err_msg!(e.to_string())).await;
                        return;
                    }
                    Ok(step) => match step.outcome {
                        Outcome::Continue(ref st) => {
                            let active = st.active.iter().filter(|o| o.is_some()).count();
                            send(
                                &tx,
                                step_msg!(
                                    step.turn,
                                    format!("{}/{} branches active", active, st.active.len())
                                ),
                            )
                            .await;
                        }
                        Outcome::Halt(ref outputs) => {
                            let done = outputs.iter().filter(|o| o.is_some()).count();
                            send(
                                &tx,
                                done_msg!(format!("{}/{} branches completed", done, outputs.len())),
                            )
                            .await;
                            return;
                        }
                        Outcome::Pause(_, _) => {
                            send(&tx, done_msg!("paused")).await;
                            return;
                        }
                    },
                }
            }
        }

        unknown => {
            send(&tx, err_msg!(format!("unknown strategy: {unknown}"))).await;
            return;
        }
    }

    send(&tx, done_msg!("stream complete")).await;
}

// ── HTTP handlers ─────────────────────────────────────────────────────────────

/// `GET /api/loops/demo?strategy=<name>&query=<text>` — SSE stream.
pub async fn demo_dispatch_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let strategy = params
        .get("strategy")
        .cloned()
        .unwrap_or_else(|| "gate".to_owned());
    let query = params
        .get("query")
        .cloned()
        .unwrap_or_else(|| "what is the capital of France?".to_owned());

    let cfg = state.litellm_config.read().await;
    let base_url = cfg.base_url.clone();
    use secrecy::ExposeSecret as _;
    let api_key = cfg.api_key.expose_secret().to_owned();
    let model = cfg.model.clone();
    drop(cfg);

    let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);

    let strat = strategy.clone();
    tokio::spawn(async move {
        run_strategy_sse(&strat, query, base_url, api_key, model, tx).await;
    });

    let event_stream = stream::unfold(rx, |mut rx| async move {
        let msg = rx.recv().await?;
        let event = Event::default().data(msg);
        Some((Ok::<_, Infallible>(event), rx))
    });

    Sse::new(event_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response()
}

/// `GET /loops-demo` — HTML page.
pub async fn demo_page_handler() -> Response {
    axum::response::Html(DEMO_HTML).into_response()
}

/// `GET /loops-demo.js` — client JavaScript.
pub async fn demo_js_handler() -> Response {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        DEMO_JS,
    )
        .into_response()
}

// ── Static assets ─────────────────────────────────────────────────────────────

static DEMO_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>LA Loop Strategy Demo</title>
<style>
*{box-sizing:border-box;margin:0;padding:0}
body{font-family:'SF Mono','Cascadia Code',monospace;background:#0d0d0d;color:#e0e0e0;display:flex;height:100vh;overflow:hidden}
#sidebar{width:220px;border-right:1px solid #222;overflow-y:auto;flex-shrink:0}
#sidebar h2{font-size:11px;color:#555;padding:12px 14px 8px;text-transform:uppercase;letter-spacing:1px}
.sg{padding:6px 14px 4px;font-size:10px;color:#444;text-transform:uppercase;letter-spacing:.8px}
.sb{display:block;width:100%;text-align:left;padding:7px 14px;font-size:12px;font-family:inherit;background:none;border:none;color:#aaa;cursor:pointer;border-left:2px solid transparent}
.sb:hover{color:#fff;background:#111}
.sb.active{color:#7ec8ff;border-left-color:#7ec8ff;background:#0a1a2a}
#main{flex:1;display:flex;flex-direction:column;overflow:hidden}
#qbar{display:flex;gap:8px;padding:12px 16px;border-bottom:1px solid #222}
#qi{flex:1;background:#111;border:1px solid #333;color:#e0e0e0;padding:8px 12px;font-family:inherit;font-size:13px;border-radius:4px}
#rb{padding:8px 18px;background:#1a4a7a;color:#7ec8ff;border:1px solid #2a5a8a;font-family:inherit;font-size:13px;cursor:pointer;border-radius:4px}
#rb:hover{background:#1e5a90}
#rb:disabled{opacity:.4;cursor:not-allowed}
#out{flex:1;overflow-y:auto;padding:16px}
.ev{display:flex;gap:10px;padding:4px 0;font-size:12px;line-height:1.6}
.ev .tn{color:#555;min-width:28px}
.ev.step .tx{color:#c8d8e8}
.ev.done .tx{color:#7ec8ff;font-weight:bold}
.ev.error .tx{color:#ff6b6b}
.ev.info .tx{color:#888}
.ev.delta .tx{color:#9ed59e;white-space:pre-wrap;font-family:ui-monospace,Menlo,monospace}
.ev.delta .tn{color:#3a7a3a}
#st{padding:8px 16px;font-size:11px;color:#555;border-top:1px solid #1a1a1a}
</style>
</head>
<body>
<div id="sidebar">
<h2>17 Strategies</h2>
<div class="sg">L2 — Shared State</div>
<button class="sb" data-s="gate">GateStrategy</button>
<button class="sb" data-s="scope_governor">ScopeGovernor</button>
<button class="sb" data-s="build">BuildStrategy</button>
<button class="sb" data-s="secure">SecureStrategy</button>
<button class="sb" data-s="scrum">ScrumStrategy</button>
<button class="sb" data-s="enrich">EnrichStrategy</button>
<div class="sg">L0 — Custom Executor</div>
<button class="sb" data-s="react">ReActStrategy</button>
<button class="sb" data-s="bcra">BcraStrategy</button>
<button class="sb" data-s="multipass">MultiPassVerify</button>
<button class="sb" data-s="drain">DrainStrategy</button>
<button class="sb" data-s="critique_refine">CritiqueRefine</button>
<button class="sb" data-s="reflexion">ReflexionStrategy</button>
<button class="sb" data-s="cove">CoVeStrategy</button>
<button class="sb" data-s="red_team">RedTeamStrategy</button>
<button class="sb" data-s="ach">AchStrategy</button>
<button class="sb" data-s="itt">IttStrategy</button>
<button class="sb" data-s="ensemble">EnsembleStrategy</button>
</div>
<div id="main">
<div id="qbar">
<input id="qi" type="text" placeholder="Enter query..." value="what causes cascading failures in distributed systems?">
<button id="rb">Run</button>
</div>
<div id="out"><div class="ev info"><span class="tn">—</span><span class="tx">Select a strategy and click Run.</span></div></div>
<div id="st">Ready · <span id="sl">no strategy selected</span></div>
</div>
<script src="/loops-demo.js"></script>
</body>
</html>"#;

static DEMO_JS: &str = r#"(function(){
'use strict';
var cur=null,src=null;
document.querySelectorAll('.sb').forEach(function(b){
  b.addEventListener('click',function(){
    document.querySelectorAll('.sb').forEach(function(x){x.classList.remove('active');});
    b.classList.add('active');
    cur=b.getAttribute('data-s');
    document.getElementById('sl').textContent=cur;
  });
});
document.getElementById('rb').addEventListener('click',function(){
  if(!cur){alert('Select a strategy first.');return;}
  if(src){src.close();src=null;}
  var q=document.getElementById('qi').value.trim()||'demo query';
  var out=document.getElementById('out');
  out.innerHTML='';
  document.getElementById('rb').disabled=true;
  document.getElementById('st').textContent='Running '+cur+'…';
  var url='/api/loops/demo?strategy='+encodeURIComponent(cur)+'&query='+encodeURIComponent(q);
  var es=new EventSource(url);
  src=es;
  // Tracks the live "tokens" div so each delta appends instead of creating
  // a new <div>. Reset whenever a non-delta event arrives so the next LLM
  // call gets its own running area.
  var liveTx=null;
  es.onmessage=function(e){
    var d;try{d=JSON.parse(e.data);}catch(_){return;}
    if(d.text==='keep-alive')return;
    if(d.type==='delta'){
      if(!liveTx){
        var ldiv=document.createElement('div');
        ldiv.className='ev delta';
        var ltn=document.createElement('span');
        ltn.className='tn';ltn.textContent='⋯';
        liveTx=document.createElement('span');
        liveTx.className='tx';
        ldiv.appendChild(ltn);ldiv.appendChild(liveTx);
        out.appendChild(ldiv);
      }
      liveTx.textContent+=d.text||'';
      out.scrollTop=out.scrollHeight;
      return;
    }
    // Non-delta event ends the current live token group.
    liveTx=null;
    var div=document.createElement('div');
    div.className='ev '+(d.type||'info');
    var tn=document.createElement('span');
    tn.className='tn';
    tn.textContent=d.turn!=null?'#'+d.turn:'—';
    var tx=document.createElement('span');
    tx.className='tx';
    tx.textContent=d.text||'';
    div.appendChild(tn);div.appendChild(tx);
    out.appendChild(div);
    out.scrollTop=out.scrollHeight;
    if(d.type==='done'||d.type==='error'){
      es.close();src=null;
      document.getElementById('rb').disabled=false;
      document.getElementById('st').textContent=(d.type==='done'?'Done':'Error')+' · '+cur;
    }
  };
  es.onerror=function(){
    es.close();src=null;
    document.getElementById('rb').disabled=false;
    document.getElementById('st').textContent='Connection error';
  };
});
}());
"#;
