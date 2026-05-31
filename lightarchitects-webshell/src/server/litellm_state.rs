//! Runtime-switchable `LiteLLM` provider configuration.
//!
//! [`LitellmConfig`] is stored in `AppState` behind `Arc<RwLock<_>>` so that
//! `POST /api/litellm/config` can update the active endpoint, API key, and
//! model without a server restart.
//!
//! Bootstrap order:
//!   1. `LitellmConfig::from_env()` — reads `LA_LITELLM_*` env vars at startup
//!   2. `POST /api/litellm/config` — operator overwrites at runtime via keychain
//!
//! All surfaces that need an [`OpenAICompatProvider`] should call
//! [`LitellmConfig::build_provider`] rather than reading env vars directly.

use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use lightarchitects::agent::openai_compat::OpenAICompatProvider;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tokio::task::spawn_blocking;

use crate::auth::AuthGuard;
use crate::server::AppState;

/// Active `LiteLLM` endpoint configuration.
#[derive(Debug, Clone)]
pub struct LitellmConfig {
    /// Base URL of the `LiteLLM` proxy (e.g. `http://localhost:4000`).
    pub base_url: String,
    /// API key forwarded as `Authorization: Bearer <key>`.
    pub api_key: SecretString,
    /// Model name routed by the proxy (e.g. `anthropic/claude-opus-4-7`).
    pub model: String,
    /// Wall-clock timestamp of the last write to this config.
    pub updated_at: DateTime<Utc>,
}

impl Default for LitellmConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            api_key: SecretString::from(String::new()),
            model: String::new(),
            updated_at: DateTime::UNIX_EPOCH,
        }
    }
}

impl LitellmConfig {
    /// Construct from `LA_LITELLM_BASE_URL / API_KEY / MODEL` env vars.
    ///
    /// Falls back to sensible defaults so the server starts even when the
    /// vars are unset (useful in dev without a live `LiteLLM` proxy).
    #[must_use]
    pub fn from_env() -> Self {
        let base_url = std::env::var("LA_LITELLM_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:4000".to_owned());
        let api_key = SecretString::from(
            std::env::var("LA_LITELLM_API_KEY").unwrap_or_else(|_| "la-local-dev".to_owned()),
        );
        let model = std::env::var("LA_LITELLM_MODEL").unwrap_or_else(|_| "local-llama".to_owned());
        Self {
            base_url,
            api_key,
            model,
            updated_at: Utc::now(),
        }
    }

    /// Build an [`OpenAICompatProvider`] from the current config values.
    ///
    /// This is cheap — the wrapped `reqwest::Client` reuses its connection
    /// pool; only the wrapper struct is allocated.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the base URL is empty or the HTTP client cannot
    /// be constructed (extremely unlikely in practice).
    pub fn build_provider(&self) -> Result<OpenAICompatProvider, String> {
        OpenAICompatProvider::for_litellm(
            Some(self.base_url.clone()),
            self.api_key.expose_secret(),
            self.model.clone(),
        )
    }
}

/// Request body for `POST /api/litellm/config`.
#[derive(Debug, Deserialize)]
pub struct ConfigUpdateRequest {
    /// `LiteLLM` proxy base URL (e.g. `http://localhost:4000`).
    pub base_url: String,
    /// API key forwarded as `Bearer` token.
    pub api_key: String,
    /// Model name (e.g. `anthropic/claude-opus-4-7`).
    pub model: String,
}

/// Response body for `GET /api/litellm/config`.
#[derive(Debug, Serialize)]
pub struct ConfigStatusResponse {
    /// Active `LiteLLM` proxy base URL.
    pub base_url: String,
    /// Active model name routed by the proxy.
    pub model: String,
    /// Whether an API key is currently stored in the Keychain.
    pub has_key: bool,
    /// Timestamp of the last config update.
    pub updated_at: DateTime<Utc>,
}

/// `POST /api/litellm/config` — store key in keychain + update `AppState`.
///
/// Stores the API key in the macOS Keychain (`la-litellm-credential`) and
/// writes all three fields to `AppState.litellm_config` atomically.
///
/// # Errors
///
/// Returns 400 if validation fails; 500 if the keychain write fails.
pub async fn update_config(
    State(state): State<AppState>,
    _auth: AuthGuard,
    Json(req): Json<ConfigUpdateRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    if req.api_key.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "api_key must not be empty".to_owned(),
        ));
    }
    // F-3: length caps prevent oversized input reaching AppState / reqwest.
    if req.base_url.len() > 512 {
        return Err((
            StatusCode::BAD_REQUEST,
            "base_url exceeds 512-byte limit".to_owned(),
        ));
    }
    if req.model.len() > 256 {
        return Err((
            StatusCode::BAD_REQUEST,
            "model exceeds 256-byte limit".to_owned(),
        ));
    }
    // F-2: SSRF guard — only https (any host) or http (localhost-only) accepted.
    let parsed = url::Url::parse(&req.base_url)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid base_url: {e}")))?;
    match parsed.scheme() {
        "https" => {}
        "http" => {
            let host = parsed.host_str().unwrap_or("");
            if !matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]") {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "http base_url only permitted for localhost; use https for remote proxies"
                        .to_owned(),
                ));
            }
        }
        scheme => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unsupported scheme '{scheme}' — use https or http://localhost"),
            ));
        }
    }

    let key = req.api_key.clone();
    spawn_blocking(move || {
        crate::auth::credential::keychain::keychain_set(
            crate::auth::credential::litellm::KEYCHAIN_SERVICE,
            &key,
        )
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut cfg = state.litellm_config.write().await;
    cfg.base_url = req.base_url;
    cfg.api_key = SecretString::from(req.api_key);
    cfg.model = req.model;
    cfg.updated_at = Utc::now();

    tracing::info!(
        target: "litellm.config",
        base_url = %cfg.base_url,
        model = %cfg.model,
        "LiteLLM config updated by operator"
    );

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/litellm/config` — return current config (key redacted).
pub async fn get_config(
    State(state): State<AppState>,
    _auth: AuthGuard,
) -> Json<ConfigStatusResponse> {
    let cfg = state.litellm_config.read().await;
    let has_key = !cfg.api_key.expose_secret().is_empty();
    Json(ConfigStatusResponse {
        base_url: cfg.base_url.clone(),
        model: cfg.model.clone(),
        has_key,
        updated_at: cfg.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;

    // ── Unit tests ───────────────────────────────────────────────────────────

    #[test]
    fn from_env_defaults_to_localhost() {
        let cfg = LitellmConfig::from_env();
        assert!(!cfg.base_url.is_empty());
        assert!(!cfg.model.is_empty());
    }

    #[test]
    fn build_provider_succeeds_with_defaults() {
        let cfg = LitellmConfig {
            base_url: "http://localhost:4000".to_owned(),
            api_key: SecretString::from("test-key"),
            model: "local-llama".to_owned(),
            updated_at: Utc::now(),
        };
        assert!(cfg.build_provider().is_ok());
    }

    // ── Property tests — SSRF guard invariants (F-2/F-3) ─────────────────────

    fn validate_base_url(base_url: &str) -> Result<(), (StatusCode, String)> {
        if base_url.len() > 512 {
            return Err((
                StatusCode::BAD_REQUEST,
                "base_url exceeds 512-byte limit".to_owned(),
            ));
        }
        let parsed = url::Url::parse(base_url)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid base_url: {e}")))?;
        match parsed.scheme() {
            "https" => Ok(()),
            "http" => {
                let host = parsed.host_str().unwrap_or("");
                if matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]") {
                    Ok(())
                } else {
                    Err((
                        StatusCode::BAD_REQUEST,
                        "http only for localhost".to_owned(),
                    ))
                }
            }
            scheme => Err((
                StatusCode::BAD_REQUEST,
                format!("unsupported scheme '{scheme}'"),
            )),
        }
    }

    #[test]
    fn ssrf_guard_accepts_https_any_host() {
        assert!(validate_base_url("https://litellm.example.com").is_ok());
        assert!(validate_base_url("https://api.openai.com/v1").is_ok());
        assert!(validate_base_url("https://10.0.0.1:4000").is_ok()); // https ok even for RFC-1918
    }

    #[test]
    fn ssrf_guard_accepts_http_localhost_variants() {
        assert!(validate_base_url("http://localhost:4000").is_ok());
        assert!(validate_base_url("http://127.0.0.1:4000").is_ok());
        assert!(validate_base_url("http://localhost").is_ok());
    }

    #[test]
    fn ssrf_guard_rejects_http_remote_host() {
        assert!(validate_base_url("http://litellm.example.com").is_err());
        assert!(validate_base_url("http://10.0.0.1").is_err()); // RFC-1918 over http
        assert!(validate_base_url("http://169.254.169.254/metadata").is_err()); // IMDS
    }

    #[test]
    fn ssrf_guard_rejects_non_http_schemes() {
        assert!(validate_base_url("ftp://example.com").is_err());
        assert!(validate_base_url("file:///etc/passwd").is_err());
        assert!(validate_base_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn ssrf_guard_rejects_malformed_url() {
        assert!(validate_base_url("not_a_url").is_err());
        assert!(validate_base_url("").is_err());
    }

    #[test]
    fn length_cap_rejects_oversized_base_url() {
        let oversized = format!("https://example.com/{}", "a".repeat(500));
        assert!(validate_base_url(&oversized).is_err());
    }

    // ── Regression tests — auth/validation error paths ────────────────────────

    #[test]
    fn default_config_has_unix_epoch_sentinel() {
        // Regression: sentinel epoch distinguishes "never configured" from any real update.
        let cfg = LitellmConfig::default();
        assert_eq!(cfg.updated_at, DateTime::UNIX_EPOCH);
    }

    #[test]
    fn default_config_has_key_false_when_empty() {
        let cfg = LitellmConfig::default();
        let has_key = !cfg.api_key.expose_secret().is_empty();
        assert!(!has_key, "default config should report has_key=false");
    }

    #[test]
    fn config_status_response_never_exposes_raw_key() {
        // Regression: GET /api/litellm/config must not expose the actual key value.
        // `ConfigStatusResponse` has no `api_key` field — this is a compile-time
        // guarantee enforced by the type. The test verifies `has_key` is a bool,
        // not the key string, and that the struct does not even accept a key value.
        let cfg = LitellmConfig {
            base_url: "https://litellm.example.com".to_owned(),
            api_key: SecretString::from("sk-real-secret"),
            model: "gpt-4o".to_owned(),
            updated_at: Utc::now(),
        };
        let has_key = !cfg.api_key.expose_secret().is_empty();
        let resp = ConfigStatusResponse {
            base_url: cfg.base_url.clone(),
            model: cfg.model.clone(),
            has_key,
            updated_at: cfg.updated_at,
        };
        // has_key is a bool — the raw key is structurally absent from the response type.
        assert!(resp.has_key);
        assert_eq!(resp.base_url, "https://litellm.example.com");
        assert_eq!(resp.model, "gpt-4o");
    }

    // ── Performance assertion — build_provider() must be sub-millisecond (p99) ──

    #[test]
    #[allow(clippy::unwrap_used)]
    fn build_provider_constructs_under_1ms_p99() {
        // `build_provider()` is called on every LLM request across all 4 surfaces.
        // It must remain allocation-light: URL parse + reqwest::Client construction.
        const N: usize = 1_000;
        const P99_LIMIT_MICROS: u128 = 1_000; // 1 ms

        let cfg = LitellmConfig {
            base_url: "http://localhost:4000".to_owned(),
            api_key: SecretString::from("sk-bench-key"),
            model: "anthropic/claude-opus-4-7".to_owned(),
            updated_at: DateTime::UNIX_EPOCH,
        };

        let mut timings: Vec<u128> = Vec::with_capacity(N);
        for _ in 0..N {
            let t0 = std::time::Instant::now();
            let _ = cfg.build_provider().unwrap();
            timings.push(t0.elapsed().as_micros());
        }

        timings.sort_unstable();
        let p99_idx = (N * 99 / 100).min(N - 1); // integer percentile, no float cast
        let p99 = timings[p99_idx];

        assert!(
            p99 < P99_LIMIT_MICROS,
            "build_provider() p99={p99}µs exceeds {P99_LIMIT_MICROS}µs limit"
        );
    }
}
