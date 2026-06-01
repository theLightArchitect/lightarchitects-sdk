//! Observability span helpers for `LiteLLM`-mediated requests.
//!
//! Emits structured AYIN spans for three events in every `LiteLLM` round-trip:
//!
//! | Span name | When | Key attributes |
//! |-----------|------|---------------|
//! | `litellm.request` | HTTP request dispatched to proxy | `model`, `traceparent`, `build_id` |
//! | `litellm.response` | HTTP response received | `response_ms`, `prompt_tokens`, `completion_tokens`, `cost_usd` |
//! | `mcp.semantic_filter` | MCP tool-list filtered before dispatch | `tools_before`, `tools_after`, `top_k`, `latency_ms` |
//!
//! All spans include a `turn_span_id` field so AYIN can correlate them with
//! the parent copilot turn.
//!
//! # Design note
//!
//! Spans are emitted via the `tracing` macros (which the AYIN subscriber
//! intercepts and forwards to the JSONL store + OTLP endpoint).  No direct
//! `opentelemetry` API calls are made — the tracing-opentelemetry bridge
//! handles propagation.

use std::time::Instant;

use tracing::info;

/// Emit a `litellm.request` span when a request is dispatched to the proxy.
///
/// Call this immediately before the HTTP request is sent.
/// Returns an [`Instant`] to pass to [`record_litellm_response`].
pub fn record_litellm_request(
    build_id: &str,
    model: &str,
    traceparent: &str,
    turn_span_id: &str,
) -> Instant {
    info!(
        target: "litellm.request",
        build_id,
        model,
        traceparent,
        turn_span_id,
        "litellm.request"
    );
    Instant::now()
}

/// Emit a `litellm.response` span after the proxy returns a response.
///
/// # Parameters
/// - `start`: the [`Instant`] returned by [`record_litellm_request`].
/// - `prompt_tokens` / `completion_tokens`: from `x-litellm-usage` headers or
///   response body `usage` field.
/// - `cost_usd`: from `x-litellm-response-cost` header or `response_cost` field.
pub fn record_litellm_response(
    build_id: &str,
    model: &str,
    turn_span_id: &str,
    start: Instant,
    prompt_tokens: u32,
    completion_tokens: u32,
    cost_usd: f64,
) {
    let response_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    info!(
        target: "litellm.response",
        build_id,
        model,
        turn_span_id,
        response_ms,
        prompt_tokens,
        completion_tokens,
        cost_usd,
        "litellm.response"
    );
}

/// Emit a `mcp.semantic_filter` span after the MCP tool list is filtered.
///
/// Called by the semantic-filter middleware before the tool list is injected
/// into the `LiteLLM` request.  The Phase 4.5 Sub-C mechanical check verifies
/// `tools_after == 5` (top-K default) and `latency_ms < 200`.
///
/// # Parameters
/// - `tools_before`: full tool count before filtering.
/// - `tools_after`: tool count passed to the model (`top_k` default: 5).
/// - `top_k`: the `K` applied.
/// - `start`: `Instant` from before the filter ran.
pub fn record_mcp_semantic_filter(
    build_id: &str,
    turn_span_id: &str,
    tools_before: usize,
    tools_after: usize,
    top_k: usize,
    start: Instant,
) {
    let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    info!(
        target: "mcp.semantic_filter",
        build_id,
        turn_span_id,
        tools_before,
        tools_after,
        top_k,
        latency_ms,
        "mcp.semantic_filter"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `record_litellm_request` must return an `Instant` that is not in the future.
    #[test]
    fn record_litellm_request_returns_instant_in_past() {
        let start = record_litellm_request("build-1", "gpt-4o", "00-abc-def-01", "turn-1");
        assert!(start.elapsed().as_nanos() < 1_000_000_000);
    }

    /// `record_mcp_semantic_filter` must not panic for zero-tool edge case.
    #[test]
    fn record_mcp_semantic_filter_zero_tools_does_not_panic() {
        let start = Instant::now();
        record_mcp_semantic_filter("b", "t", 0, 0, 5, start);
    }

    /// `record_litellm_response` must accept zero-cost responses (cached hits).
    #[test]
    fn record_litellm_response_zero_cost_allowed() {
        let start = Instant::now();
        record_litellm_response("b", "gemma2-9b-it", "t", start, 100, 50, 0.0);
    }
}
