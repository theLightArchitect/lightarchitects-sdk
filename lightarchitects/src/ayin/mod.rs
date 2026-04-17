//! Feature-gated AYIN observability wrapper for lightarchitects-sdk transports.
//!
//! `lightarchitects-ayin` wraps any [`crate::core::transport::Transport`] in an
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
//! use crate::ayin::ObservableTransport;
//! use crate::core::StdioTransport;
//!
//! # async fn example() -> Result<(), crate::core::SdkError> {
//! // Works identically with or without the `observe` feature.
//! // When `observe` is active, every send() writes a TraceSpan to AYIN.
//! let transport: ObservableTransport<StdioTransport> =
//!     ObservableTransport::new(todo!("inner transport"));
//! # Ok(()) }
//! ```
//!
// Note: TraceSpan lives in the `ayin` crate (AYIN-DEV workspace), which is an
// optional path dependency. Links to it are elided in rustdoc to keep CI clean.

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
    use std::sync::Arc;

    use ayin::span::{Actor, TraceContext, TraceOutcome};
    use ayin::store::TraceStore;

    use super::{JsonRpcRequest, JsonRpcResponse, SdkError, Transport};

    /// Transport wrapper that records an AYIN [`ayin::span::TraceSpan`] for
    /// every MCP call.
    ///
    /// Construct via [`ObservableTransport::new`]. Spans are written
    /// asynchronously — a `tokio::spawn` fire-and-forget ensures trace I/O
    /// never blocks the caller.
    pub struct ObservableTransport<T: Transport> {
        inner: T,
        store: Arc<TraceStore>,
        actor: Actor,
    }

    impl<T: Transport> ObservableTransport<T> {
        /// Wrap `inner` and record AYIN spans using the `lightarchitects-sdk` actor.
        ///
        /// Spans are written to the default AYIN store path
        /// (`~/lightarchitects/soul/helix/ayin/traces/`).
        #[must_use]
        pub fn new(inner: T) -> Self {
            Self {
                inner,
                store: Arc::new(TraceStore::with_defaults()),
                actor: Actor::new("lightarchitects-sdk"),
            }
        }

        /// Wrap `inner`, using a custom actor name for the trace spans.
        #[must_use]
        pub fn with_actor(inner: T, actor: impl Into<String>) -> Self {
            Self {
                inner,
                store: Arc::new(TraceStore::with_defaults()),
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
            let store = Arc::clone(&self.store);
            tokio::spawn(async move {
                match ctx.finish() {
                    Ok(span) => {
                        if let Err(e) = store.write(&span).await {
                            tracing::warn!(error = %e, "AYIN trace write failed");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "AYIN span build failed");
                    }
                }
            });

            result
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
