//! Observability primitives for lightsquad wave execution.
//!
//! Three sub-modules implement the full tracing + metrics stack:
//!
//! | Module | Purpose |
//! |---|---|
//! | [`traceparent`] | W3C `traceparent` carrier — parse, generate, propagate |
//! | [`span_schema`] | Tool-call span attribute schema for AYIN traces |
//! | [`metrics`] | Google SRE Golden Signals + Apdex for wave execution |
//!
//! # Feature gate
//!
//! This module is compiled only when the `lightsquad` feature is enabled.

/// W3C [`traceparent`](traceparent::TraceParent) carrier — parse and generate
/// W3C Trace Context `traceparent` header values for lightsquad spans.
pub mod traceparent;

/// Tool-call [`span_schema`](span_schema::SpanAttrs) — OTEL attribute schema
/// for spans emitted by lightsquad workers, forwarded to AYIN via HTTP.
pub mod span_schema;

/// Wave-level [`metrics`](metrics::WaveMetrics) — Google SRE Golden Signals
/// and Apdex score for a completed lightsquad wave.
pub mod metrics;
