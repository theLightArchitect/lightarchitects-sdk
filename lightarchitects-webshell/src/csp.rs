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
//! | `script-src`        | `'self' 'unsafe-eval' 'sha256-<hash>'` | eval required by Three.js; inline hashes computed at startup from embedded HTML (no `'unsafe-inline'`) |
//! | `style-src`         | `'self' 'unsafe-inline' fonts.googleapis.com` | Tailwind generates inline styles; Google Fonts stylesheet |
//! | `font-src`          | `'self' fonts.gstatic.com data:` | Google Fonts binary + bundled Berkeley Mono |
//! | `connect-src`       | `'self' ws://localhost:* wss://localhost:*` | PTY WebSocket + SSE same-origin |
//! | `img-src`           | `'self' data: blob:` | canvas toDataURL + blob URLs |
//! | `worker-src`        | `'self' blob:`     | Vite worker chunks |
//! | `frame-ancestors`   | `'none'`           | clickjacking prevention |

use std::sync::OnceLock;

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use sha2::{Digest, Sha256};
use tracing::debug;

/// Structural CSP template — `script-src` hashes are appended at startup.
///
/// Never use this constant directly in middleware. Use [`get_production_csp`] which
/// extends this with `sha256-<hash>` entries for every inline script found in the
/// embedded `dist/index.html` (`SvelteKit` bootstrap).
pub const CSP_BASE: &str = concat!(
    "default-src 'self'; ",
    "script-src 'self' 'unsafe-eval'",
    // inline-script sha256 hashes appended here at startup by get_production_csp()
);

/// Lazily-computed production CSP — includes sha256 hashes of `SvelteKit` inline scripts.
///
/// Computed once at first request from the embedded `dist/index.html` and `dist/200.html`.
/// This avoids `'unsafe-inline'` while still allowing the `SvelteKit` hydration bootstrap.
static PRODUCTION_CSP: OnceLock<String> = OnceLock::new();

/// Return the production CSP string with inline script hashes embedded.
///
/// Safe to call from any thread; computation is idempotent and `OnceLock`-guarded.
pub fn get_production_csp() -> &'static str {
    PRODUCTION_CSP.get_or_init(compute_production_csp)
}

/// Build the production CSP string, scanning embedded HTML for inline scripts and
/// appending their sha256 hashes to `script-src`.
fn compute_production_csp() -> String {
    use crate::static_assets::Assets;

    let mut hashes: Vec<String> = Vec::new();

    // Scan the two SPA entry points that SvelteKit adapter-static generates.
    for filename in ["index.html", "200.html"] {
        if let Some(file) = Assets::get(filename) {
            if let Ok(html) = std::str::from_utf8(file.data.as_ref()) {
                extract_inline_script_hashes(html, &mut hashes);
            }
        }
    }

    let hash_clause = if hashes.is_empty() {
        String::new()
    } else {
        format!(" {}", hashes.join(" "))
    };

    format!(
        "default-src 'self'; \
         script-src 'self' 'unsafe-eval'{hash_clause}; \
         style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; \
         font-src 'self' https://fonts.gstatic.com data:; \
         connect-src 'self' ws://localhost:* wss://localhost:* http://localhost:*; \
         img-src 'self' data: blob:; \
         worker-src 'self' blob:; \
         frame-ancestors 'none'; \
         report-uri /api/csp-report"
    )
}

/// Extract `sha256-<base64>` hashes for all non-external inline `<script>` bodies in `html`.
///
/// Skips `<script src="...">` (external scripts are covered by `'self'`).
/// Does not trim the script body before hashing — browsers hash the exact bytes between
/// `<script>` and `</script>`.
fn extract_inline_script_hashes(html: &str, hashes: &mut Vec<String>) {
    let mut remaining = html;

    while let Some(tag_start) = remaining.find("<script") {
        // Advance past "<script"
        let after_keyword = &remaining[tag_start + 7..];

        // The next character must be whitespace, '>', or '/' (e.g. `<script>` or `<script type=...>`)
        let first_char = after_keyword.chars().next().unwrap_or('\0');
        if !matches!(first_char, ' ' | '\t' | '\n' | '\r' | '>' | '/') {
            // Not a real <script> tag (e.g. `<scriptable>`). Skip past this occurrence.
            remaining = &remaining[tag_start + 7..];
            continue;
        }

        // Find end of opening tag
        let Some(attrs_end) = after_keyword.find('>') else {
            break;
        };
        let attrs = &after_keyword[..attrs_end];

        // Skip external scripts — hashes are only for inline content
        if attrs.contains("src=") {
            remaining = &after_keyword[attrs_end + 1..];
            continue;
        }

        // Self-closing <script /> has no body — skip
        if attrs.ends_with('/') {
            remaining = &after_keyword[attrs_end + 1..];
            continue;
        }

        let body_start = &after_keyword[attrs_end + 1..];
        let Some(close_pos) = body_start.find("</script>") else {
            break;
        };

        let script_body = &body_start[..close_pos];

        // Only hash non-empty bodies (browser ignores `<script></script>`)
        if !script_body.trim().is_empty() {
            let digest = Sha256::digest(script_body.as_bytes());
            let hash_str = format!("'sha256-{}'", B64.encode(digest));
            if !hashes.contains(&hash_str) {
                hashes.push(hash_str);
            }
        }

        remaining = &body_start[close_pos + 9..]; // advance past </script>
    }
}

/// The CSP policy string — kept for legacy tests; production uses [`get_production_csp`].
#[allow(dead_code)]
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

/// Relaxed CSP used only when `--dev-mode` is set.
///
/// Vite HMR needs inline/eval script allowances plus loopback WebSocket
/// endpoints. Production never selects this policy.
pub const DEV_CSP_POLICY: &str = concat!(
    "default-src 'self'; ",
    "script-src 'self' 'unsafe-eval' 'unsafe-inline' http://localhost:5173 http://127.0.0.1:5173; ",
    "style-src 'self' 'unsafe-inline' https://fonts.googleapis.com http://localhost:5173 http://127.0.0.1:5173; ",
    "font-src 'self' https://fonts.gstatic.com data: http://localhost:5173 http://127.0.0.1:5173; ",
    "connect-src 'self' ws://localhost:* wss://localhost:* http://localhost:* ws://127.0.0.1:* http://127.0.0.1:*; ",
    "img-src 'self' data: blob:; ",
    "worker-src 'self' blob:; ",
    "frame-ancestors 'none'; ",
    "report-uri /api/csp-report",
);

/// Axum middleware that injects `Content-Security-Policy-Report-Only`.
///
/// Uses the same computed policy as [`enforce_layer`] — inline script hashes
/// from the embedded `SvelteKit` bundle are included so the report stream is
/// quiet under normal operation.
///
/// Attach via `.layer(axum::middleware::from_fn(csp::report_only_layer))`.
pub async fn report_only_layer(req: Request<Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    if let Ok(val) = HeaderValue::from_str(get_production_csp()) {
        response.headers_mut().insert(
            HeaderName::from_static("content-security-policy-report-only"),
            val,
        );
    }
    response
}

/// Axum middleware that injects enforcing `Content-Security-Policy`.
///
/// The policy includes `sha256-<hash>` entries for every inline script found in the
/// embedded `dist/index.html` and `dist/200.html`, computed once at startup via
/// [`get_production_csp`]. No `'unsafe-inline'` is used.
///
/// Attach via `.layer(axum::middleware::from_fn(csp::enforce_layer))`.
pub async fn enforce_layer(req: Request<Body>, next: Next) -> Response {
    enforce_with_policy(req, next, get_production_csp()).await
}

/// Axum middleware that injects a dev-mode enforcing CSP.
pub async fn dev_enforce_layer(req: Request<Body>, next: Next) -> Response {
    enforce_with_policy(req, next, DEV_CSP_POLICY).await
}

async fn enforce_with_policy(req: Request<Body>, next: Next, policy: &str) -> Response {
    let mut response = next.run(req).await;
    if let Ok(val) = HeaderValue::from_str(policy) {
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
    async fn dev_enforce_header_allows_vite_loopback() {
        let app = Router::new()
            .route("/", get(ok_handler))
            .layer(axum::middleware::from_fn(dev_enforce_layer));

        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let hdr = resp
            .headers()
            .get("content-security-policy")
            .expect("dev enforce header must be present");
        let val = hdr.to_str().unwrap();
        assert!(val.contains("http://localhost:5173"));
        assert!(val.contains("ws://127.0.0.1:*"));
        assert!(val.contains("'unsafe-inline'"));
    }

    #[test]
    fn extract_hashes_skips_external_scripts() {
        let html = r#"<html><head><script src="/app.js"></script></head></html>"#;
        let mut hashes = Vec::new();
        extract_inline_script_hashes(html, &mut hashes);
        assert!(
            hashes.is_empty(),
            "external src= scripts must not be hashed"
        );
    }

    #[test]
    fn extract_hashes_covers_inline_scripts() {
        // The hash here is the sha256 of the exact body bytes (including whitespace).
        let body = "window.__init = true;";
        let html = format!("<script>{body}</script>");
        let mut hashes = Vec::new();
        extract_inline_script_hashes(&html, &mut hashes);
        assert_eq!(hashes.len(), 1);
        // Verify hash format: 'sha256-<base64>'
        assert!(
            hashes[0].starts_with("'sha256-"),
            "hash must have sha256- prefix: {}",
            hashes[0]
        );
        assert!(
            hashes[0].ends_with('\''),
            "hash must be wrapped in single quotes: {}",
            hashes[0]
        );
    }

    #[test]
    fn extract_hashes_deduplicates() {
        let body = "console.log(1);";
        let html = format!("<script>{body}</script><script>{body}</script>");
        let mut hashes = Vec::new();
        extract_inline_script_hashes(&html, &mut hashes);
        assert_eq!(hashes.len(), 1, "identical scripts must produce one hash");
    }

    #[test]
    fn extract_hashes_skips_empty_scripts() {
        let html = "<script></script><script>  </script>";
        let mut hashes = Vec::new();
        extract_inline_script_hashes(html, &mut hashes);
        assert!(
            hashes.is_empty(),
            "empty/whitespace-only scripts must not be hashed"
        );
    }

    #[test]
    fn extract_hashes_ignores_scriptable_tag() {
        // <scriptable> is not a <script> tag
        let html = "<scriptable>init()</scriptable>";
        let mut hashes = Vec::new();
        extract_inline_script_hashes(html, &mut hashes);
        assert!(
            hashes.is_empty(),
            "<scriptable> must not match <script> extractor"
        );
    }

    #[test]
    fn enforce_layer_policy_contains_no_unsafe_inline() {
        // The computed production CSP must never contain 'unsafe-inline' in script-src.
        // (dev mode uses DEV_CSP_POLICY which does include it — this verifies production only.)
        let csp = get_production_csp();
        // Extract only the script-src directive value
        if let Some(script_src_start) = csp.find("script-src ") {
            let after = &csp[script_src_start..];
            let directive = after.split(';').next().unwrap_or(after);
            assert!(
                !directive.contains("'unsafe-inline'"),
                "production script-src must not contain 'unsafe-inline': {directive}"
            );
        }
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
