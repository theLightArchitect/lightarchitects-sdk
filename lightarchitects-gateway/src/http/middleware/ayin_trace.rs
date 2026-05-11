//! AYIN HTTP span middleware.
//!
//! Writes one `platform.http.request` [`TraceSpan`] per HTTP request to the
//! AYIN trace directory (`~/.lightarchitects/lightarchitects/soul/helix/ayin/traces/`).
//! Fire-and-forget via `tokio::spawn` — never blocks the response path.
//!
//! Placed outermost in the middleware stack (wraps TraceLayer) so it records
//! every request including those rejected by rate-limit or auth middleware.
//!
//! Required for P95/P99 latency baseline establishment (WGC pre-flight #4).

use axum::body::Body;
use axum::http::{Request, Response};
use axum::middleware::Next;
use lightarchitects::ayin::semconv::lasdlc;
use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};
use serde_json::json;

/// The AYIN action type for platform HTTP requests.
const SPAN_HTTP_REQUEST: &str = "platform.http.request";

/// Emit a `platform.http.request` AYIN trace span for every HTTP request.
pub async fn ayin_trace_middleware(req: Request<Body>, next: Next) -> Response<Body> {
    let method = req.method().to_string();
    let path = req.uri().path().to_owned();
    let start = std::time::Instant::now();

    let response = next.run(req).await;

    let status = response.status().as_u16();
    let latency_ms = start.elapsed().as_millis() as u64;

    emit_http_span(method, path, status, latency_ms);

    response
}

fn emit_http_span(method: String, path: String, status: u16, latency_ms: u64) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    handle.spawn(async move {
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
        let ctx = TraceContext::new(Actor::new("gateway"), SPAN_HTTP_REQUEST)
            .metadata(metadata)
            .outcome(outcome);
        match ctx.finish() {
            Ok(span) => write_http_span(span).await,
            Err(e) => tracing::warn!(error = %e, "HTTP AYIN span build failed"),
        }
    });
}

async fn write_http_span(span: lightarchitects::ayin::span::TraceSpan) {
    use std::path::PathBuf;
    let base = dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lightarchitects/soul/helix/ayin/traces");
    let dir = base
        .join(span.actor.as_str())
        .join(span.timestamp.format("%Y-%m-%d").to_string());
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        tracing::warn!(error = %e, "AYIN HTTP trace dir failed");
        return;
    }
    let safe_action = span.action.replace('/', "_");
    let id_str = span.id.to_string();
    let name = format!(
        "{}-{}-{}.json",
        span.timestamp.format("%H-%M-%S"),
        safe_action,
        &id_str[..8]
    );
    match serde_json::to_vec(&span) {
        Ok(bytes) => {
            if let Err(e) = tokio::fs::write(dir.join(name), bytes).await {
                tracing::warn!(error = %e, "AYIN HTTP trace write failed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "AYIN HTTP span serialize failed"),
    }
}
