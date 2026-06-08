//! `OffloadAwareProvider<P>` — wraps any [`LlmAgentProvider`] and routes
//! catalog-matched, no-tool-use requests through the [`OffloadDispatcher`]
//! to a cheap-tier specialist model, with full LÆX-supervised retry +
//! HITL escalation.
//!
//! # Routing precedence (first-match-wins)
//!
//! 1. Tool-using turn (`request().tool_definitions` non-empty) → fallthrough
//!    to inner.
//! 2. No `model_hint` set, or hint not in catalog → fallthrough.
//! 3. Pattern declares `starts_with_anchor: true` (e.g. P3) → fallthrough.
//!    `Day 9-10` limitation; `Day 11` `prompt_builder` will extract the rendered
//!    anchor.
//! 4. Unknown sibling identity → fallthrough.
//! 5. Catalog pattern + charter resolved → offload pipeline runs.
//!
//! # Pipeline (when offload is selected)
//!
//! 1. Resolve declared `context_sources` (helix/canon/baseline) via the
//!    registered [`ContextResolver`]s. Any resolver failure is logged-and-
//!    skipped; partial context is OK.
//! 2. Assemble enriched prompt: `{persona}\n\n{charter}\n\n{context_blocks}\n\n{user_prompt}`.
//!    Day 9-10 uses inline concatenation; Day 11 enforces token budgets.
//! 3. Dispatch via [`OffloadDispatcher::dispatch`] → raw model output.
//! 4. Shape-validate via [`ShapeValidator`]; on failure with `pattern.refinement`
//!    set, one refined retry; on second failure → fallthrough.
//! 5. If `pattern.verifier.enabled`, run [`LaexSupervisor::supervise`]:
//!    - `Pass` → return.
//!    - `Hitl` → escalate via [`HitlEscalator`]:
//!      - `Approved` → return `last_output` anyway.
//!      - `Denied` / `NotConfigured` / any error → fallthrough.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use super::catalog::{ContextSource, OffloadCatalog, Pattern};
use super::charter::SiblingCharterRegistry;
use super::context::{ContextResolver, ResolvedContext};
#[cfg(test)]
use super::hitl_bridge::BridgeError;
use super::hitl_bridge::{EscalationRequest, EscalationResolution, HitlEscalator};
use super::laex_supervisor::{
    LaexSupervisor, OffloadDispatcher, SupervisorVerdict, VerifierContext,
};
use super::prompt_builder::{self, BudgetConfig};
use super::refiner::PromptRefiner;
use super::validator::ShapeValidator;
use crate::agent::provider::{
    AgentRequest, AgentResponse, LlmAgentProvider, ProviderCapabilities, ProviderError,
    SanitizedAgentRequest, TokenUsage,
};

/// Closure that maps an [`AgentRequest`] to template-variable values for
/// `{{slot}}` substitution + anchor extraction (Day 11).
///
/// The default constructor [`OffloadAwareProvider::new`] uses a no-op
/// resolver — patterns with `starts_with_anchor: true` (e.g. P3) fall
/// through. Use [`OffloadAwareProvider::new_with_arg_resolver`] to wire
/// a real resolver.
pub type ArgResolver = Arc<dyn Fn(&AgentRequest) -> HashMap<String, String> + Send + Sync>;

/// Wrapper provider that transparently offloads catalog-matched requests.
pub struct OffloadAwareProvider<P: LlmAgentProvider> {
    inner: Arc<P>,
    catalog: Arc<OffloadCatalog>,
    resolvers: HashMap<&'static str, Arc<dyn ContextResolver>>,
    dispatcher: Arc<dyn OffloadDispatcher>,
    supervisor: Arc<LaexSupervisor>,
    escalator: Arc<dyn HitlEscalator>,
    arg_resolver: ArgResolver,
}

impl<P: LlmAgentProvider> OffloadAwareProvider<P> {
    /// Construct with the full component set.
    ///
    /// `resolvers` is keyed by `ContextSource::kind_str()` — i.e.
    /// `"helix"`, `"canon"`, `"industry-baseline"`, `"context7"`. Resolvers
    /// for kinds not present in the map are silently skipped at resolve time.
    pub fn new(
        inner: Arc<P>,
        catalog: Arc<OffloadCatalog>,
        resolvers: HashMap<&'static str, Arc<dyn ContextResolver>>,
        dispatcher: Arc<dyn OffloadDispatcher>,
        supervisor: Arc<LaexSupervisor>,
        escalator: Arc<dyn HitlEscalator>,
    ) -> Self {
        Self::new_with_arg_resolver(
            inner,
            catalog,
            resolvers,
            dispatcher,
            supervisor,
            escalator,
            Arc::new(|_req| HashMap::new()),
        )
    }

    /// Construct with a custom [`ArgResolver`] that extracts template
    /// variables from each request — enables P3 (`starts_with_anchor: true`)
    /// support.
    pub fn new_with_arg_resolver(
        inner: Arc<P>,
        catalog: Arc<OffloadCatalog>,
        resolvers: HashMap<&'static str, Arc<dyn ContextResolver>>,
        dispatcher: Arc<dyn OffloadDispatcher>,
        supervisor: Arc<LaexSupervisor>,
        escalator: Arc<dyn HitlEscalator>,
        arg_resolver: ArgResolver,
    ) -> Self {
        Self {
            inner,
            catalog,
            resolvers,
            dispatcher,
            supervisor,
            escalator,
            arg_resolver,
        }
    }

    /// Decide whether to offload this request. Returns the matched pattern
    /// when offload should proceed, or `None` to signal fallthrough.
    fn try_classify<'a>(&'a self, req: &'a SanitizedAgentRequest) -> Option<&'a Pattern> {
        let raw = req.request();
        if !raw.tool_definitions.is_empty() {
            return None;
        }
        let hint = raw.model_hint.as_deref()?;
        let p = self.catalog.get(hint)?;
        if p.role.as_deref() == Some("verifier") {
            return None;
        }
        if !p
            .eligible
            .siblings
            .iter()
            .any(|s| s == raw.sibling_identity.as_str())
        {
            return None;
        }
        // Day 11: P3 (starts_with_anchor) is now supported when the
        // configured arg_resolver provides template-variable values.
        Some(p)
    }

    async fn resolve_context_blocks(
        &self,
        pattern: &Pattern,
        sibling: &str,
    ) -> Vec<ResolvedContext> {
        let Some(overlay) = pattern.context_sources.as_ref() else {
            return Vec::new();
        };
        let sources: &[ContextSource] = overlay
            .overrides
            .get(sibling)
            .map_or(overlay.default.as_slice(), |o| o.as_slice());
        let mut out = Vec::with_capacity(sources.len());
        for source in sources {
            let Some(resolver) = self.resolvers.get(source.kind_str()) else {
                continue;
            };
            match resolver.resolve(source, sibling).await {
                Ok(rc) => out.push(rc),
                Err(_e) => {
                    // Resolver failures are non-fatal — partial context is
                    // acceptable. Day 14 wires this into AYIN telemetry.
                }
            }
        }
        out
    }

    fn verifier_context_from_blocks(blocks: &[ResolvedContext]) -> VerifierContext {
        let canon = blocks
            .iter()
            .filter(|b| b.kind == "canon")
            .map(|b| b.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let baseline = blocks
            .iter()
            .filter(|b| b.kind == "industry-baseline")
            .map(|b| b.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        VerifierContext {
            canon_excerpts: canon,
            baseline_excerpts: baseline,
        }
    }

    fn build_response(output: String, pattern_id: &str, retries: u8) -> AgentResponse {
        let input_estimate = u32::try_from(output.len() / 4).unwrap_or(u32::MAX);
        let mut attrs = HashMap::new();
        attrs.insert("offload.pattern_id".to_owned(), Value::from(pattern_id));
        attrs.insert("offload.path".to_owned(), Value::from("primary"));
        AgentResponse {
            output: Value::String(output),
            turns_used: 1,
            cost_usd: 0.0,
            tokens: TokenUsage {
                input: input_estimate,
                output: input_estimate,
            },
            provider_attrs: attrs,
            retry_count: retries,
        }
    }

    fn resolve_anchor(pattern: &Pattern, vars: &HashMap<String, String>) -> Option<String> {
        prompt_builder::extract_rendered_anchor(pattern, vars)
    }

    fn render_prompt(
        pattern: &Pattern,
        charter: &super::charter::SiblingCharter,
        blocks: &[ResolvedContext],
        user_prompt: &str,
    ) -> Option<String> {
        let budgets = BudgetConfig::from_pattern(pattern);
        prompt_builder::assemble(
            charter.persona,
            charter.charter,
            blocks,
            user_prompt,
            &budgets,
        )
        .ok()
        .map(|a| a.rendered)
    }

    /// Run the offload pipeline. Returns `Ok(Some(response))` on success,
    /// `Ok(None)` on any reason to fall through, `Err(_)` only when the
    /// pipeline produces an unrecoverable error worth surfacing instead of
    /// silently falling back.
    async fn try_offload(
        &self,
        req: &SanitizedAgentRequest,
    ) -> Result<Option<AgentResponse>, ProviderError> {
        let Some(pattern) = self.try_classify(req) else {
            return Ok(None);
        };
        let sibling = req.request().sibling_identity.as_str();
        let Some(charter) = SiblingCharterRegistry::resolve(sibling) else {
            return Ok(None);
        };

        let blocks = self.resolve_context_blocks(pattern, sibling).await;
        let user_prompt = req.safe_prompt();

        let vars = (self.arg_resolver)(req.request());
        let starts_with_anchor = Self::resolve_anchor(pattern, &vars);
        if pattern.shape.starts_with_anchor == Some(true) && starts_with_anchor.is_none() {
            return Ok(None);
        }
        let Some(rendered) = Self::render_prompt(pattern, charter, &blocks, user_prompt) else {
            return Ok(None);
        };

        // First dispatch.
        let Ok(primary_output) = self.dispatcher.dispatch(pattern, &rendered).await else {
            return Ok(None);
        };

        // Shape validate (with rendered anchor when pattern declares one).
        let (final_output, retry_count) = match ShapeValidator::validate(
            &primary_output,
            &pattern.shape,
            starts_with_anchor.as_deref(),
        ) {
            Ok(()) => (primary_output, 0_u8),
            Err(viol) => {
                let Some(refinement) = pattern.refinement.as_ref() else {
                    return Ok(None);
                };
                let refined =
                    PromptRefiner::refine_after_shape_failure(&rendered, refinement, &viol);
                let Ok(retried) = self.dispatcher.dispatch(pattern, &refined).await else {
                    return Ok(None);
                };
                if ShapeValidator::validate(&retried, &pattern.shape, starts_with_anchor.as_deref())
                    .is_err()
                {
                    return Ok(None);
                }
                (retried, 1_u8)
            }
        };

        // If no verifier configured, ship the shape-valid output.
        let verifier_active = pattern
            .verifier
            .as_ref()
            .is_some_and(|v| v.enabled && v.pattern.is_some());
        if !verifier_active {
            return Ok(Some(Self::build_response(
                final_output,
                &pattern.id,
                retry_count,
            )));
        }

        // LÆX supervision loop.
        let ctx = Self::verifier_context_from_blocks(&blocks);
        match self
            .supervisor
            .supervise(pattern, final_output.clone(), &rendered, &ctx)
            .await
        {
            Ok(SupervisorVerdict::Pass { output }) => {
                Ok(Some(Self::build_response(output, &pattern.id, retry_count)))
            }
            Ok(SupervisorVerdict::Hitl {
                reason,
                last_output,
                last_amendment_hint,
            }) => {
                let task_id = req
                    .request()
                    .chain_origin
                    .clone()
                    .unwrap_or_else(|| pattern.id.clone());
                let traceparent = req.request().parent_span_id.clone();
                let escalation = EscalationRequest {
                    task_id,
                    reason,
                    last_output: last_output.clone(),
                    last_amendment_hint,
                    traceparent,
                };
                match self.escalator.escalate(escalation).await {
                    Ok(EscalationResolution::Approved { .. }) => Ok(Some(Self::build_response(
                        last_output,
                        &pattern.id,
                        retry_count,
                    ))),
                    Ok(EscalationResolution::Denied { .. }) | Err(_) => Ok(None),
                }
            }
            Err(_) => Ok(None),
        }
    }
}

#[async_trait]
impl<P: LlmAgentProvider> LlmAgentProvider for OffloadAwareProvider<P> {
    fn name(&self) -> &'static str {
        "offload-aware"
    }

    async fn spawn(&self, req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
        match self.try_offload(&req).await? {
            Some(resp) => Ok(resp),
            None => self.inner.spawn(req).await,
        }
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.inner.capabilities()
    }

    fn estimate_cost(&self, input_tokens: u32, max_output_tokens: u32) -> f64 {
        self.inner.estimate_cost(input_tokens, max_output_tokens)
    }
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

    use serde_json::json;

    use super::super::catalog::{Calibration, Eligibility, Refinement, Shape, Verifier};
    use super::*;

    // ─── Mock provider ────────────────────────────────────────────────────

    struct MockProvider {
        canned: Mutex<VecDeque<Result<AgentResponse, ProviderError>>>,
        calls: Mutex<u32>,
    }

    impl MockProvider {
        fn new(canned: Vec<Result<AgentResponse, ProviderError>>) -> Self {
            Self {
                canned: Mutex::new(canned.into()),
                calls: Mutex::new(0),
            }
        }
        fn call_count(&self) -> u32 {
            *self.calls.lock().unwrap()
        }
    }

    #[async_trait]
    impl LlmAgentProvider for MockProvider {
        fn name(&self) -> &'static str {
            "mock-inner"
        }
        async fn spawn(&self, _req: SanitizedAgentRequest) -> Result<AgentResponse, ProviderError> {
            *self.calls.lock().unwrap() += 1;
            self.canned
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err(ProviderError::Internal("mock exhausted".to_owned())))
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                schema_enforcement: crate::agent::provider::SchemaMode::None,
                native_budget_cap: false,
                native_turn_cap: false,
                auth_inherits_session: false,
            }
        }
        fn estimate_cost(&self, _i: u32, _o: u32) -> f64 {
            0.0
        }
    }

    // ─── Mock dispatcher ──────────────────────────────────────────────────

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
                .unwrap_or_else(|| Err("dispatcher exhausted".to_owned()))
        }
    }

    // ─── Fixtures ─────────────────────────────────────────────────────────

    fn pattern_p1_no_verifier() -> Pattern {
        Pattern {
            id: "P1".to_owned(),
            name: "Explain code".to_owned(),
            role: None,
            template: "Explain this".to_owned(),
            eligible: Eligibility {
                siblings: vec!["claude".to_owned(), "corso".to_owned()],
                tool_use_required: false,
                max_input_tokens: 4000,
            },
            context_sources: None,
            shape: Shape {
                kind: "sentence_no_fences".to_owned(),
                max_words: Some(50),
                forbidden_substrings: Some(vec!["```".to_owned()]),
                required_keys: None,
                verdict_enum: None,
                starts_with_anchor: None,
            },
            refinement: Some(Refinement {
                anchor: "Single sentence. No fence.".to_owned(),
            }),
            verifier: None,
            calibration: Calibration {
                last_dry_run: None,
                sample_count: None,
                success_rate: None,
            },
        }
    }

    fn pattern_p3_with_anchor() -> Pattern {
        let mut p = pattern_p1_no_verifier();
        p.id = "P3".to_owned();
        p.shape.starts_with_anchor = Some(true);
        p
    }

    fn pattern_with_verifier(verifier_id: &str) -> Pattern {
        let mut p = pattern_p1_no_verifier();
        p.verifier = Some(Verifier {
            enabled: true,
            pattern: Some(verifier_id.to_owned()),
            escalate_on_fail: Some("AUTO_RETRY".to_owned()),
            max_auto_retries: 1,
        });
        p
    }

    fn verifier_pattern() -> Pattern {
        Pattern {
            id: "PV_test".to_owned(),
            name: "verify".to_owned(),
            role: Some("verifier".to_owned()),
            template: "Vet {{primary_output}}".to_owned(),
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

    fn catalog_with(patterns: Vec<Pattern>) -> Arc<OffloadCatalog> {
        Arc::new(OffloadCatalog {
            version: "1.1".to_owned(),
            last_calibrated: None,
            default_model: None,
            patterns,
        })
    }

    fn sanitized_req(
        sibling: &str,
        prompt: &str,
        model_hint: Option<&str>,
        tools: bool,
    ) -> SanitizedAgentRequest {
        let mut raw = crate::agent::provider::AgentRequest {
            sibling_identity: sibling.to_owned(),
            user_prompt: prompt.to_owned(),
            schema: None,
            allowed_tools: vec![],
            max_turns: 1,
            max_budget_usd: 0.10,
            model_hint: model_hint.map(str::to_owned),
            parent_span_id: None,
            chain_origin: Some("test-origin".to_owned()),
            chain_depth: 0,
            aud: None,
            conversation_history: vec![],
            tool_definitions: vec![],
        };
        if tools {
            raw.tool_definitions
                .push(crate::agent::tool_executor::ToolDefinition {
                    name: "test_tool".to_owned(),
                    description: String::new(),
                    input_schema: serde_json::json!({}),
                });
        }
        raw.sanitize().unwrap()
    }

    fn build_provider(
        inner_canned: Vec<Result<AgentResponse, ProviderError>>,
        dispatch_canned: Vec<Result<String, String>>,
        patterns: Vec<Pattern>,
        escalator: Arc<dyn HitlEscalator>,
    ) -> (
        OffloadAwareProvider<MockProvider>,
        Arc<MockProvider>,
        Arc<MockDispatcher>,
    ) {
        let inner = Arc::new(MockProvider::new(inner_canned));
        let cat = catalog_with(patterns);
        let disp = Arc::new(MockDispatcher::new(dispatch_canned));
        let supervisor = Arc::new(LaexSupervisor::new(cat.clone(), disp.clone()));
        let provider = OffloadAwareProvider::new(
            inner.clone(),
            cat,
            HashMap::new(),
            disp.clone(),
            supervisor,
            escalator,
        );
        (provider, inner, disp)
    }

    #[allow(clippy::unnecessary_wraps)]
    fn canned_inner_ok(text: &str) -> Result<AgentResponse, ProviderError> {
        Ok(AgentResponse {
            output: Value::String(text.to_owned()),
            turns_used: 1,
            cost_usd: 0.0,
            tokens: TokenUsage {
                input: 0,
                output: 0,
            },
            provider_attrs: HashMap::new(),
            retry_count: 0,
        })
    }

    // ─── Tests ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn passthrough_when_no_pattern_matches() {
        let (provider, inner, disp) = build_provider(
            vec![canned_inner_ok("from inner")],
            vec![],
            vec![pattern_p1_no_verifier()],
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        let req = sanitized_req("claude", "hi", None, false);
        let resp = provider.spawn(req).await.unwrap();
        assert_eq!(resp.output, Value::String("from inner".to_owned()));
        assert_eq!(inner.call_count(), 1);
        assert_eq!(disp.calls().len(), 0);
    }

    #[tokio::test]
    async fn passthrough_when_tool_use_present() {
        let (provider, inner, disp) = build_provider(
            vec![canned_inner_ok("from inner")],
            vec![],
            vec![pattern_p1_no_verifier()],
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        let req = sanitized_req("claude", "hi", Some("P1"), true);
        let _ = provider.spawn(req).await.unwrap();
        assert_eq!(inner.call_count(), 1);
        assert_eq!(disp.calls().len(), 0);
    }

    #[tokio::test]
    async fn passthrough_when_charter_missing() {
        let (provider, inner, _) = build_provider(
            vec![canned_inner_ok("inner")],
            vec![],
            vec![pattern_p1_no_verifier()],
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        // "unknown_sibling" is not eligible for P1 either, so this falls through twice.
        let req = sanitized_req("unknown_sibling", "hi", Some("P1"), false);
        let _ = provider.spawn(req).await.unwrap();
        assert_eq!(inner.call_count(), 1);
    }

    #[tokio::test]
    async fn passthrough_when_pattern_has_starts_with_anchor() {
        let (provider, inner, disp) = build_provider(
            vec![canned_inner_ok("inner")],
            vec![],
            vec![pattern_p3_with_anchor()],
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        let req = sanitized_req("claude", "hi", Some("P3"), false);
        let _ = provider.spawn(req).await.unwrap();
        assert_eq!(inner.call_count(), 1);
        assert_eq!(disp.calls().len(), 0);
    }

    #[tokio::test]
    async fn passthrough_when_dispatcher_errors() {
        let (provider, inner, disp) = build_provider(
            vec![canned_inner_ok("inner")],
            vec![Err("network down".to_owned())],
            vec![pattern_p1_no_verifier()],
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        let req = sanitized_req("claude", "hi", Some("P1"), false);
        let resp = provider.spawn(req).await.unwrap();
        assert_eq!(resp.output, Value::String("inner".to_owned()));
        assert_eq!(inner.call_count(), 1);
        assert_eq!(disp.calls().len(), 1);
    }

    #[tokio::test]
    async fn offload_success_no_verifier_returns_pass_response() {
        let (provider, inner, disp) = build_provider(
            vec![],
            vec![Ok("Returns n clamped.".to_owned())],
            vec![pattern_p1_no_verifier()],
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        let req = sanitized_req("claude", "explain clamp", Some("P1"), false);
        let resp = provider.spawn(req).await.unwrap();
        assert_eq!(resp.output, Value::String("Returns n clamped.".to_owned()));
        assert_eq!(inner.call_count(), 0);
        assert_eq!(disp.calls().len(), 1);
        assert_eq!(
            resp.provider_attrs.get("offload.pattern_id"),
            Some(&json!("P1"))
        );
    }

    #[tokio::test]
    async fn offload_shape_failure_then_retry_success() {
        let (provider, inner, disp) = build_provider(
            vec![],
            vec![
                Ok("```js\nbad\n```".to_owned()),
                Ok("Clean sentence.".to_owned()),
            ],
            vec![pattern_p1_no_verifier()],
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        let req = sanitized_req("claude", "explain", Some("P1"), false);
        let resp = provider.spawn(req).await.unwrap();
        assert_eq!(resp.output, Value::String("Clean sentence.".to_owned()));
        assert_eq!(resp.retry_count, 1);
        assert_eq!(disp.calls().len(), 2);
        assert_eq!(inner.call_count(), 0);
    }

    #[tokio::test]
    async fn offload_shape_failure_after_retry_falls_through() {
        let (provider, inner, disp) = build_provider(
            vec![canned_inner_ok("inner")],
            vec![
                Ok("```js\nbad\n```".to_owned()),
                Ok("```still fenced```".to_owned()),
            ],
            vec![pattern_p1_no_verifier()],
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        let req = sanitized_req("claude", "explain", Some("P1"), false);
        let resp = provider.spawn(req).await.unwrap();
        assert_eq!(resp.output, Value::String("inner".to_owned()));
        assert_eq!(disp.calls().len(), 2);
        assert_eq!(inner.call_count(), 1);
    }

    #[tokio::test]
    async fn offload_verifier_pass_path() {
        let (provider, inner, disp) = build_provider(
            vec![],
            vec![
                // 1. Primary dispatch
                Ok("Clean output.".to_owned()),
                // 2. Verifier call → PASS
                Ok(r#"{"verdict":"PASS","reason":"ok","amendment_hint":null}"#.to_owned()),
            ],
            vec![pattern_with_verifier("PV_test"), verifier_pattern()],
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        let req = sanitized_req("claude", "p", Some("P1"), false);
        let resp = provider.spawn(req).await.unwrap();
        assert_eq!(resp.output, Value::String("Clean output.".to_owned()));
        assert_eq!(disp.calls().len(), 2);
        assert_eq!(inner.call_count(), 0);
    }

    #[tokio::test]
    async fn offload_verifier_hitl_null_escalator_falls_through() {
        let (provider, inner, _) = build_provider(
            vec![canned_inner_ok("inner")],
            vec![
                Ok("Clean output.".to_owned()),
                Ok(
                    r#"{"verdict":"HITL","reason":"canon violation","amendment_hint":null}"#
                        .to_owned(),
                ),
            ],
            vec![pattern_with_verifier("PV_test"), verifier_pattern()],
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        let req = sanitized_req("claude", "p", Some("P1"), false);
        let resp = provider.spawn(req).await.unwrap();
        assert_eq!(resp.output, Value::String("inner".to_owned()));
        assert_eq!(inner.call_count(), 1);
    }

    /// Stub escalator that returns a canned resolution.
    struct StubEscalator(EscalationResolution);

    #[async_trait]
    impl HitlEscalator for StubEscalator {
        async fn escalate(
            &self,
            _req: EscalationRequest,
        ) -> Result<EscalationResolution, BridgeError> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn offload_verifier_hitl_with_approver_returns_output() {
        let approver = Arc::new(StubEscalator(EscalationResolution::Approved {
            citation: Some("operator override".to_owned()),
        }));
        let (provider, inner, _) = build_provider(
            vec![],
            vec![
                Ok("Maybe-questionable output.".to_owned()),
                Ok(r#"{"verdict":"HITL","reason":"borderline","amendment_hint":null}"#.to_owned()),
            ],
            vec![pattern_with_verifier("PV_test"), verifier_pattern()],
            approver,
        );
        let req = sanitized_req("claude", "p", Some("P1"), false);
        let resp = provider.spawn(req).await.unwrap();
        assert_eq!(
            resp.output,
            Value::String("Maybe-questionable output.".to_owned())
        );
        assert_eq!(inner.call_count(), 0);
    }

    #[tokio::test]
    async fn offload_verifier_hitl_with_denial_falls_through() {
        let denier = Arc::new(StubEscalator(EscalationResolution::Denied {
            reason: "operator rejected".to_owned(),
        }));
        let (provider, inner, _) = build_provider(
            vec![canned_inner_ok("inner")],
            vec![
                Ok("bad output".to_owned()),
                Ok(r#"{"verdict":"HITL","reason":"banned","amendment_hint":null}"#.to_owned()),
            ],
            vec![pattern_with_verifier("PV_test"), verifier_pattern()],
            denier,
        );
        let req = sanitized_req("claude", "p", Some("P1"), false);
        let resp = provider.spawn(req).await.unwrap();
        assert_eq!(resp.output, Value::String("inner".to_owned()));
        assert_eq!(inner.call_count(), 1);
    }

    /// End-to-end wiring smoke against the real catalog at $HELIX.
    ///
    /// Verifies the full chain compiles + runs:
    ///   YAML load → classify → charter resolve → `assemble_prompt` → dispatch → validate → Pass.
    ///
    /// Skipped silently when the helix mount is absent.
    #[tokio::test]
    async fn day9_10_end_to_end_wiring_against_real_catalog() {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let Some(home) = home else { return };
        let yaml = home
            .join("lightarchitects")
            .join("soul")
            .join("helix")
            .join("user")
            .join("standards")
            .join("offload-catalog.yaml");
        if !yaml.exists() {
            return;
        }
        let catalog = Arc::new(OffloadCatalog::load_from_path(&yaml).unwrap());
        // Wire a mock dispatcher that returns a clean P1-shaped response.
        let mock = Arc::new(MockDispatcher::new(vec![Ok(
            "Returns n clamped to [lo, hi].".to_owned(),
        )]));
        let supervisor = Arc::new(LaexSupervisor::new(catalog.clone(), mock.clone()));
        let provider = OffloadAwareProvider::new(
            Arc::new(MockProvider::new(vec![])),
            catalog,
            HashMap::new(), // no resolvers — partial-context path
            mock.clone(),
            supervisor,
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        // P1 eligible siblings are claude/corso/quantum/eva — pick claude.
        let req = sanitized_req("claude", "Explain clamp(n, lo, hi)", Some("P1"), false);
        let resp = provider.spawn(req).await.unwrap();
        assert_eq!(
            resp.output,
            Value::String("Returns n clamped to [lo, hi].".to_owned()),
            "real catalog P1 wiring must produce dispatcher output"
        );
        assert_eq!(
            resp.provider_attrs.get("offload.pattern_id"),
            Some(&json!("P1"))
        );
        // Dispatcher MUST have been called exactly once (no verifier on P1).
        assert_eq!(mock.calls().len(), 1);
    }

    #[tokio::test]
    async fn from_catalog_uses_default_model_when_env_unset() {
        // Only run the assertion when env var is unset — Rust 2024 deny(unsafe)
        // forbids env mutation, so we read-only check.
        if std::env::var("LA_LITELLM_MODEL").is_ok() {
            return;
        }
        let cat = Arc::new(OffloadCatalog {
            version: "1.1".to_owned(),
            last_calibrated: None,
            default_model: Some("test-model-from-catalog".to_owned()),
            patterns: vec![],
        });
        let d = super::super::dispatch::LiteLLMHttpDispatcher::from_catalog(&cat).unwrap();
        assert_eq!(d.model(), "test-model-from-catalog");
    }

    /// Day 11: P3 (`starts_with_anchor`) is now offloadable when an
    /// `arg_resolver` provides `{{lang_kw}}` + `{{name}}` values.
    #[tokio::test]
    async fn p3_with_arg_resolver_offloads_when_anchor_matches() {
        let mut p3 = pattern_p3_with_anchor();
        // Give P3 a refinement.anchor template the wrapper can render.
        p3.refinement = Some(super::super::catalog::Refinement {
            anchor: "RESPOND starting with `{{lang_kw}} {{name}}(`. NO backticks.".to_owned(),
        });
        let inner = Arc::new(MockProvider::new(vec![]));
        let cat = catalog_with(vec![p3.clone()]);
        let disp = Arc::new(MockDispatcher::new(vec![Ok(
            "function clamp(n, lo, hi) { return Math.min(hi, Math.max(lo, n)); }".to_owned(),
        )]));
        let supervisor = Arc::new(LaexSupervisor::new(cat.clone(), disp.clone()));
        let resolver: super::ArgResolver = Arc::new(|_req| {
            let mut h = HashMap::new();
            h.insert("lang_kw".to_owned(), "function".to_owned());
            h.insert("name".to_owned(), "clamp".to_owned());
            h
        });
        let provider = OffloadAwareProvider::new_with_arg_resolver(
            inner.clone(),
            cat,
            HashMap::new(),
            disp.clone(),
            supervisor,
            Arc::new(super::super::hitl_bridge::NullEscalator),
            resolver,
        );
        let req = sanitized_req("claude", "Write clamp", Some("P3"), false);
        let resp = provider.spawn(req).await.unwrap();
        assert!(resp.output.as_str().unwrap().starts_with("function clamp("));
        assert_eq!(inner.call_count(), 0);
        assert_eq!(disp.calls().len(), 1);
    }

    /// Day 11: P3 still falls through with the default no-op resolver
    /// (callers must opt in via `new_with_arg_resolver`).
    #[tokio::test]
    async fn p3_with_default_resolver_falls_through() {
        let mut p3 = pattern_p3_with_anchor();
        p3.refinement = Some(super::super::catalog::Refinement {
            anchor: "RESPOND starting with `{{lang_kw}} {{name}}(`. NO backticks.".to_owned(),
        });
        let (provider, inner, disp) = build_provider(
            vec![canned_inner_ok("inner")],
            vec![],
            vec![p3],
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        let req = sanitized_req("claude", "Write clamp", Some("P3"), false);
        let _ = provider.spawn(req).await.unwrap();
        assert_eq!(inner.call_count(), 1);
        assert_eq!(disp.calls().len(), 0);
    }

    #[test]
    fn name_is_offload_aware() {
        let inner = Arc::new(MockProvider::new(vec![]));
        let cat = catalog_with(vec![]);
        let disp = Arc::new(MockDispatcher::new(vec![]));
        let supervisor = Arc::new(LaexSupervisor::new(cat.clone(), disp.clone()));
        let p = OffloadAwareProvider::new(
            inner,
            cat,
            HashMap::new(),
            disp,
            supervisor,
            Arc::new(super::super::hitl_bridge::NullEscalator),
        );
        assert_eq!(p.name(), "offload-aware");
    }
}
