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

use crate::span_context::{span_dir, spawn_with_span_context, write_span_to_disk};

/// The AYIN action type for platform HTTP requests.
const SPAN_HTTP_REQUEST: &str = "platform.http.request";

/// Emit a `platform.http.request` AYIN trace span for every HTTP request.
pub async fn ayin_trace_middleware(req: Request<Body>, next: Next) -> Response<Body> {
    let method = req.method().to_string();
    let path = req.uri().path().to_owned();
    let start = std::time::Instant::now();

    let response = next.run(req).await;

    let status = response.status().as_u16();
    let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

    emit_http_span(method, path, status, latency_ms);

    response
}

fn emit_http_span(method: String, path: String, status: u16, latency_ms: u64) {
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
    // spawn_with_span_context forwards any enclosing SPAN_CTX into the write task.
    spawn_with_span_context(async move {
        let Ok(span) = TraceContext::new(Actor::new("gateway"), SPAN_HTTP_REQUEST)
            .metadata(metadata)
            .outcome(outcome)
            .finish()
        else {
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
