//! HTTP route handlers for the credential substrate.
//!
//! Registered in [`crate::server::build_app`]:
//!
//! | Method | Path | Auth | Description |
//! |--------|------|------|-------------|
//! | POST | `/api/auth/credential/google/init` | Bearer | Start Google OAuth PKCE flow |
//! | GET | `/api/auth/credential/google/callback` | None | OAuth callback (browser redirect) |
//! | POST | `/api/auth/credential/github/device` | Bearer | Start GitHub Device Flow |
//! | POST | `/api/auth/credential/github/poll` | Bearer | Poll for GitHub token |
//! | POST | `/api/auth/credential/ollama/connect` | Bearer | Verify Ollama daemon via CLI |
//! | POST | `/api/auth/credential/{provider}/key` | Bearer | Store API key (anthropic/openai/mistral) |
//! | GET | `/api/auth/credential/{provider}/status` | Bearer | Connection state |
//! | DELETE | `/api/auth/credential/{provider}` | Bearer | Revoke stored credential |
//!
//! Security properties enforced here:
//! - OA-2: `state` UUID validated against `AppState::oauth_states`; expired entries rejected.
//! - OA-5: `redirect_uri` constructed from the locked `127.0.0.1:{port}` address.
//! - OA-7 / OA-8 / OA-10: Tokens and API keys never written to `tracing` spans.
//! - OA-9: GitHub Device Flow `interval` field enforced ≥ 5 s.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::{
    auth::AuthGuard,
    auth::credential::{
        CredentialFlow, CredentialState, OAuthPendingState, ProviderCredentialProvider,
        github::{self, GitHubCredentialProvider},
        google::{self, GoogleCredentialProvider},
        keychain,
        ollama::OllamaCredentialProvider,
    },
    server::AppState,
};

use super::pkce;

/// OAuth CSRF state TTL: 120 seconds (OA-2).
const OAUTH_STATE_TTL: Duration = Duration::from_secs(120);

/// Maximum accepted byte length for a stored API key (F10 — prevents oversized Keychain writes).
const MAX_API_KEY_BYTES: usize = 1024;

// ── Shared error helper ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: msg.into() })).into_response()
}

// ── Response types ────────────────────────────────────────────────────────────

/// Response body for `POST /api/auth/credential/google/init`.
#[derive(Debug, Serialize)]
pub struct GoogleInitResponse {
    /// Full Google authorization URL.  The frontend opens this in a new tab.
    pub redirect_url: String,
}

/// Response body for `GET /api/auth/credential/{provider}/status`.
#[derive(Debug, Serialize)]
pub struct ProviderStatusResponse {
    /// Provider identifier (e.g. `"google"`).
    pub provider: String,
    /// Current connection state.
    pub state: CredentialState,
}

// ── POST /api/auth/credential/google/init ────────────────────────────────────

/// Starts a Google OAuth 2.0 PKCE authorization flow.
///
/// Generates a fresh PKCE pair (OA-1), stores an `OAuthPendingState` keyed by
/// a UUID `state` parameter (OA-2), and returns the authorization URL.
/// The redirect URI is locked to `http://127.0.0.1:{port}/...` (OA-5).
///
/// # Errors
///
/// Returns a `503` when `LA_GOOGLE_CLIENT_ID` is not set, or `500` when the
/// authorization URL cannot be constructed.
pub async fn google_init(
    _auth: AuthGuard,
    State(state): State<AppState>,
) -> Result<Json<GoogleInitResponse>, Response> {
    let client_id = std::env::var("LA_GOOGLE_CLIENT_ID").map_err(|_| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "LA_GOOGLE_CLIENT_ID not configured",
        )
    })?;

    // OA-5: redirect_uri locked to localhost:{port} — never user-supplied.
    let redirect_uri = format!(
        "http://127.0.0.1:{}/api/auth/credential/google/callback",
        state.config.port
    );

    // OA-1: 256-bit CSPRNG verifier + S256 challenge.
    let (code_verifier, code_challenge) = pkce::generate_pkce_pair();
    let state_uuid = Uuid::new_v4();

    // OA-2: store CSRF state with locked redirect_uri and PKCE verifier.
    state.oauth_states.insert(
        state_uuid,
        OAuthPendingState {
            provider_id: "google".to_owned(),
            code_verifier,
            redirect_uri: redirect_uri.clone(),
            expires_at: Instant::now() + OAUTH_STATE_TTL,
        },
    );

    let mut auth_url = reqwest::Url::parse(google::AUTH_ENDPOINT).map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("invalid auth endpoint: {e}"),
        )
    })?;

    let scopes = google::SCOPES.join(" ");
    auth_url
        .query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &scopes)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state_uuid.to_string())
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");

    Ok(Json(GoogleInitResponse {
        redirect_url: auth_url.to_string(),
    }))
}

// ── GET /api/auth/credential/google/callback ─────────────────────────────────

/// Query parameters delivered by Google to the callback URL.
#[derive(Debug, Deserialize)]
pub struct GoogleCallbackParams {
    /// Authorization code from Google.
    pub code: Option<String>,
    /// State UUID echoed back from the init request (OA-2 CSRF check).
    pub state: Option<String>,
    /// OAuth error code when the user denied access (e.g. `"access_denied"`).
    pub error: Option<String>,
}

/// Token response from `oauth2.googleapis.com`.
#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    /// Short-lived access token (not persisted — OA-3).
    #[allow(dead_code)]
    access_token: String,
    /// Refresh token (persisted to Keychain — OA-3, OA-12).
    refresh_token: Option<String>,
}

/// Handles the OAuth callback from Google.
///
/// Validates the CSRF state (OA-2), exchanges the code for tokens, stores
/// the refresh token in the Keychain (OA-3), and returns a self-closing page.
///
/// Unauthenticated — the browser follows the Google redirect and cannot
/// carry a Bearer token.
pub async fn google_callback(
    State(state): State<AppState>,
    Query(params): Query<GoogleCallbackParams>,
) -> Response {
    if let Some(ref error) = params.error {
        let msg = format!("Google OAuth error: {error}");
        tracing::warn!(target: "credential.google", error = %error, "OAuth denied");
        return Html(error_page(&msg)).into_response();
    }

    let (Some(code), Some(state_str)) = (params.code, params.state) else {
        return Html(error_page("Missing code or state parameter")).into_response();
    };

    // OA-2: validate and consume CSRF state.
    let Ok(state_uuid) = state_str.parse::<Uuid>() else {
        tracing::warn!(target: "credential.google", "callback received non-UUID state");
        return Html(error_page("Invalid state parameter")).into_response();
    };

    let Some((_, pending)) = state.oauth_states.remove(&state_uuid) else {
        tracing::warn!(target: "credential.google", state = %state_uuid, "state not found or consumed");
        return Html(error_page("State not found or expired")).into_response();
    };

    if Instant::now() > pending.expires_at {
        tracing::warn!(target: "credential.google", state = %state_uuid, "state expired");
        return Html(error_page("OAuth state expired — please try again")).into_response();
    }

    let refresh_token = match perform_token_exchange(&code, &pending).await {
        Ok(rt) => rt,
        Err(msg) => return Html(error_page(&msg)).into_response(),
    };

    // OA-3: store refresh token via Keychain subprocess only.
    if let Err(e) = GoogleCredentialProvider.store_credential(&refresh_token) {
        tracing::error!(target: "credential.google", error = %e, "keychain store failed");
        return Html(error_page("Failed to store credential in Keychain")).into_response();
    }

    state
        .credential_store
        .insert("google".to_owned(), CredentialState::Connected);
    tracing::info!(target: "credential.google", "Google credential stored — provider connected");
    Html(success_page("Google")).into_response()
}

/// Exchanges the authorization code for a Google refresh token.
///
/// Reads `LA_GOOGLE_CLIENT_ID` and `LA_GOOGLE_CLIENT_SECRET` from the
/// environment.  The refresh token is returned; the access token is discarded
/// (OA-3 — only the refresh token is persisted).
///
/// # Errors
///
/// Returns a human-readable error message string on failure (displayed in the
/// browser page — not logged to `tracing` to avoid leaking token values).
async fn perform_token_exchange(code: &str, pending: &OAuthPendingState) -> Result<String, String> {
    let client_id = std::env::var("LA_GOOGLE_CLIENT_ID")
        .map_err(|_| "LA_GOOGLE_CLIENT_ID not configured".to_owned())?;
    let client_secret = std::env::var("LA_GOOGLE_CLIENT_SECRET")
        .map_err(|_| "LA_GOOGLE_CLIENT_SECRET not configured".to_owned())?;

    let http = reqwest::Client::builder().build().map_err(|e| {
        tracing::error!(target: "credential.google", error = %e, "HTTP client build failed");
        "Internal error during token exchange".to_owned()
    })?;

    let resp = http
        .post(google::TOKEN_ENDPOINT)
        .form(&[
            ("code", code),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", pending.redirect_uri.as_str()),
            ("code_verifier", pending.code_verifier.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| {
            tracing::error!(target: "credential.google", error = %e, "token exchange HTTP error");
            "Token exchange request failed".to_owned()
        })?;

    let token: GoogleTokenResponse = resp.json().await.map_err(|e| {
        tracing::error!(target: "credential.google", error = %e, "token response parse error");
        "Token exchange response malformed".to_owned()
    })?;

    token.refresh_token.ok_or_else(|| {
        tracing::warn!(
            target: "credential.google",
            "Google did not return a refresh_token"
        );
        "No refresh token returned — revoke access at myaccount.google.com and retry".to_owned()
    })
}

// ── GET /api/auth/credential/{provider}/status ───────────────────────────────

/// Returns the current connection state for a provider.
///
/// Checks the in-memory cache first, then falls back to a Keychain probe.
///
/// # Errors
///
/// Returns `404` for unknown provider identifiers.
pub async fn provider_status(
    _auth: AuthGuard,
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<Json<ProviderStatusResponse>, Response> {
    let credential_state = if let Some(cached) = state.credential_store.get(&provider) {
        cached.clone()
    } else {
        let service = provider_keychain_service(&provider).ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                format!("unknown provider: {provider}"),
            )
        })?;
        match tokio::task::spawn_blocking(move || keychain::keychain_get(service))
            .await
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("task join: {e}")))?
        {
            Ok(Some(_)) => CredentialState::Connected,
            Ok(None) => CredentialState::NotConnected,
            Err(e) => {
                tracing::warn!(target: "credential", provider = %provider, error = %e, "keychain probe failed");
                CredentialState::NotConnected
            }
        }
    };
    Ok(Json(ProviderStatusResponse {
        provider,
        state: credential_state,
    }))
}

// ── DELETE /api/auth/credential/{provider} ────────────────────────────────────

/// Revokes a stored credential (sign-out).
///
/// Removes the Keychain entry and updates the in-memory state cache.
///
/// # Errors
///
/// Returns `404` for unknown providers, `500` when the Keychain delete fails.
pub async fn provider_revoke(
    _auth: AuthGuard,
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<StatusCode, Response> {
    let service = provider_keychain_service(&provider).ok_or_else(|| {
        err(
            StatusCode::NOT_FOUND,
            format!("unknown provider: {provider}"),
        )
    })?;

    tokio::task::spawn_blocking(move || keychain::keychain_delete(service))
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("task join: {e}")))?
        .map_err(|e| {
            tracing::error!(target: "credential", provider = %provider, error = %e, "keychain delete failed");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to remove credential from Keychain",
            )
        })?;

    state
        .credential_store
        .insert(provider.clone(), CredentialState::SignedOut);
    tracing::info!(target: "credential", provider = %provider, "credential revoked");
    Ok(StatusCode::NO_CONTENT)
}

// ── POST /api/auth/credential/{provider}/key ──────────────────────────────────

/// Request body for storing an API key (OA-7, OA-8, OA-10).
#[derive(Debug, Deserialize)]
pub struct StoreKeyRequest {
    /// API key — never logged (OA-7/8/10).
    pub key: String,
}

/// Stores a static API key in the macOS Keychain for `ApiKey` providers.
///
/// Accepts `anthropic`, `openai`, and `mistral` as provider identifiers.
/// The key is written via Keychain subprocess only (OA-3) and never logged.
///
/// # Errors
///
/// Returns `404` for unknown providers, `500` when the Keychain write fails.
pub async fn store_api_key(
    _auth: AuthGuard,
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Json(body): Json<StoreKeyRequest>,
) -> Result<StatusCode, Response> {
    let service = provider_keychain_service(&provider).ok_or_else(|| {
        err(
            StatusCode::NOT_FOUND,
            format!("unknown provider: {provider}"),
        )
    })?;

    if body.key.len() > MAX_API_KEY_BYTES {
        return Err(err(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("API key exceeds maximum length of {MAX_API_KEY_BYTES} bytes"),
        ));
    }
    let key = body.key;
    tokio::task::spawn_blocking(move || keychain::keychain_set(service, &key))
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("task join: {e}")))?
        .map_err(|e| {
            tracing::error!(target: "credential", provider = %provider, error = %e, "keychain set failed");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to store credential in Keychain",
            )
        })?;

    state
        .credential_store
        .insert(provider.clone(), CredentialState::Connected);
    tracing::info!(target: "credential", provider = %provider, "API key stored");
    Ok(StatusCode::CREATED)
}

// ── POST /api/auth/credential/github/device ───────────────────────────────────

/// Internal GitHub device code response shape (RFC 8628 §3.2).
#[derive(Debug, Deserialize)]
struct GitHubDeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

/// Response returned to the UI for the device flow initiation.
#[derive(Debug, Serialize)]
pub struct GitHubDeviceResponse {
    /// Code the operator enters at `verification_uri`.
    pub user_code: String,
    /// URL the operator opens in a browser.
    pub verification_uri: String,
    /// Opaque device code — passed back in poll requests.
    pub device_code: String,
    /// Seconds until the device code expires.
    pub expires_in: u64,
    /// Minimum seconds between poll requests (OA-9 — enforced ≥ 5).
    pub interval: u64,
}

/// Initiates the GitHub Device Flow (RFC 8628, OA-9).
///
/// Calls `github.com/login/device/code` with the configured client ID and
/// returns the `user_code` + `verification_uri` for the operator to complete
/// authentication on any browser.
///
/// # Errors
///
/// Returns `503` when `LA_GITHUB_CLIENT_ID` is not set, `502` on GitHub
/// HTTP errors.
pub async fn github_device_init(
    _auth: AuthGuard,
    State(_state): State<AppState>,
) -> Result<Json<GitHubDeviceResponse>, Response> {
    let client_id = std::env::var("LA_GITHUB_CLIENT_ID").map_err(|_| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "LA_GITHUB_CLIENT_ID not configured",
        )
    })?;

    let http = reqwest::Client::new();
    let resp = http
        .post(github::DEVICE_CODE_URL)
        .header("Accept", "application/json")
        .form(&[("client_id", client_id.as_str())])
        .send()
        .await
        .map_err(|e| {
            tracing::error!(target: "credential.github", error = %e, "device code request failed");
            err(StatusCode::BAD_GATEWAY, "GitHub device code request failed")
        })?;

    let device: GitHubDeviceCodeResponse = resp.json().await.map_err(|e| {
        tracing::error!(target: "credential.github", error = %e, "device code parse error");
        err(
            StatusCode::BAD_GATEWAY,
            "GitHub device code response malformed",
        )
    })?;

    Ok(Json(GitHubDeviceResponse {
        user_code: device.user_code,
        verification_uri: device.verification_uri,
        device_code: device.device_code,
        expires_in: device.expires_in,
        interval: device.interval.max(5), // OA-9: never below 5 s
    }))
}

// ── POST /api/auth/credential/github/poll ────────────────────────────────────

/// Request body for polling the GitHub Device Flow.
#[derive(Debug, Deserialize)]
pub struct GitHubPollRequest {
    /// Device code returned by `github/device` — passed back to identify the
    /// in-flight authorization.
    pub device_code: String,
}

/// Poll outcome.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitHubPollStatus {
    /// Token received and stored in the Keychain.
    Connected,
    /// Operator has not yet approved — keep polling.
    Pending,
    /// GitHub says to slow down; the UI should double the interval.
    SlowDown,
    /// Operator denied access.
    Denied,
    /// Device code expired; restart the flow.
    Expired,
}

/// Poll response body.
#[derive(Debug, Serialize)]
pub struct GitHubPollResponse {
    /// Current polling outcome.
    pub status: GitHubPollStatus,
}

/// Internal GitHub token poll response (RFC 8628 §3.5).
#[derive(Debug, Deserialize)]
struct GitHubTokenPollResponse {
    access_token: Option<String>,
    error: Option<String>,
}

/// Polls GitHub for a Device Flow token (RFC 8628, OA-9).
///
/// Makes a single poll attempt against `github.com/login/oauth/access_token`.
/// On success, stores the access token in the Keychain (OA-3) and returns
/// `{ status: "connected" }`.  The UI continues polling until `connected` or
/// a terminal state (`denied` / `expired`).
///
/// # Errors
///
/// Returns `503` when `LA_GITHUB_CLIENT_ID` is not set, `502` on HTTP errors.
pub async fn github_device_poll(
    _auth: AuthGuard,
    State(state): State<AppState>,
    Json(body): Json<GitHubPollRequest>,
) -> Result<Json<GitHubPollResponse>, Response> {
    let client_id = std::env::var("LA_GITHUB_CLIENT_ID").map_err(|_| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "LA_GITHUB_CLIENT_ID not configured",
        )
    })?;

    let http = reqwest::Client::new();
    let resp = http
        .post(github::POLL_URL)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id.as_str()),
            ("device_code", body.device_code.as_str()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await
        .map_err(|e| {
            tracing::error!(target: "credential.github", error = %e, "poll request failed");
            err(StatusCode::BAD_GATEWAY, "GitHub poll request failed")
        })?;

    let poll: GitHubTokenPollResponse = resp.json().await.map_err(|e| {
        tracing::error!(target: "credential.github", error = %e, "poll response parse error");
        err(StatusCode::BAD_GATEWAY, "GitHub poll response malformed")
    })?;

    if let Some(token) = poll.access_token {
        // OA-3: store token via Keychain subprocess; never logged.
        GitHubCredentialProvider
            .store_credential(&token)
            .map_err(|e| {
                tracing::error!(target: "credential.github", error = %e, "keychain store failed");
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "failed to store GitHub credential",
                )
            })?;
        state
            .credential_store
            .insert("github".to_owned(), CredentialState::Connected);
        tracing::info!(target: "credential.github", "GitHub token stored — provider connected");
        return Ok(Json(GitHubPollResponse {
            status: GitHubPollStatus::Connected,
        }));
    }

    let status = match poll.error.as_deref() {
        Some("slow_down") => GitHubPollStatus::SlowDown,
        Some("access_denied") => GitHubPollStatus::Denied,
        Some("expired_token") => GitHubPollStatus::Expired,
        _ => GitHubPollStatus::Pending,
    };
    Ok(Json(GitHubPollResponse { status }))
}

// ── POST /api/auth/credential/ollama/connect ─────────────────────────────────

/// Verifies Ollama is reachable by running `ollama list` via CLI subprocess.
///
/// On success, stores a `"connected"` sentinel in the Keychain so the UI
/// can show connection state across restarts.  No secret is involved —
/// Ollama is a local service with no API key.
///
/// # Errors
///
/// Returns `503` when the Ollama binary is not found or the daemon is not
/// running, `500` when the Keychain write fails.
pub async fn ollama_connect(
    _auth: AuthGuard,
    State(state): State<AppState>,
) -> Result<StatusCode, Response> {
    let flow = OllamaCredentialProvider.credential_flow().map_err(|e| {
        tracing::error!(target: "credential.ollama", error = %e, "ollama binary not found");
        err(StatusCode::SERVICE_UNAVAILABLE, format!("{e}"))
    })?;

    let CredentialFlow::CliSubprocess { binary, args } = flow else {
        return Err(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "unexpected flow type for Ollama",
        ));
    };

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&binary).args(&args).output()
    })
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("task join: {e}")))?
    .map_err(|e| {
        tracing::error!(target: "credential.ollama", error = %e, "subprocess exec failed");
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "Failed to run ollama binary",
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(target: "credential.ollama", stderr = %stderr, "ollama list failed");
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "Ollama daemon not reachable",
        ));
    }

    OllamaCredentialProvider
        .store_credential("connected")
        .map_err(|e| {
            tracing::error!(target: "credential.ollama", error = %e, "keychain store failed");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to store Ollama state in Keychain",
            )
        })?;

    state
        .credential_store
        .insert("ollama".to_owned(), CredentialState::Connected);
    tracing::info!(target: "credential.ollama", "Ollama verified — provider connected");
    Ok(StatusCode::CREATED)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Maps a provider identifier to its Keychain service name.
///
/// Returns `None` for unknown providers (HTTP 404 to callers).
fn provider_keychain_service(provider: &str) -> Option<&'static str> {
    use crate::auth::credential::{
        anthropic, deepseek, github, litellm, mistral, ollama, ollama_cloud, openai, vertex,
    };
    match provider {
        "google" => Some(google::KEYCHAIN_SERVICE),
        "github" => Some(github::KEYCHAIN_SERVICE),
        "anthropic" => Some(anthropic::KEYCHAIN_SERVICE),
        "openai" => Some(openai::KEYCHAIN_SERVICE),
        "mistral" => Some(mistral::KEYCHAIN_SERVICE),
        "ollama" => Some(ollama::KEYCHAIN_SERVICE),
        "litellm" => Some(litellm::KEYCHAIN_SERVICE),
        "ollama-cloud" => Some(ollama_cloud::KEYCHAIN_SERVICE),
        "deepseek" => Some(deepseek::KEYCHAIN_SERVICE),
        "google-vertex" => Some(vertex::KEYCHAIN_SERVICE),
        _ => None,
    }
}

/// Self-closing HTML page shown after a successful OAuth callback.
fn success_page(provider: &str) -> String {
    format!(
        r"<!DOCTYPE html><html><head><title>{provider} Connected</title></head>
<body><p>{provider} credential saved. You can close this tab.</p>
<script>window.close();</script></body></html>"
    )
}

/// HTML-encode a string for safe interpolation into an HTML body.
///
/// Prevents reflected XSS when user-controlled input (e.g. `params.error`
/// from an OAuth redirect) is included in a response page.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Error HTML page shown on OAuth failure.
fn error_page(msg: &str) -> String {
    let safe = html_escape(msg);
    format!(
        r"<!DOCTYPE html><html><head><title>Authentication Error</title></head>
<body><p>Error: {safe}</p><p>Close this tab and try again.</p></body></html>"
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn provider_keychain_service_google() {
        assert_eq!(
            provider_keychain_service("google"),
            Some(google::KEYCHAIN_SERVICE)
        );
    }

    #[test]
    fn provider_keychain_service_all_byok_providers() {
        use crate::auth::credential::{
            anthropic, deepseek, github, litellm, mistral, ollama, ollama_cloud, openai, vertex,
        };
        assert_eq!(
            provider_keychain_service("github"),
            Some(github::KEYCHAIN_SERVICE)
        );
        assert_eq!(
            provider_keychain_service("anthropic"),
            Some(anthropic::KEYCHAIN_SERVICE)
        );
        assert_eq!(
            provider_keychain_service("openai"),
            Some(openai::KEYCHAIN_SERVICE)
        );
        assert_eq!(
            provider_keychain_service("mistral"),
            Some(mistral::KEYCHAIN_SERVICE)
        );
        assert_eq!(
            provider_keychain_service("ollama"),
            Some(ollama::KEYCHAIN_SERVICE)
        );
        assert_eq!(
            provider_keychain_service("litellm"),
            Some(litellm::KEYCHAIN_SERVICE)
        );
        assert_eq!(
            provider_keychain_service("ollama-cloud"),
            Some(ollama_cloud::KEYCHAIN_SERVICE)
        );
        assert_eq!(
            provider_keychain_service("deepseek"),
            Some(deepseek::KEYCHAIN_SERVICE)
        );
        assert_eq!(
            provider_keychain_service("google-vertex"),
            Some(vertex::KEYCHAIN_SERVICE)
        );
    }

    #[test]
    fn provider_keychain_service_unknown_returns_none() {
        assert!(provider_keychain_service("unknown-provider").is_none());
    }

    #[test]
    fn success_page_contains_provider_name() {
        let html = success_page("Google");
        assert!(html.contains("Google"));
        assert!(html.contains("window.close()"));
    }

    #[test]
    fn error_page_contains_message() {
        let html = error_page("test error");
        assert!(html.contains("test error"));
    }

    #[test]
    fn error_page_escapes_html_injection() {
        let html = error_page("<script>alert(1)</script>");
        assert!(
            !html.contains("<script>"),
            "XSS payload must not appear raw in output"
        );
        assert!(
            html.contains("&lt;script&gt;"),
            "payload must be HTML-escaped"
        );
    }

    #[test]
    fn html_escape_encodes_all_special_chars() {
        let escaped = html_escape("& < > \" '");
        assert_eq!(escaped, "&amp; &lt; &gt; &quot; &#x27;");
    }

    // ── Property tests (V1 — proptest Suite 3) ───────────────────────────────

    use proptest::prelude::*;

    proptest! {
        /// Any arbitrary input must never produce raw angle brackets in the output.
        /// This is the primary XSS defence — angle brackets start injected tags.
        #[test]
        fn html_escape_no_raw_angle_brackets(s in ".*") {
            let out = html_escape(&s);
            prop_assert!(!out.contains('<'), "raw '<' survived html_escape");
            prop_assert!(!out.contains('>'), "raw '>' survived html_escape");
        }

        /// OA-12 invariant: every known provider must resolve to a Keychain service name.
        /// Shrinking targets the specific provider that returns None.
        #[test]
        fn known_providers_always_have_keychain_service(
            provider in prop_oneof![
                Just("google"),
                Just("github"),
                Just("anthropic"),
                Just("openai"),
                Just("mistral"),
                Just("ollama"),
            ]
        ) {
            prop_assert!(provider_keychain_service(provider).is_some());
        }
    }

    /// OA-12 injectivity: no two providers share a Keychain service name.
    ///
    /// Proptest samples presence; this test proves the mapping is injective —
    /// a distinct invariant that presence alone does not imply.
    #[test]
    fn keychain_service_names_are_injective() {
        use crate::auth::credential::{anthropic, github, mistral, ollama, openai};
        use std::collections::HashSet;
        let services = [
            google::KEYCHAIN_SERVICE,
            github::KEYCHAIN_SERVICE,
            anthropic::KEYCHAIN_SERVICE,
            openai::KEYCHAIN_SERVICE,
            mistral::KEYCHAIN_SERVICE,
            ollama::KEYCHAIN_SERVICE,
        ];
        let unique: HashSet<_> = services.iter().collect();
        assert_eq!(
            unique.len(),
            services.len(),
            "OA-12 violation: duplicate Keychain service name among providers"
        );
    }
}
