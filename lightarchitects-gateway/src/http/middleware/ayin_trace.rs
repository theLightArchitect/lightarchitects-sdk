//! AYIN HTTP span middleware.
//!
//! Writes one `platform.http.request` [`TraceSpan`] per HTTP request to the
//! AYIN trace directory. Fire-and-forget via [`spawn_with_span_context`] so
//! it never blocks the response path and inherits any enclosing span context.
//!
//! Placed outermost in the middleware stack so it records every request,
//! including those rejected by rate-limit or auth middleware.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, Response};
use axum::middleware::Next;
use lightarchitects::ayin::semconv::lasdlc;
use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
use serde_json::json;

use crate::span_context::{
    current_span_ctx, span_dir, spawn_with_span_context, with_span_context, write_span_to_disk,
};

/// The AYIN action type for platform HTTP requests.
const SPAN_HTTP_REQUEST: &str = "platform.http.request";

/// Emit a `platform.http.request` AYIN trace span for every HTTP request.
///
/// Seeds a per-request [`GatewaySpanContext`] scope so that [`emit_http_span`]
/// can attach any enclosing `parent_id` / `session_id` (e.g., if this request
/// is made internally from within a strategy's `with_span_context` scope).
/// For standalone Arena HTTP clients the context is `None/None` — those spans
/// are correctly root-level in the Lineage Circuit.
pub async fn ayin_trace_middleware(req: Request<Body>, next: Next) -> Response<Body> {
    let method = req.method().to_string();
    let path = req.uri().path().to_owned();
    let start = std::time::Instant::now();

    // Capture any enclosing span context. HTTP tasks run in axum's task pool and
    // do NOT inherit the MCP stdio task's task_local storage, so this is typically
    // the default (None/None) for external callers.
    let ctx = current_span_ctx();

    with_span_context(ctx, async move {
        let response = next.run(req).await;
        let status = response.status().as_u16();
        let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        emit_http_span(method, path, status, latency_ms);
        response
    })
    .await
}

fn emit_http_span(method: String, path: String, status: u16, latency_ms: u64) {
    let ctx = current_span_ctx();
    let outcome = if status < 400 {
        TraceOutcome::Continue
    } else {
        TraceOutcome::Block
    };
    let metadata = json!({
        "http.method": method,
        lasdlc::ATTR_ROUTE: path,
        "http.status_code": status,
        lasdlc::ATTR_LATENCY_MS: latency_ms,
    });
    spawn_with_span_context(async move {
        let mut builder = TraceContext::new(Actor::new("gateway"), SPAN_HTTP_REQUEST)
            .metadata(metadata)
            .outcome(outcome);
        if let Some(pid) = ctx.parent_id {
            builder = builder.parent(pid);
        }
        if let Some(ref sid) = ctx.session_id {
            builder = builder.session_id(sid);
        }
        let Ok(span) = builder.finish() else {
            return;
        };
        let base = dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lightarchitects/soul/helix/ayin/traces");
        let dir = span_dir(&base, span.actor.as_str(), &span.timestamp);
        if let Err(e) = write_span_to_disk(&span, &dir).await {
            tracing::warn!(error = %e, "HTTP AYIN span write failed");
        }
    });
}
