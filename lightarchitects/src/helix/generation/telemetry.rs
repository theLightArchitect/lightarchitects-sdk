// Provider names appear in prose; suppress doc_markdown noise.
#![allow(clippy::doc_markdown)]

//! AYIN telemetry instrumentation for `helix::generation`.
//!
//! This module provides 100% span coverage for the RAG pipeline:
//!
//! 1. [`SpanInstrumented<C>`] — a zero-overhead wrapper that emits one
//!    `helix.generation.complete` span per `LlmCompleter::complete` call. The
//!    span includes the provider name, model class, token counts, latency,
//!    and outcome — wired into the AYIN lineage graph via
//!    [`crate::ayin::current_span_ctx`] for automatic `parent_id` propagation.
//!
//! 2. [`PipelineSpan`] — a builder that wraps an entire retrieval-augmented
//!    generation pipeline (classify → retrieve → complete) in a single
//!    parent span, with `DecisionPoint`s emitted for intent classification
//!    and strategy selection. Child spans (the wrapped completer's
//!    `complete` span, helix retrieval spans) auto-link via task-local
//!    propagation.
//!
//! # Lineage-graph compatibility
//!
//! AYIN's Lineage Circuit renders spans as a radial dendrogram keyed on
//! `parent_id`. For correct rendering, every emitted span MUST have either
//! `parent_id = None` (root) or a `parent_id` that matches a previously
//! emitted span in the same `session_id`. This module guarantees that
//! invariant by:
//!
//! - Reading `current_span_ctx().parent_id` before emitting `complete` spans.
//! - Setting up a new task-local context inside [`PipelineSpan::run`] so the
//!   wrapped completer sees the pipeline span as its parent.
//! - Forwarding the caller's `session_id` to all emitted spans (groups them
//!   together in the lineage view).
//!
//! Spans are written via [`crate::ayin::write_span_to_disk`] in a background
//! task — the calling code path is never blocked on disk I/O.
//!
//! # Example: instrumenting a complete RAG pipeline
//!
//! ```ignore
//! use lightarchitects::helix::generation::{
//!     KeywordIntentClassifier, IntentClassifier, PromptPolicy,
//!     completer::AnthropicCompleter,
//!     telemetry::{SpanInstrumented, PipelineSpan},
//! };
//!
//! let raw = AnthropicCompleter::from_env("claude-sonnet-4-6")?;
//! let llm = SpanInstrumented::new(raw);          // auto-spans every complete()
//! let classifier = KeywordIntentClassifier;
//!
//! let pipeline = PipelineSpan::start("memory_query")
//!     .session_id("session-abc-123");
//!
//! pipeline.run(|ctx| async move {
//!     let intent = ctx.classify_intent(&classifier, query);
//!     let strategy = ctx.select_strategy(intent, llm.model_class());
//!     let context = match strategy { /* fetch from helix */ };
//!     let policy = PromptPolicy::for_intent(intent);
//!     llm.complete(policy.system_prompt(), &build_user_prompt(policy, &context, query))
//!         .await
//! }).await
//! ```

use std::time::Instant;

use async_trait::async_trait;
use uuid::Uuid;

use super::{ContextStrategy, IntentClassifier, ModelClass, QuestionIntent};
use crate::ayin::semconv::helix_generation as sc;
use crate::ayin::{
    Actor, SpanContext, TraceContext, TraceOutcome, current_span_ctx, default_trace_base, span_dir,
    spawn_with_span_context, with_span_context, write_span_to_disk,
};
use crate::helix::generation::completer::{Completion, CompletionError, LlmCompleter};

/// Pending decision-point record collected by [`PipelineRunCtx`]. Converted
/// to [`crate::ayin::DecisionPoint`] when the pipeline span is finalised.
#[derive(Debug, Clone)]
struct PendingDecision {
    name: &'static str,
    input: String,
    decision: String,
}

// ── Label helpers ────────────────────────────────────────────────────────────

fn model_class_label(mc: ModelClass) -> &'static str {
    match mc {
        ModelClass::Frontier => "frontier",
        ModelClass::MidTier => "mid_tier",
        ModelClass::Cheap => "cheap",
    }
}

fn intent_label(i: QuestionIntent) -> &'static str {
    match i {
        QuestionIntent::Literal => "literal",
        QuestionIntent::Preference => "preference",
        QuestionIntent::Temporal => "temporal",
        QuestionIntent::Counting => "counting",
        QuestionIntent::Abstention => "abstention",
    }
}

fn error_kind_label(e: &CompletionError) -> &'static str {
    match e {
        CompletionError::Http(_) => "http",
        CompletionError::Auth(_) => "auth",
        CompletionError::Empty => "empty",
        CompletionError::Timeout { .. } => "timeout",
        CompletionError::Provider(_) => "provider",
        CompletionError::Serde(_) => "serde",
        CompletionError::MissingCredential(_) => "missing_credential",
    }
}

// ── SpanInstrumented wrapper ─────────────────────────────────────────────────

/// Wraps any [`LlmCompleter`] to emit an AYIN `helix.generation.complete`
/// span around every `complete` call. Drop-in replacement — the wrapper
/// implements [`LlmCompleter`] itself, so callers swap in
/// `SpanInstrumented::new(my_completer)` and downstream code is unchanged.
///
/// # Telemetry shape
///
/// One [`crate::ayin::TraceSpan`] per call with:
///
/// - `action` = `"helix.generation.complete"`
/// - `actor` = `Actor::soul()` (provider sub-identity in metadata)
/// - `parent_id` = taken from [`current_span_ctx`] (auto-links to caller)
/// - `session_id` = taken from [`current_span_ctx`]
/// - `outcome` = [`TraceOutcome::Continue`] on success, [`TraceOutcome::Block`]
///   on error (with `error_kind` in metadata)
/// - `metadata` = `{provider, model, model_class, input_tokens,
///   output_tokens, latency_ms, system_chars, user_chars, output_chars,
///   error_kind?}`
///
/// # Lineage-graph guarantee
///
/// When called inside a [`PipelineSpan::run`] scope, the emitted span's
/// `parent_id` is the pipeline span's id — the lineage circuit will render
/// them as parent and child in the radial dendrogram.
#[derive(Debug, Clone)]
pub struct SpanInstrumented<C: LlmCompleter> {
    inner: C,
}

impl<C: LlmCompleter> SpanInstrumented<C> {
    /// Wrap a completer with span emission. The inner completer is consumed.
    pub const fn new(inner: C) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped completer (e.g. to read `model_class` without
    /// going through the trait).
    pub const fn inner(&self) -> &C {
        &self.inner
    }
}

#[async_trait]
impl<C: LlmCompleter> LlmCompleter for SpanInstrumented<C> {
    fn name(&self) -> String {
        self.inner.name()
    }

    fn model_class(&self) -> ModelClass {
        self.inner.model_class()
    }

    async fn complete(&self, system: &str, user: &str) -> Result<Completion, CompletionError> {
        let ctx = current_span_ctx();
        let provider = self.inner.name();
        let model_class = self.inner.model_class();
        let system_chars = system.chars().count();
        let user_chars = user.chars().count();

        let start = Instant::now();
        let result = self.inner.complete(system, user).await;
        let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

        // Extract metadata snapshot from result without consuming it.
        let outcome = match &result {
            Ok(_) => TraceOutcome::Continue,
            Err(_) => TraceOutcome::Block,
        };
        let (input_tokens, output_tokens, output_chars) = match &result {
            Ok(c) => (c.input_tokens, c.output_tokens, c.text.chars().count()),
            Err(_) => (0, 0, 0),
        };
        let error_kind = result.as_ref().err().map(error_kind_label);

        emit_complete_span(
            ctx,
            provider,
            model_class,
            latency_ms,
            input_tokens,
            output_tokens,
            system_chars,
            user_chars,
            output_chars,
            outcome,
            error_kind,
        );

        result
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_complete_span(
    ctx: SpanContext,
    provider: String,
    model_class: ModelClass,
    latency_ms: u64,
    input_tokens: u32,
    output_tokens: u32,
    system_chars: usize,
    user_chars: usize,
    output_chars: usize,
    outcome: TraceOutcome,
    error_kind: Option<&'static str>,
) {
    spawn_with_span_context(async move {
        // Split "<provider>:<model>" → ("<provider>", "<model>").
        let model = provider
            .split_once(':')
            .map_or(String::new(), |(_, m)| m.to_owned());
        let mut meta = serde_json::json!({
            sc::ATTR_GENERATION_PROVIDER: provider,
            sc::ATTR_GENERATION_MODEL: model,
            sc::ATTR_GENERATION_MODEL_CLASS: model_class_label(model_class),
            sc::ATTR_GENERATION_INPUT_TOKENS: input_tokens,
            sc::ATTR_GENERATION_OUTPUT_TOKENS: output_tokens,
            sc::ATTR_GENERATION_LATENCY_MS: latency_ms,
            sc::ATTR_GENERATION_SYSTEM_CHARS: system_chars,
            sc::ATTR_GENERATION_USER_CHARS: user_chars,
            sc::ATTR_GENERATION_OUTPUT_CHARS: output_chars,
        });
        if let Some(kind) = error_kind {
            meta[sc::ATTR_GENERATION_ERROR_KIND] = serde_json::Value::String(kind.to_owned());
        }

        let mut builder =
            TraceContext::new(Actor::soul(), sc::SPAN_GENERATION_COMPLETE).metadata(meta);
        if let Some(pid) = ctx.parent_id {
            builder = builder.parent(pid);
        }
        if let Some(ref sid) = ctx.session_id {
            builder = builder.session_id(sid);
        }
        builder = builder.outcome(outcome);

        match builder.finish() {
            Ok(span) => {
                let dir = span_dir(
                    &default_trace_base(),
                    Actor::soul().as_str(),
                    &span.timestamp,
                );
                if let Err(e) = write_span_to_disk(&span, &dir).await {
                    tracing::warn!(error = %e, "helix.generation.complete span write failed");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "helix.generation.complete span build failed");
            }
        }
    });
}

// ── PipelineSpan: parent span for full RAG pipelines ─────────────────────────

/// Builder for a parent span wrapping a full retrieval-augmented generation
/// pipeline. The wrapped closure runs inside a task-local context whose
/// `parent_id` is this span's id, so any downstream [`SpanInstrumented`]
/// completer calls and any `soul.helix.retrieve` spans link as children.
///
/// Decision points are captured for the intent classification and strategy
/// selection so the lineage circuit can show them inline on the parent span
/// without needing separate child spans for sub-microsecond operations.
pub struct PipelineSpan {
    span_id: Uuid,
    session_id: Option<String>,
    parent_id: Option<Uuid>,
    action: String,
    start: Instant,
    actor: Actor,
}

impl PipelineSpan {
    /// Begin a new pipeline span with the given `action` (e.g.
    /// `"memory_query"`). The actor defaults to `Actor::soul()`.
    #[must_use]
    pub fn start(action: impl Into<String>) -> Self {
        let ctx = current_span_ctx();
        Self {
            span_id: Uuid::new_v4(),
            session_id: ctx.session_id.clone(),
            parent_id: ctx.parent_id,
            action: action.into(),
            start: Instant::now(),
            actor: Actor::soul(),
        }
    }

    /// Override the default `Actor::soul()` attribution.
    #[must_use]
    pub fn actor(mut self, actor: Actor) -> Self {
        self.actor = actor;
        self
    }

    /// Override the auto-inherited session id.
    #[must_use]
    pub fn session_id(mut self, sid: impl Into<String>) -> Self {
        self.session_id = Some(sid.into());
        self
    }

    /// This span's id — useful when manually nesting child spans.
    #[must_use]
    pub const fn span_id(&self) -> Uuid {
        self.span_id
    }

    /// Run the wrapped closure inside a task-local context whose
    /// `parent_id` is this pipeline span. The closure receives a
    /// [`PipelineRunCtx`] handle for emitting decision points.
    ///
    /// On return, the pipeline span is written to disk in the background.
    pub async fn run<F, Fut, T>(self, f: F) -> T
    where
        F: FnOnce(PipelineRunCtx) -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let pipeline_id = self.span_id;
        let session_id = self.session_id.clone();
        let nested_ctx =
            SpanContext::seeded(session_id.clone().unwrap_or_default(), Some(pipeline_id));
        let run_ctx = PipelineRunCtx::new(pipeline_id);
        let decisions_handle = run_ctx.decisions.clone();

        let result = with_span_context(nested_ctx, f(run_ctx)).await;

        let latency_ms = u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX);
        let collected_decisions = std::sync::Arc::try_unwrap(decisions_handle)
            .map(std::sync::Mutex::into_inner)
            .map(std::result::Result::unwrap_or_default)
            .unwrap_or_default();

        emit_pipeline_span(
            self.span_id,
            self.parent_id,
            session_id,
            self.actor,
            self.action,
            latency_ms,
            collected_decisions,
        );

        result
    }
}

fn emit_pipeline_span(
    span_id: Uuid,
    parent_id: Option<Uuid>,
    session_id: Option<String>,
    actor: Actor,
    action: String,
    latency_ms: u64,
    decisions: Vec<PendingDecision>,
) {
    let actor_str = actor.as_str().to_owned();
    tokio::spawn(async move {
        let mut builder =
            TraceContext::new(actor, &action)
                .with_id(span_id)
                .metadata(serde_json::json!({
                    sc::ATTR_GENERATION_LATENCY_MS: latency_ms,
                }));
        if let Some(pid) = parent_id {
            builder = builder.parent(pid);
        }
        if let Some(sid) = session_id {
            builder = builder.session_id(&sid);
        }
        // Record collected decision points. `decision(...)` takes `self` by
        // value and only fails on out-of-range confidence; we always pass
        // `confidence: None` so the Err branch is statically unreachable.
        // If it ever does fire (future API change), we abandon the emit
        // rather than leave a half-built span.
        //
        // `try_fold` avoids the borrow-checker's "moved in loop" error that
        // would otherwise fire when chaining by-value builder calls inside a
        // plain `for` loop.
        let builder = decisions.into_iter().try_fold(builder, |b, dp| {
            b.decision(dp.name, &dp.input, &dp.decision, None, 0)
                .map_err(|e| {
                    tracing::error!(
                        error = %e,
                        name = dp.name,
                        "TraceContext::decision() rejected confidence=None — \
                         pipeline span abandoned (this should be unreachable)"
                    );
                })
        });
        let builder = match builder {
            Ok(b) => b.outcome(TraceOutcome::Continue),
            Err(()) => return,
        };
        match builder.finish() {
            Ok(span) => {
                let dir = span_dir(&default_trace_base(), &actor_str, &span.timestamp);
                if let Err(e) = write_span_to_disk(&span, &dir).await {
                    tracing::warn!(error = %e, "helix.generation.rag_pipeline span write failed");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "helix.generation.rag_pipeline span build failed");
            }
        }
    });
}

/// Handle exposed inside [`PipelineSpan::run`] for emitting decision points
/// that AYIN renders inline on the parent span.
#[derive(Clone)]
pub struct PipelineRunCtx {
    pipeline_id: Uuid,
    decisions: std::sync::Arc<std::sync::Mutex<Vec<PendingDecision>>>,
}

impl PipelineRunCtx {
    fn new(pipeline_id: Uuid) -> Self {
        Self {
            pipeline_id,
            decisions: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// The parent pipeline span's id (the `parent_id` that child spans will
    /// inherit via task-local).
    #[must_use]
    pub const fn parent_span_id(&self) -> Uuid {
        self.pipeline_id
    }

    /// Classify a query and record the result as a decision point on the
    /// pipeline span. Convenience wrapper around
    /// [`IntentClassifier::classify`] with telemetry.
    #[must_use]
    pub fn classify_intent(
        &self,
        classifier: &impl IntentClassifier,
        query: &str,
    ) -> QuestionIntent {
        let intent = classifier.classify(query);
        // Truncate the query to keep AYIN payloads bounded (see
        // crate::ayin::writer::SPAN_BUDGET_BYTES). 200 chars is plenty for
        // a meaningful decision-point preview.
        let truncated: String = query.chars().take(200).collect();
        let pending = PendingDecision {
            name: sc::DECISION_INTENT_CLASSIFIED,
            input: truncated,
            decision: intent_label(intent).to_owned(),
        };
        if let Ok(mut guard) = self.decisions.lock() {
            guard.push(pending);
        }
        intent
    }

    /// Select a context strategy for the given intent + model class and
    /// record the result as a decision point on the pipeline span.
    #[must_use]
    pub fn select_strategy(
        &self,
        intent: QuestionIntent,
        model_class: ModelClass,
    ) -> ContextStrategy {
        let strategy = super::optimal_strategy_for_intent_with_model_class(intent, model_class);
        let pending = PendingDecision {
            name: sc::DECISION_STRATEGY_SELECTED,
            input: format!(
                "intent={}, model_class={}",
                intent_label(intent),
                model_class_label(model_class)
            ),
            decision: strategy.as_str().to_owned(),
        };
        if let Ok(mut guard) = self.decisions.lock() {
            guard.push(pending);
        }
        strategy
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use crate::helix::generation::KeywordIntentClassifier;

    // A fake completer that just echoes back a canned response and counts calls.
    #[derive(Clone)]
    struct FakeCompleter {
        calls: Arc<AtomicU32>,
    }

    #[async_trait]
    impl LlmCompleter for FakeCompleter {
        fn name(&self) -> String {
            "fake:test-model".to_owned()
        }
        fn model_class(&self) -> ModelClass {
            ModelClass::Frontier
        }
        async fn complete(
            &self,
            _system: &str,
            _user: &str,
        ) -> Result<Completion, CompletionError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(Completion {
                text: "ok".to_owned(),
                input_tokens: 10,
                output_tokens: 2,
                latency_ms: 1,
                provider: self.name(),
            })
        }
    }

    #[tokio::test]
    async fn span_instrumented_delegates_to_inner() {
        let fake = FakeCompleter {
            calls: Arc::new(AtomicU32::new(0)),
        };
        let calls = fake.calls.clone();
        let wrapped = SpanInstrumented::new(fake);
        let result = wrapped.complete("sys", "user").await.unwrap();
        assert_eq!(result.text, "ok");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn span_instrumented_preserves_name_and_class() {
        let fake = FakeCompleter {
            calls: Arc::new(AtomicU32::new(0)),
        };
        let wrapped = SpanInstrumented::new(fake);
        assert_eq!(wrapped.name(), "fake:test-model");
        assert_eq!(wrapped.model_class(), ModelClass::Frontier);
    }

    #[tokio::test]
    async fn pipeline_run_classifies_and_selects_strategy() {
        let pipe = PipelineSpan::start("test_pipeline").session_id("test-session");
        let classifier = KeywordIntentClassifier;
        let result = pipe
            .run(|ctx| async move {
                let intent = ctx.classify_intent(&classifier, "How many things did I buy?");
                let strategy = ctx.select_strategy(intent, ModelClass::Frontier);
                (intent, strategy)
            })
            .await;
        assert_eq!(result.0, QuestionIntent::Counting);
        assert_eq!(result.1, ContextStrategy::FullContext);
    }

    #[tokio::test]
    async fn pipeline_run_propagates_parent_id_to_child_via_task_local() {
        let pipe = PipelineSpan::start("parent");
        let parent_id = pipe.span_id();
        let captured_parent = pipe
            .run(|_ctx| async move { current_span_ctx().parent_id })
            .await;
        assert_eq!(captured_parent, Some(parent_id));
    }
}
