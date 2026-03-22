use crate::{AuthConfig, AuthError, KeyReader};
use axum::{Router, extract::Query, response::Html, routing::get};
use std::future::IntoFuture;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing::{debug, info};
use url::Url;

/// Parameters received on the localhost callback.
#[derive(serde::Deserialize)]
struct CallbackParams {
    key: Option<String>,
    error: Option<String>,
    state: Option<String>,
}

/// Run the browser-based auth login flow.
///
/// 1. Binds to 127.0.0.1 on an ephemeral port
/// 2. Opens the browser to lightarchitects.io/auth/cli with PKCE state parameter
/// 3. Waits for the callback with the API key
/// 4. Saves the key locally
pub async fn auth_login(config: &AuthConfig) -> Result<String, AuthError> {
    // Generate a random state parameter for CSRF protection
    let state = generate_state();

    // Bind to 127.0.0.1 on ephemeral port (RFC 8252 §8.3: NOT localhost, NOT 0.0.0.0)
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| AuthError::LoginFailed(format!("Failed to bind: {e}")))?;

    let local_addr = listener
        .local_addr()
        .map_err(|e| AuthError::LoginFailed(format!("Failed to get local addr: {e}")))?;

    let callback_url = format!("http://127.0.0.1:{}/callback", local_addr.port());
    debug!("Callback URL: {callback_url}");

    // Build the auth URL
    let mut auth_url = Url::parse(&format!("{}/auth/cli", config.api_base_url))
        .map_err(|e| AuthError::LoginFailed(format!("Invalid base URL: {e}")))?;

    auth_url
        .query_pairs_mut()
        .append_pair("callback_url", &callback_url)
        .append_pair("state", &state);

    // Open browser
    info!("Opening browser for authentication...");
    println!("\nOpening: {auth_url}\n");
    println!("If the browser doesn't open, visit the URL above manually.\n");

    if let Err(e) = open::that(auth_url.as_str()) {
        eprintln!("Failed to open browser: {e}");
        eprintln!("Please visit the URL above manually.");
    }

    // Set up one-shot channel for the callback result.
    // Wrapped in Arc<Mutex<Option>> because axum requires Clone on handlers,
    // but oneshot::Sender is intentionally non-Clone (single-use).
    let (tx, rx) = oneshot::channel::<Result<String, AuthError>>();
    let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));

    let expected_state = state.clone();
    let app = Router::new().route(
        "/callback",
        get(move |Query(params): Query<CallbackParams>| async move {
            let result = handle_callback(params, &expected_state);
            // Take the sender (first call wins, subsequent calls are no-ops)
            if let Some(sender) = tx.lock().ok().and_then(|mut guard| guard.take()) {
                let _ = sender.send(result);
            }
            Html(CALLBACK_HTML.to_string())
        }),
    );

    // Run server with timeout
    let timeout = config.login_timeout;
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    );

    let key = tokio::select! {
        result = rx => {
            result.map_err(|_| AuthError::LoginFailed("Callback channel closed".to_string()))??
        }
        _ = tokio::time::sleep(timeout) => {
            return Err(AuthError::LoginTimeout { seconds: timeout.as_secs() });
        }
        result = server.into_future() => {
            result.map_err(|e| AuthError::LoginFailed(format!("Server error: {e}")))?;
            return Err(AuthError::LoginFailed("Server exited unexpectedly".to_string()));
        }
    };

    // Save the key locally
    KeyReader::save(config, &key)?;
    info!("API key saved to {}", config.key_file_path.display());

    Ok(key)
}

fn handle_callback(params: CallbackParams, expected_state: &str) -> Result<String, AuthError> {
    // Verify state parameter (CSRF protection)
    match &params.state {
        Some(state) if state == expected_state => {}
        Some(_) => {
            return Err(AuthError::LoginFailed(
                "State parameter mismatch — possible CSRF attack".to_string(),
            ));
        }
        None => {
            return Err(AuthError::LoginFailed(
                "Missing state parameter".to_string(),
            ));
        }
    }

    // Check for error
    if let Some(error) = params.error {
        return Err(AuthError::LoginFailed(error));
    }

    // Extract the key
    match params.key {
        Some(key) if !key.is_empty() => Ok(key),
        _ => Err(AuthError::LoginFailed("No API key in callback".to_string())),
    }
}

fn generate_state() -> String {
    use sha2::{Digest, Sha256};
    let random_bytes: [u8; 32] = rand_bytes();
    let mut hasher = Sha256::new();
    hasher.update(random_bytes);
    hex::encode(hasher.finalize())[..16].to_string()
}

fn rand_bytes() -> [u8; 32] {
    use sha2::Digest;
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let mut bytes = [0u8; 32];
    // Use process ID + time as entropy source (adequate for state parameter)
    let combined = format!(
        "{seed}{}{}",
        std::process::id(),
        seed.wrapping_mul(6364136223846793005)
    );
    let hash = sha2::Sha256::digest(combined.as_bytes());
    bytes.copy_from_slice(&hash);
    bytes
}

const CALLBACK_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
  <title>Light Architects — Authenticated</title>
  <style>
    body {
      font-family: system-ui, sans-serif;
      background: #05050a;
      color: #fff;
      display: flex;
      align-items: center;
      justify-content: center;
      height: 100vh;
      margin: 0;
    }
    .card {
      text-align: center;
      padding: 3rem;
      background: rgba(255,255,255,0.05);
      border: 1px solid rgba(255,255,255,0.1);
      border-radius: 1rem;
      max-width: 400px;
    }
    h1 { color: #D4AF37; margin-bottom: 0.5rem; }
    p { color: rgba(255,255,255,0.6); }
  </style>
</head>
<body>
  <div class="card">
    <h1>Authenticated</h1>
    <p>You can close this tab and return to your terminal.</p>
  </div>
</body>
</html>"#;
