//! Content Security Policy middleware and violation report endpoint.
//!
//! # Phase order
//!
//! `SEC-3a` — Report-Only: adds `Content-Security-Policy-Report-Only` so
//! violations are sent to `/api/csp-report` and logged without blocking.
//!
//! `SEC-3b` — Enforce: replaces the Report-Only header with the enforcing
//! `Content-Security-Policy` header once the report stream confirms no
//! legitimate inline-script or eval violations.
//!
//! # Policy rationale
//!
//! | Directive           | Allowance          | Reason |
//! |---------------------|--------------------|--------|
//! | `default-src`       | `'self'`           | baseline deny-all |
//! | `script-src`        | `'self' 'unsafe-eval'` | Three.js shader compiler requires eval |
//! | `style-src`         | `'self' 'unsafe-inline' fonts.googleapis.com` | Tailwind generates inline styles; Google Fonts stylesheet |
//! | `font-src`          | `'self' fonts.gstatic.com data:` | Google Fonts binary + bundled Berkeley Mono |
//! | `connect-src`       | `'self' ws://localhost:* wss://localhost:*` | PTY WebSocket + SSE same-origin |
//! | `img-src`           | `'self' data: blob:` | canvas toDataURL + blob URLs |
//! | `worker-src`        | `'self' blob:`     | Vite worker chunks |
//! | `frame-ancestors`   | `'none'`           | clickjacking prevention |

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::debug;

/// The CSP policy string shared by both Report-Only and Enforce headers.
pub const CSP_POLICY: &str = concat!(
    "default-src 'self'; ",
    "script-src 'self' 'unsafe-eval'; ",
    "style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; ",
    "font-src 'self' https://fonts.gstatic.com data:; ",
    "connect-src 'self' ws://localhost:* wss://localhost:* http://localhost:*; ",
    "img-src 'self' data: blob:; ",
    "worker-src 'self' blob:; ",
    "frame-ancestors 'none'; ",
    "report-uri /api/csp-report",
);

/// Axum middleware that injects `Content-Security-Policy-Report-Only`.
///
/// Attach via `.layer(axum::middleware::from_fn(csp::report_only_layer))`.
pub async fn report_only_layer(req: Request<Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    if let Ok(val) = HeaderValue::from_str(CSP_POLICY) {
        response.headers_mut().insert(
            HeaderName::from_static("content-security-policy-report-only"),
            val,
        );
    }
    response
}

/// Axum middleware that injects enforcing `Content-Security-Policy`.
///
/// Replaces `report_only_layer` once the report stream confirms no violations.
/// Attach via `.layer(axum::middleware::from_fn(csp::enforce_layer))`.
pub async fn enforce_layer(req: Request<Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    if let Ok(val) = HeaderValue::from_str(CSP_POLICY) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("content-security-policy"), val);
    }
    response
}

/// `POST /api/csp-report` — receives browser violation reports and logs them.
///
/// Returns `204 No Content`. Body is intentionally ignored in Report-Only
/// phase; a future phase can parse and forward to AYIN for observability.
pub async fn csp_report_handler(body: String) -> impl IntoResponse {
    debug!(target: "webshell::csp", report = %body, "CSP violation report received");
    StatusCode::NO_CONTENT
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    async fn ok_handler() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn report_only_header_present() {
        let app = Router::new()
            .route("/", get(ok_handler))
            .layer(axum::middleware::from_fn(report_only_layer));

        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let hdr = resp
            .headers()
            .get("content-security-policy-report-only")
            .expect("report-only header must be present");
        let val = hdr.to_str().unwrap();
        assert!(
            val.contains("default-src 'self'"),
            "missing default-src: {val}"
        );
        assert!(
            val.contains("report-uri /api/csp-report"),
            "missing report-uri: {val}"
        );
        assert!(
            !resp.headers().contains_key("content-security-policy"),
            "enforce header must NOT be present in report-only mode"
        );
    }

    #[tokio::test]
    async fn enforce_header_replaces_report_only() {
        let app = Router::new()
            .route("/", get(ok_handler))
            .layer(axum::middleware::from_fn(enforce_layer));

        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let hdr = resp
            .headers()
            .get("content-security-policy")
            .expect("enforce header must be present");
        assert!(hdr.to_str().unwrap().contains("default-src 'self'"));
        assert!(
            !resp
                .headers()
                .contains_key("content-security-policy-report-only"),
            "report-only header must NOT be present in enforce mode"
        );
    }

    #[tokio::test]
    async fn csp_report_returns_204() {
        let app = Router::new().route("/api/csp-report", axum::routing::post(csp_report_handler));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/csp-report")
                    .body(Body::from(
                        r#"{"csp-report":{"violated-directive":"script-src"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }
}
