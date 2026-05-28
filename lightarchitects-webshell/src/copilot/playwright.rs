//! CDP bridge — Chrome `DevTools` Protocol client for dev-mode browser inspection.
//!
//! Uses the `chromiumoxide` crate (NOT the `@playwright/test` npm package, which
//! is E2E-test only). Exposes screenshot and DOM snapshot capabilities through
//! HTTP API endpoints that the frontend calls.
//!
//! ## Security model
//!
//! - **Localhost only**: CDP connects to `127.0.0.1` exclusively; private IP ranges
//!   are rejected per Security Guardrails §5.4.
//! - **URL allowlist**: `navigate()` is restricted to `localhost` + `127.0.0.1`
//!   origins; arbitrary URLs are blocked.
//! - **Indirect injection shield**: DOM content returned to the LLM is sanitized
//!   through the existing `IndirectInjectionShield` before injection.
//! - **Session authentication**: Each `PlaywrightBridge` instance carries a
//!   cryptographic token validated on every API request.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::handler::Handler;
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

/// Cryptographic token length for session auth (16 bytes = 32 hex chars).
const AUTH_TOKEN_BYTES: usize = 16;

/// Maximum concurrent CDP operations per session.
const MAX_CONCURRENT_OPS: usize = 4;

/// Maximum DOM body text length returned to callers (prevents oversized responses).
const DOM_BODY_CAP: usize = 10_000;

/// Maximum text length per DOM element returned to callers.
const DOM_ELEMENT_TEXT_CAP: usize = 500;

/// Maximum number of DOM elements returned per snapshot.
const DOM_ELEMENT_COUNT_CAP: usize = 100;

/// Wall-clock timeout for individual CDP operations.
const CDP_OP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Playwright bridge — manages a CDP browser connection for dev-mode inspection.
pub struct PlaywrightBridge {
    browser: Arc<Mutex<Option<Browser>>>,
    /// Handler must be kept alive — dropping it closes the CDP connection.
    handler: Arc<Mutex<Option<Handler>>>,
    auth_token: String,
    /// Session identifier for logging; not used in request handling.
    #[allow(dead_code)]
    session_id: String,
    /// Semaphore to bound concurrent CDP operations.
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl Default for PlaywrightBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaywrightBridge {
    /// Create a new bridge with a fresh auth token.
    ///
    /// The browser is lazily connected on first use — construction is cheap.
    pub fn new() -> Self {
        let mut token_bytes = [0u8; AUTH_TOKEN_BYTES];
        rand::thread_rng().fill_bytes(&mut token_bytes);
        let auth_token = hex::encode(token_bytes);
        let session_id = Uuid::new_v4().to_string();

        info!(session_id = %session_id, "playwright: bridge created");
        Self {
            browser: Arc::new(Mutex::new(None)),
            handler: Arc::new(Mutex::new(None)),
            auth_token,
            session_id,
            semaphore: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_OPS)),
        }
    }

    /// Validate the auth token against this bridge's token.
    pub fn validate_token(&self, provided: &str) -> bool {
        use constant_time_eq::constant_time_eq;
        constant_time_eq(self.auth_token.as_bytes(), provided.as_bytes())
    }

    /// Ensure a browser connection exists, creating one lazily if needed.
    async fn ensure_browser(&self) -> Result<(), String> {
        let mut guard = self.browser.lock().await;
        if guard.is_some() {
            return Ok(());
        }

        let (browser, handler) = Browser::launch(
            BrowserConfig::builder()
                .arg("--headless=new")
                .arg("--disable-gpu")
                .arg("--no-sandbox")
                .arg("--disable-dev-shm-usage")
                .arg("--disable-extensions")
                .arg("--remote-debugging-port=0")
                .build()
                .map_err(|e| {
                    warn!(error = %e, "playwright: failed to launch browser");
                    "browser_launch_failed".to_owned()
                })?,
        )
        .await
        .map_err(|e| {
            warn!(error = %e, "playwright: browser launch I/O error");
            "browser_launch_io_error".to_owned()
        })?;

        info!("playwright: browser launched");
        *guard = Some(browser);
        drop(guard);
        let mut h_guard = self.handler.lock().await;
        *h_guard = Some(handler);
        Ok(())
    }

    /// Take a screenshot of the current page at the given URL.
    ///
    /// The URL must be a localhost origin — remote URLs are rejected.
    ///
    /// # Errors
    ///
    /// Returns an opaque error string on URL validation failure, browser
    /// launch failure, timeout, or CDP protocol error.
    pub async fn take_screenshot(&self, url: &str) -> Result<Vec<u8>, String> {
        validate_url(url)?;
        self.ensure_browser().await?;

        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| "cdp_semaphore_closed".to_owned())?;

        let guard = self.browser.lock().await;
        let browser = guard.as_ref().ok_or("browser_unavailable".to_owned())?;

        let page = tokio::time::timeout(CDP_OP_TIMEOUT, browser.new_page(url))
            .await
            .map_err(|_| "cdp_page_timeout".to_owned())?
            .map_err(|e| {
                warn!(url, error = %e, "playwright: failed to open page");
                "cdp_page_open_failed".to_owned()
            })?;

        let params = chromiumoxide::page::ScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Png)
            .build();
        let screenshot = tokio::time::timeout(CDP_OP_TIMEOUT, page.screenshot(params))
            .await
            .map_err(|_| "cdp_screenshot_timeout".to_owned())?
            .map_err(|e| {
                warn!(url, error = %e, "playwright: screenshot failed");
                "cdp_screenshot_failed".to_owned()
            })?;

        Ok(screenshot)
    }

    /// Capture a DOM snapshot of the current page at the given URL.
    ///
    /// Returns a JSON structure describing the DOM tree. The URL must be
    /// a localhost origin — remote URLs are rejected.
    ///
    /// # Errors
    ///
    /// Returns an opaque error string on URL validation failure, browser
    /// launch failure, timeout, or DOM evaluation error.
    pub async fn inspect_dom(&self, url: &str) -> Result<DomSnapshot, String> {
        validate_url(url)?;
        self.ensure_browser().await?;

        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| "cdp_semaphore_closed".to_owned())?;

        let guard = self.browser.lock().await;
        let browser = guard.as_ref().ok_or("browser_unavailable".to_owned())?;

        let page = tokio::time::timeout(CDP_OP_TIMEOUT, browser.new_page(url))
            .await
            .map_err(|_| "cdp_page_timeout".to_owned())?
            .map_err(|e| {
                warn!(url, error = %e, "playwright: failed to open page for DOM");
                "cdp_page_open_failed".to_owned()
            })?;

        // Wait for page to settle before extracting DOM.
        let _ = tokio::time::timeout(CDP_OP_TIMEOUT, page.wait_for_navigation_response()).await;

        #[allow(clippy::uninlined_format_args)]
        let js_expr = format!(
            "JSON.stringify({{\
                url: location.href,\
                title: document.title,\
                body: document.body ? document.body.innerText.substring(0, {body_cap}) : '',\
                elements: Array.from(document.querySelectorAll('[id],[data-testid],[data-card-role]'))\
                    .slice(0, {elem_cap}).map(el => ({{\
                        tag: el.tagName,\
                        id: el.id || undefined,\
                        role: el.getAttribute('role') || undefined,\
                        cardRole: el.getAttribute('data-card-role') || undefined,\
                        text: (el.textContent || '').substring(0, {text_cap}),\
                    }}))\
            }})",
            body_cap = DOM_BODY_CAP,
            elem_cap = DOM_ELEMENT_COUNT_CAP,
            text_cap = DOM_ELEMENT_TEXT_CAP,
        );

        let dom_json = tokio::time::timeout(CDP_OP_TIMEOUT, page.evaluate(js_expr))
            .await
            .map_err(|_| "cdp_dom_eval_timeout".to_owned())?
            .map_err(|e| {
                warn!(url, error = %e, "playwright: DOM evaluation failed");
                "cdp_dom_eval_failed".to_owned()
            })?;

        let dom_str = dom_json
            .value()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();

        let snapshot: DomSnapshot = serde_json::from_str(&dom_str).unwrap_or(DomSnapshot {
            url: url.to_owned(),
            title: String::new(),
            body: String::new(),
            elements: Vec::new(),
        });

        Ok(snapshot)
    }

    /// Shut down the browser gracefully.
    pub async fn shutdown(&self) {
        let mut guard = self.browser.lock().await;
        if let Some(browser) = guard.take() {
            info!("playwright: shutting down browser");
            drop(browser);
        }
    }
}

/// DOM snapshot returned by `inspect_dom()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct DomSnapshot {
    pub url: String,
    pub title: String,
    pub body: String,
    pub elements: Vec<DomElement>,
}

/// A single element extracted from the DOM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct DomElement {
    pub tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(rename = "cardRole", skip_serializing_if = "Option::is_none")]
    pub card_role: Option<String>,
    pub text: String,
}

// ---------------------------------------------------------------------------
// URL validation — Security Guardrails §5.4
// ---------------------------------------------------------------------------

/// Validate that a URL targets localhost only. Rejects private IPs and
/// non-local origins to prevent SSRF (Security Guardrails §5.4).
///
/// Uses `url::Host` enum variants to avoid string-prefix bypass (e.g.
/// `127.0.0.1.attacker.com` would pass `starts_with("127.")`).
fn validate_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| {
        warn!(url, error = %e, "playwright: invalid URL");
        "invalid_url".to_owned()
    })?;

    let is_allowed = match parsed.host() {
        Some(url::Host::Domain(d)) => d == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };

    if !is_allowed {
        warn!(url, host = ?parsed.host(), "playwright: URL rejected — non-local host");
        return Err("url_not_local".to_owned());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// API route handlers
// ---------------------------------------------------------------------------

/// Shared state for Playwright routes — holds the bridge behind an Arc<Mutex>.
pub type PlaywrightState = Arc<Mutex<Option<PlaywrightBridge>>>;

/// Response body for CDP session init endpoint.
#[derive(Debug, Serialize)]
#[allow(missing_docs)]
pub struct InitResponse {
    pub token: String,
    pub session_id: String,
}

/// POST /api/copilot/playwright/init
///
/// Initializes a `PlaywrightBridge` (if none exists) and returns the auth token.
/// Dev-mode only — the frontend calls this once when dev tools are activated.
/// Requires Bearer auth (same as other authenticated endpoints).
pub async fn handle_init(
    _: crate::auth::AuthGuard,
    State(state): State<crate::server::AppState>,
) -> axum::response::Response {
    if !state.config.playwright_enabled() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "playwright_not_enabled"})),
        )
            .into_response();
    }

    let mut guard = state.playwright_state.lock().await;
    if let Some(bridge) = guard.as_ref() {
        return (
            StatusCode::OK,
            Json(InitResponse {
                token: bridge.auth_token.clone(),
                session_id: bridge.session_id.clone(),
            }),
        )
            .into_response();
    }

    let bridge = PlaywrightBridge::new();
    let resp = InitResponse {
        token: bridge.auth_token.clone(),
        session_id: bridge.session_id.clone(),
    };
    *guard = Some(bridge);
    drop(guard);

    info!("playwright: bridge initialized via API");
    (StatusCode::OK, Json(resp)).into_response()
}

/// Request body for CDP screenshot endpoint.
#[derive(Debug, Deserialize)]
#[allow(missing_docs)]
pub struct ScreenshotRequest {
    pub url: String,
    pub token: String,
}

/// Response body for CDP screenshot endpoint.
#[derive(Debug, Serialize)]
#[allow(missing_docs)]
pub struct ScreenshotResponse {
    pub image: String, // base64-encoded PNG
    pub mime: String,  // "image/png"
}

/// Request body for CDP DOM snapshot endpoint.
#[derive(Debug, Deserialize)]
#[allow(missing_docs)]
pub struct DomSnapshotRequest {
    pub url: String,
    pub token: String,
}

/// POST /api/copilot/playwright/screenshot
///
/// Takes a screenshot of the given localhost URL via CDP.
/// Requires a valid auth token for the current Playwright session.
pub async fn handle_screenshot(
    State(state): State<crate::server::AppState>,
    Json(req): Json<ScreenshotRequest>,
) -> axum::response::Response {
    if !state.config.playwright_enabled() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "playwright_not_enabled"})),
        )
            .into_response();
    }

    let guard = state.playwright_state.lock().await;
    let Some(bridge) = guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "playwright_not_available"})),
        )
            .into_response();
    };

    if !bridge.validate_token(&req.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid_token"})),
        )
            .into_response();
    }

    match bridge.take_screenshot(&req.url).await {
        Ok(png_data) => {
            let b64 = base64_encode(&png_data);
            (
                StatusCode::OK,
                Json(ScreenshotResponse {
                    image: b64,
                    mime: "image/png".to_owned(),
                }),
            )
                .into_response()
        }
        Err(e) => {
            warn!(error = %e, "screenshot endpoint error");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": e})),
            )
                .into_response()
        }
    }
}

/// POST /api/copilot/playwright/dom-snapshot
///
/// Captures a DOM snapshot of the given localhost URL via CDP.
/// Requires a valid auth token for the current Playwright session.
pub async fn handle_dom_snapshot(
    State(state): State<crate::server::AppState>,
    Json(req): Json<DomSnapshotRequest>,
) -> axum::response::Response {
    if !state.config.playwright_enabled() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "playwright_not_enabled"})),
        )
            .into_response();
    }

    let guard = state.playwright_state.lock().await;
    let Some(bridge) = guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "playwright_not_available"})),
        )
            .into_response();
    };

    if !bridge.validate_token(&req.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid_token"})),
        )
            .into_response();
    }

    match bridge.inspect_dom(&req.url).await {
        Ok(snapshot) => (StatusCode::OK, Json(snapshot)).into_response(),
        Err(e) => {
            warn!(error = %e, "dom-snapshot endpoint error");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": e})),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Base64-encode bytes without external dependency.
fn base64_encode(data: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(data)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn url_validation_accepts_localhost() {
        assert!(validate_url("http://localhost:5173/").is_ok());
        assert!(validate_url("http://127.0.0.1:8733/cockpit").is_ok());
        assert!(validate_url("http://[::1]:5173/").is_ok());
        // 127.x.x.x loopback range is accepted via Ipv4::is_loopback()
        assert!(validate_url("http://127.0.0.2:3000/").is_ok());
        assert!(validate_url("http://127.255.255.255:3000/").is_ok());
    }

    #[test]
    fn url_validation_rejects_remote() {
        assert!(validate_url("https://example.com/").is_err());
        assert!(validate_url("http://192.168.1.1/").is_err());
        assert!(validate_url("http://10.0.0.1/").is_err());
        assert!(validate_url("http://172.16.0.1/").is_err());
        // SSRF bypass: hostname starting with "127." is NOT a loopback IP
        assert!(validate_url("http://127.0.0.1.attacker.com/").is_err());
        assert!(validate_url("http://127.0.0.1.evil.example.com/").is_err());
    }

    #[test]
    fn auth_token_constant_time_comparison() {
        let bridge = PlaywrightBridge::new();
        let token = bridge.auth_token.clone();
        assert!(bridge.validate_token(&token));
        assert!(!bridge.validate_token("wrong_token"));
    }

    #[test]
    fn dom_snapshot_serialization() {
        let snap = DomSnapshot {
            url: "http://localhost:5173/".to_owned(),
            title: "Test".to_owned(),
            body: "content".to_owned(),
            elements: vec![DomElement {
                tag: "DIV".to_owned(),
                id: Some("app".to_owned()),
                role: None,
                card_role: Some("preset-chips".to_owned()),
                text: "hello".to_owned(),
            }],
        };
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"cardRole\":\"preset-chips\""));
    }
}
