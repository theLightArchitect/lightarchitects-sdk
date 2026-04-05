//! [`AyinClient`] — HTTP client for the AYIN viewer REST API.
//!
//! The AYIN viewer exposes two endpoints at `localhost:3742`:
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | `GET` | `/api/sessions` | List all trace actors and their recorded dates |
//! | `GET` | `/api/spans/:actor/:date` | Retrieve spans for a specific actor+date |
//!
//! Requires the AYIN viewer `LaunchAgent` (`io.lightarchitects.ayin`) to be
//! running. Start it with:
//!
//! ```bash
//! launchctl kickstart -k gui/$(id -u)/io.lightarchitects.ayin
//! ```
//!
//! # Feature gate
//!
//! This module is only compiled when the `http-client` Cargo feature is enabled:
//!
//! ```toml
//! lightarchitects-ayin = { path = "...", features = ["http-client"] }
//! ```

use std::fmt::Write as FmtWrite;

use serde::Deserialize;

// ── Response types ─────────────────────────────────────────────────────────────

/// A single trace session entry from `GET /api/sessions`.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionEntry {
    /// Actor name (e.g. `"lightarchitects-sdk"`, `"corso"`, `"soul-mcp"`).
    pub actor: String,
    /// Date string in `YYYY-MM-DD` format.
    pub date: String,
}

/// Response from `GET /api/sessions`.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionList {
    /// All actor+date pairs with recorded spans.
    pub sessions: Vec<SessionEntry>,
}

/// A single span from `GET /api/spans/:actor/:date`.
#[derive(Debug, Clone, Deserialize)]
pub struct SpanRecord {
    /// Span identifier.
    #[serde(default)]
    pub id: Option<String>,
    /// Action / method name that was called.
    #[serde(default)]
    pub action: Option<String>,
    /// Outcome: `"continue"`, `"error"`, or similar.
    #[serde(default)]
    pub outcome: Option<String>,
    /// Wall-clock start time (ISO 8601).
    #[serde(default)]
    pub started_at: Option<String>,
    /// Duration in milliseconds.
    #[serde(default)]
    pub duration_ms: Option<f64>,
    /// Error message, when `outcome` is `"error"`.
    #[serde(default)]
    pub error: Option<String>,
    /// Raw span fields not yet modelled here.
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Response from `GET /api/spans/:actor/:date`.
#[derive(Debug, Clone, Deserialize)]
pub struct SpanList {
    /// Spans recorded for the requested actor+date, in chronological order.
    pub spans: Vec<SpanRecord>,
}

// ── AyinClient ─────────────────────────────────────────────────────────────────

/// HTTP client for the AYIN viewer REST API.
///
/// Connects to the AYIN viewer running at `localhost:3742` (or a custom base
/// URL) and provides typed access to its two endpoints.
///
/// # Example
///
/// ```no_run
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use lightarchitects_ayin::AyinClient;
///
/// let client = AyinClient::new();
/// let sessions = client.sessions().await?;
/// for s in &sessions.sessions {
///     println!("{} — {}", s.actor, s.date);
/// }
///
/// if let Some(first) = sessions.sessions.first() {
///     let spans = client.spans(&first.actor, &first.date).await?;
///     println!("{} spans", spans.spans.len());
/// }
/// # Ok(()) }
/// ```
pub struct AyinClient {
    base_url: String,
    http: reqwest::Client,
}

impl AyinClient {
    /// Create a client targeting the default AYIN viewer at `http://localhost:3742`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_base_url("http://localhost:3742")
    }

    /// Create a client targeting a custom base URL.
    ///
    /// Trailing slashes are stripped automatically.
    ///
    /// # Security note
    ///
    /// This client is designed for the AYIN viewer running at `localhost:3742`.
    /// Passing a non-localhost URL is allowed for testing but risks SSRF in
    /// server-side contexts — only pass values from trusted, controlled sources.
    #[must_use]
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        if !base_url.starts_with("http://localhost") && !base_url.starts_with("https://localhost") {
            tracing::warn!(
                url = %base_url,
                "AyinClient: non-localhost base_url — only use values from trusted sources"
            );
        }
        Self {
            base_url,
            http: reqwest::Client::new(),
        }
    }

    /// List all trace sessions (actor + date pairs) recorded by AYIN.
    ///
    /// Calls `GET /api/sessions`.
    ///
    /// # Errors
    ///
    /// Returns an error if the viewer is unreachable or returns a non-2xx status.
    pub async fn sessions(&self) -> Result<SessionList, reqwest::Error> {
        let url = format!("{}/api/sessions", self.base_url);
        self.http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json::<SessionList>()
            .await
    }

    /// Retrieve spans for a specific `actor` on `date` (`YYYY-MM-DD`).
    ///
    /// Calls `GET /api/spans/:actor/:date`.
    ///
    /// `actor` and `date` are percent-encoded before insertion into the URL path
    /// to prevent path traversal and query-string injection.
    ///
    /// # Errors
    ///
    /// Returns an error if the viewer is unreachable, the session doesn't exist,
    /// or the response is a non-2xx status.
    pub async fn spans(&self, actor: &str, date: &str) -> Result<SpanList, reqwest::Error> {
        let url = format!(
            "{}/api/spans/{}/{}",
            self.base_url,
            encode_path_segment(actor),
            encode_path_segment(date),
        );
        self.http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json::<SpanList>()
            .await
    }
}

impl Default for AyinClient {
    fn default() -> Self {
        Self::new()
    }
}

// ── URL helpers ────────────────────────────────────────────────────────────────

/// Percent-encode a single URL path segment using RFC 3986 unreserved characters.
///
/// Replaces every byte that is not an unreserved character (`A-Z a-z 0-9 - _ . ~`)
/// with its `%XX` representation. This prevents path traversal (`../`) and
/// query-string injection (`?key=value`) when caller-supplied strings are
/// interpolated directly into a URL path.
fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            // `write!` to `String` is infallible.
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}
