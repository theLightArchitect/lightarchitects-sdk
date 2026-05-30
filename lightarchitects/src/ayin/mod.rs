//! Feature-gated AYIN observability wrapper for lightarchitects-sdk transports.
//!
//! `lightarchitects-ayin` wraps any [`lightarchitects::core::transport::Transport`] in an
//! [`ObservableTransport`] that optionally records a `TraceSpan` for every
//! MCP tool call.
//!
//! # Feature flag
//!
//! Instrumentation is **compile-time opt-in** via the `observe` Cargo feature.
//! Without it, [`ObservableTransport<T>`] is a zero-cost newtype — the only
//! overhead is one extra function call that the compiler optimises away.
//!
//! ```toml
//! # No tracing (default — zero cost)
//! lightarchitects-ayin = { path = "../lightarchitects-ayin" }
//!
//! # Enable AYIN span recording
//! lightarchitects-ayin = { path = "../lightarchitects-ayin", features = ["observe"] }
//! ```
//!
//! # Usage
//!
//! ```no_run
//! use lightarchitects::ayin::ObservableTransport;
//! use lightarchitects::core::StdioTransport;
//!
//! # async fn example() -> Result<(), lightarchitects::core::SdkError> {
//! // Works identically with or without the `observe` feature.
//! // When `observe` is active, every send() writes a TraceSpan to AYIN.
//! let transport: ObservableTransport<StdioTransport> =
//!     ObservableTransport::new(todo!("inner transport"));
//! # Ok(()) }
//! ```
//!
/// Trace span types and builder — [`TraceSpan`], [`Actor`], [`TraceContext`].
pub mod span;

/// LASDLC semantic-convention attribute and span name constants.
pub mod semconv;

/// Session ID and W3C Trace Context propagation across MCP call chains.
pub mod propagation;
pub use propagation::{
    PropagationContext, SESSION_PROPAGATION_KEY, TRACEPARENT_KEY, TRACESTATE_KEY,
    validate_traceparent, validate_tracestate,
};
pub use span::{
    Actor, DecisionPoint, Sibling, StrandActivation, TraceContext, TraceOutcome, TraceSpan,
};

/// Error types for the trace engine.
pub mod error;
pub use error::TraceError;

/// Canonical AYIN action enum — observability sessions, spans, conversations.
pub mod actions;
pub use actions::AyinAction;

/// Conversation-level JSONL tracing with automatic pivot detection.
///
/// [`CognitivePhase`] is always available (no feature gate). [`ConversationTracer`]
/// and related types are noop stubs unless the `conversations` feature is enabled.
pub mod conversation;
pub use conversation::CognitivePhase;
pub use conversation::ConversationTracer;

#[cfg(feature = "conversations")]
pub use conversation::{ConversationError, PivotCheckResult, PivotRecord, PivotState, ToolRecord};

// ── HTTP client (feature-gated) ───────────────────────────────────────────────

#[cfg(feature = "http-client")]
mod client;
#[cfg(feature = "http-client")]
pub use client::{AyinClient, SessionEntry, SessionList, SpanList, SpanRecord};

use crate::core::error::SdkError;
use crate::core::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
use crate::core::transport::Transport;

// ── Shared writer: atomic disk write + task-local propagation ─────────────────

/// Atomic span writer and task-local context propagation helpers.
///
/// All three LA binaries (gateway, webshell, CLI) share this module so span
/// durability semantics and propagation patterns are uniform with zero
/// duplication.
#[cfg(feature = "observe")]
pub mod writer;

#[cfg(feature = "observe")]
pub use writer::{
    DynSpanEmitter, FileSpanEmitter, SPAN_CTX, SpanContext, SpanEmitError, current_span_ctx,
    default_trace_base, span_dir, spawn_with_span_context, with_span_context, write_span_to_disk,
};

// ── Feature-enabled: full AYIN instrumentation ────────────────────────────────

#[cfg(feature = "observe")]
mod observe_impl {
    use std::sync::Arc;

    use la_ayinspan::observe::SpanObserve;
    use la_ayinspan::turn::TurnContext;

    use crate::ayin::span::{Actor, TraceContext, TraceOutcome, TraceSpan};

    use super::writer::{DynSpanEmitter, FileSpanEmitter};
    use super::{JsonRpcRequest, JsonRpcResponse, SdkError, Transport};

    /// Transport wrapper that records an AYIN [`TraceSpan`] for every MCP call.
    ///
    /// Implements [`SpanObserve`]: `on_action_start` is a no-op (reserved for
    /// future pre-call telemetry); `on_action_finish` clones the span and fires
    /// `emitter.emit` on a blocking thread so async callers are never stalled.
    ///
    /// Construct via [`ObservableTransport::new`] (uses [`FileSpanEmitter`]) or
    /// [`ObservableTransport::with_emitter`] for a custom backend.
    pub struct ObservableTransport<T: Transport> {
        inner: T,
        actor: Actor,
        emitter: Arc<DynSpanEmitter>,
    }

    impl<T: Transport> ObservableTransport<T> {
        /// Wrap `inner` using the default [`FileSpanEmitter`] at the canonical trace dir.
        #[must_use]
        pub fn new(inner: T) -> Self {
            Self {
                inner,
                actor: Actor::new("lightarchitects-sdk"),
                emitter: Arc::from(FileSpanEmitter::with_default_base().into_dyn()),
            }
        }

        /// Wrap `inner` with a custom actor name and the default [`FileSpanEmitter`].
        #[must_use]
        pub fn with_actor(inner: T, actor: impl Into<String>) -> Self {
            Self {
                inner,
                actor: Actor::new(actor),
                emitter: Arc::from(FileSpanEmitter::with_default_base().into_dyn()),
            }
        }

        /// Wrap `inner` with a fully custom emitter backend.
        #[must_use]
        pub fn with_emitter(inner: T, emitter: Arc<DynSpanEmitter>) -> Self {
            Self {
                inner,
                actor: Actor::new("lightarchitects-sdk"),
                emitter,
            }
        }
    }

    impl<T: Transport> SpanObserve for ObservableTransport<T> {
        fn on_action_start(&self, _actor: &Actor, _action: &str, _ctx: Option<&TurnContext>) {
            // Reserved for pre-call telemetry (timing, rate-limit counters).
        }

        fn on_action_finish(&self, span: &TraceSpan) {
            // TraceSpan == la_ayinspan::TraceSpan (same type via re-export in span.rs).
            let span = span.clone();
            let emitter = Arc::clone(&self.emitter);
            super::writer::spawn_with_span_context(async move {
                let result = tokio::task::spawn_blocking(move || emitter.emit(span)).await;
                match result {
                    Ok(Err(e)) => {
                        tracing::warn!(error = %e, "AYIN ObservableTransport span emit failed");
                    }
                    Err(e) => tracing::warn!(error = %e, "AYIN span emit task panicked"),
                    Ok(Ok(())) => {}
                }
            });
        }
    }

    impl<T: Transport> Transport for ObservableTransport<T> {
        async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, SdkError> {
            let action = request.method.clone();
            self.on_action_start(&self.actor, &action, None);

            let result = self.inner.send(request).await;

            let outcome = match &result {
                Ok(_) => TraceOutcome::Continue,
                Err(e) => TraceOutcome::Error(e.to_string()),
            };

            if let Ok(span) = TraceContext::new(self.actor.clone(), &action)
                .outcome(outcome)
                .finish()
            {
                self.on_action_finish(&span);
            }

            result
        }
    }
}

#[cfg(feature = "observe")]
pub use observe_impl::ObservableTransport;

/// Emit a finished [`TraceSpan`] asynchronously (fire-and-forget).
///
/// Writes to `~/lightarchitects/soul/helix/ayin/traces/{actor}/{YYYY-MM-DD}/...json`
/// via [`writer::write_span_to_disk`] (atomic tmp→rename, EXDEV fallback, macOS
/// `F_FULLFSYNC`). Inherits the caller's [`SpanContext`] via
/// [`spawn_with_span_context`] so child spans carry the correct session/parent.
///
/// No-op (zero cost) when the `observe` feature is disabled.
#[cfg(feature = "observe")]
pub fn emit_span_background(ctx: crate::ayin::span::TraceContext) {
    let base = writer::default_trace_base();
    writer::spawn_with_span_context(async move {
        match ctx.finish() {
            Ok(span) => {
                let dir = writer::span_dir(&base, span.actor.as_str(), &span.timestamp);
                if let Err(e) = writer::write_span_to_disk(&span, &dir).await {
                    tracing::warn!(error = %e, "AYIN span disk write failed");
                }
            }
            Err(e) => tracing::warn!(error = %e, "AYIN span build failed"),
        }
    });
}

/// No-op stub — compiles away entirely when `observe` is disabled.
#[cfg(not(feature = "observe"))]
#[inline]
pub fn emit_span_background(_ctx: crate::ayin::span::TraceContext) {}

// ── Feature-disabled: zero-cost newtype ───────────────────────────────────────

#[cfg(not(feature = "observe"))]
mod noop_impl {
    use super::{JsonRpcRequest, JsonRpcResponse, SdkError, Transport};

    /// Zero-cost transport wrapper.
    ///
    /// When the `observe` feature is **disabled**, this type is a transparent
    /// newtype over `T` with no added overhead. Enable `observe` to activate
    /// full AYIN span recording.
    pub struct ObservableTransport<T: Transport>(T);

    impl<T: Transport> ObservableTransport<T> {
        /// Wrap `inner`. No-op when `observe` feature is disabled.
        #[must_use]
        pub fn new(inner: T) -> Self {
            Self(inner)
        }

        /// Wrap `inner` with a custom actor name. No-op when `observe` feature
        /// is disabled (actor name is ignored).
        #[must_use]
        pub fn with_actor(inner: T, _actor: impl Into<String>) -> Self {
            Self(inner)
        }
    }

    impl<T: Transport> Transport for ObservableTransport<T> {
        async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, SdkError> {
            self.0.send(request).await
        }
    }
}

#[cfg(not(feature = "observe"))]
pub use noop_impl::ObservableTransport;
