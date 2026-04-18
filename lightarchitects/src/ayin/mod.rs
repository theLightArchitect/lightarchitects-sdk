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

// ── Feature-enabled: full AYIN instrumentation ────────────────────────────────

#[cfg(feature = "observe")]
mod observe_impl {
    use std::path::PathBuf;

    use crate::ayin::span::{Actor, TraceContext, TraceOutcome, TraceSpan};

    use super::{JsonRpcRequest, JsonRpcResponse, SdkError, Transport};

    /// Transport wrapper that records an AYIN [`TraceSpan`] for every MCP call.
    ///
    /// Construct via [`ObservableTransport::new`]. Spans are written
    /// asynchronously — a `tokio::spawn` fire-and-forget ensures trace I/O
    /// never blocks the caller.
    pub struct ObservableTransport<T: Transport> {
        inner: T,
        actor: Actor,
    }

    impl<T: Transport> ObservableTransport<T> {
        /// Wrap `inner` and record AYIN spans using the `lightarchitects-sdk` actor.
        ///
        /// Spans are written to `~/lightarchitects/soul/helix/ayin/traces/`.
        #[must_use]
        pub fn new(inner: T) -> Self {
            Self {
                inner,
                actor: Actor::new("lightarchitects-sdk"),
            }
        }

        /// Wrap `inner`, using a custom actor name for the trace spans.
        #[must_use]
        pub fn with_actor(inner: T, actor: impl Into<String>) -> Self {
            Self {
                inner,
                actor: Actor::new(actor),
            }
        }
    }

    impl<T: Transport> Transport for ObservableTransport<T> {
        async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, SdkError> {
            let action = request.method.clone();
            let result = self.inner.send(request).await;

            let outcome = match &result {
                Ok(_) => TraceOutcome::Continue,
                Err(e) => TraceOutcome::Error(e.to_string()),
            };

            // Build and persist the span asynchronously — never blocks the caller.
            let ctx = TraceContext::new(self.actor.clone(), &action).outcome(outcome);
            tokio::spawn(async move {
                match ctx.finish() {
                    Ok(span) => write_span(&span).await,
                    Err(e) => tracing::warn!(error = %e, "AYIN span build failed"),
                }
            });

            result
        }
    }

    /// Write a span as JSON to the AYIN traces directory.
    ///
    /// Path: `~/lightarchitects/soul/helix/ayin/traces/{actor}/{YYYY-MM-DD}/{HH-MM-SS}-{action}-{id8}.json`
    async fn write_span(span: &TraceSpan) {
        let base = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lightarchitects/soul/helix/ayin/traces");
        let dir = base
            .join(span.actor.as_str())
            .join(span.timestamp.format("%Y-%m-%d").to_string());
        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
            tracing::warn!(error = %e, "AYIN trace dir create failed");
            return;
        }
        let safe_action = span.action.replace('/', "_");
        let id8 = &span.id.to_string()[..8];
        let name = format!(
            "{}-{}-{}.json",
            span.timestamp.format("%H-%M-%S"),
            safe_action,
            id8
        );
        match serde_json::to_vec(span) {
            Ok(bytes) => {
                if let Err(e) = tokio::fs::write(dir.join(name), bytes).await {
                    tracing::warn!(error = %e, "AYIN trace write failed");
                }
            }
            Err(e) => tracing::warn!(error = %e, "AYIN span serialize failed"),
        }
    }
}

#[cfg(feature = "observe")]
pub use observe_impl::ObservableTransport;

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
