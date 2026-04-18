// SAFETY: this file manipulates process-wide env vars inside unsafe blocks.
// Test cases are serialized via ENV_LOCK so unsafe env mutations are single-threaded.
//
// `clippy::await_holding_lock` is allowed here because `ENV_LOCK` is held
// intentionally for the full test body: dropping it before the `.await`
// would let a sibling test mutate the env mid-run. The lock is only ever
// taken by this test file, so there's no deadlock risk — just a serial
// critical section larger than clippy's default prefers.
#![allow(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::await_holding_lock
)]

//! Phase A wire test — proves the end-to-end chain:
//!
//! `ui::dispatch(action, params)` → reads `LA_GUI_URL` + `LA_BUILD_ID` +
//! `LA_NOTIFY_TOKEN` from env → constructs `<GUI_URL>/api/builds/<BUILD_ID>/notify`
//! → POSTs with `X-LA-Notify-Token` header and event JSON body → gets HTTP 200.
//!
//! Everything between the MCP tool entry point and the HTTP request landing
//! at the webshell notify endpoint is exercised here. Downstream webshell
//! behavior (token validation, broadcast, SSE fan-out) is Phase C's wire test.
//!
//! This file is intentionally light on assertions for things already covered
//! by unit tests (JSON shape for each action — see `ui.rs::tests`) and focuses
//! on the integration seam: does the call actually produce a real HTTP POST
//! with the expected headers and body?

use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;

/// Captured request body for a single `/api/builds/:id/notify` POST.
#[derive(Debug, Clone)]
struct Captured {
    build_id: String,
    notify_token: String,
    body: Value,
}

/// Shared state for the mock webshell.
#[derive(Default)]
struct MockState {
    captured: Mutex<Vec<Captured>>,
    /// When set, the next POST will return this status + body instead of 200.
    force_response: Mutex<Option<(StatusCode, String)>>,
}

async fn capture_notify(
    Path(build_id): Path<String>,
    State(state): State<Arc<MockState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, String) {
    let token = headers
        .get("x-la-notify-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();

    {
        let mut captured = state.captured.lock().expect("poisoned");
        captured.push(Captured {
            build_id,
            notify_token: token,
            body,
        });
    }

    if let Some((status, body)) = state.force_response.lock().expect("poisoned").take() {
        (status, body)
    } else {
        (StatusCode::OK, String::new())
    }
}

/// Spawn a mock webshell on an ephemeral port and return (`base_url`, state).
async fn spawn_mock_webshell() -> (String, Arc<MockState>) {
    let state = Arc::new(MockState::default());
    let app = Router::new()
        .route("/api/builds/{build_id}/notify", post(capture_notify))
        .with_state(Arc::clone(&state));

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    let base_url = format!("http://127.0.0.1:{}", addr.port());

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("mock server run");
    });

    // Tiny yield so the listener is reachable before the test fires its POST.
    tokio::task::yield_now().await;

    (base_url, state)
}

/// Scoped env-var setter that cleans up on drop.
///
/// Env mutation is process-wide; these tests must not run concurrently with
/// other tests that read/write `LA_*` vars. `cargo test` runs test binaries
/// sequentially per file by default, but within a file we take no chances —
/// every test acquires a static mutex before touching env.
struct EnvGuard {
    keys: Vec<&'static str>,
    prior: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn set(pairs: &[(&'static str, &str)]) -> Self {
        let mut prior = Vec::with_capacity(pairs.len());
        let mut keys = Vec::with_capacity(pairs.len());
        for (k, v) in pairs {
            prior.push((*k, std::env::var(*k).ok()));
            keys.push(*k);
            // SAFETY: tests holding `ENV_LOCK` below are exclusive w.r.t. each other;
            // no other thread in the test binary mutates these keys.
            unsafe {
                std::env::set_var(*k, *v);
            }
        }
        Self { keys, prior }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in self.prior.drain(..) {
            unsafe {
                match v {
                    Some(original) => std::env::set_var(k, original),
                    None => std::env::remove_var(k),
                }
            }
        }
        self.keys.clear();
    }
}

/// Serializes env-var-touching tests within this file.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ── Wire tests ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn wire_dispatch_posts_to_mock_webshell() {
    let _env_lock = ENV_LOCK.lock().expect("lock");
    let (base_url, state) = spawn_mock_webshell().await;

    let _env = EnvGuard::set(&[
        ("LA_GUI_URL", &base_url),
        ("LA_BUILD_ID", "550e8400-e29b-41d4-a716-446655440000"),
        ("LA_NOTIFY_TOKEN", "secret-notify-token-abc"),
    ]);

    let result = lightarchitects_gateway::core_tools::ui::dispatch(
        "ui_focus_pillar",
        json!({"pillar": "ARCH"}),
    )
    .await
    .expect("dispatch ok");

    // Response envelope is the standard text_result shape.
    let text = result["content"][0]["text"]
        .as_str()
        .expect("text in content");
    assert!(text.contains("\"ok\":true"), "envelope: {text}");
    assert!(text.contains("focus_pillar"), "envelope: {text}");

    // Mock received exactly one POST with the expected payload.
    let captured = state.captured.lock().expect("lock");
    assert_eq!(captured.len(), 1, "exactly one POST expected");
    let call = &captured[0];
    assert_eq!(call.build_id, "550e8400-e29b-41d4-a716-446655440000");
    assert_eq!(call.notify_token, "secret-notify-token-abc");
    assert_eq!(call.body["type"], "focus_pillar");
    assert_eq!(call.body["pillar"], "ARCH");
}

#[tokio::test]
async fn wire_dispatch_degrades_silently_when_env_unset() {
    let _env_lock = ENV_LOCK.lock().expect("lock");
    // No EnvGuard — intentionally keep env unset for this test.
    // If the test harness inherited real LA_* vars we'd need to clear them,
    // but `cargo test` does not export these.
    let prior = (
        std::env::var("LA_GUI_URL").ok(),
        std::env::var("LA_BUILD_ID").ok(),
        std::env::var("LA_NOTIFY_TOKEN").ok(),
    );
    unsafe {
        std::env::remove_var("LA_GUI_URL");
        std::env::remove_var("LA_BUILD_ID");
        std::env::remove_var("LA_NOTIFY_TOKEN");
    }

    let result = lightarchitects_gateway::core_tools::ui::dispatch(
        "ui_refresh_sitrep",
        json!({}),
    )
    .await
    .expect("dispatch ok (silent degradation)");

    let text = result["content"][0]["text"]
        .as_str()
        .expect("text in content");
    assert!(text.contains("\"degraded\":true"), "envelope: {text}");

    // Restore
    unsafe {
        if let Some(v) = prior.0 {
            std::env::set_var("LA_GUI_URL", v);
        }
        if let Some(v) = prior.1 {
            std::env::set_var("LA_BUILD_ID", v);
        }
        if let Some(v) = prior.2 {
            std::env::set_var("LA_NOTIFY_TOKEN", v);
        }
    }
}

#[tokio::test]
async fn wire_dispatch_rejects_non_localhost_gui_url() {
    let _env_lock = ENV_LOCK.lock().expect("lock");
    let _env = EnvGuard::set(&[
        // Not localhost — must be rejected by the SSRF guard before any
        // network I/O happens.
        ("LA_GUI_URL", "http://198.51.100.42:8733"),
        ("LA_BUILD_ID", "550e8400-e29b-41d4-a716-446655440000"),
        ("LA_NOTIFY_TOKEN", "secret"),
    ]);

    let err = lightarchitects_gateway::core_tools::ui::dispatch(
        "ui_notify",
        json!({"message": "should not arrive anywhere"}),
    )
    .await
    .expect_err("non-localhost URL must be rejected");

    // `validate_local_url` returns `GatewayError::Internal(...)` per
    // the gateway's security module conventions.
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("localhost"),
        "error message should mention localhost: {msg}"
    );
}

#[tokio::test]
async fn wire_dispatch_propagates_http_5xx_from_webshell() {
    let _env_lock = ENV_LOCK.lock().expect("lock");
    let (base_url, state) = spawn_mock_webshell().await;

    // Force the next response to be a 500.
    *state.force_response.lock().expect("lock") = Some((
        StatusCode::INTERNAL_SERVER_ERROR,
        "token rejected".to_owned(),
    ));

    let _env = EnvGuard::set(&[
        ("LA_GUI_URL", &base_url),
        ("LA_BUILD_ID", "550e8400-e29b-41d4-a716-446655440000"),
        ("LA_NOTIFY_TOKEN", "wrong-token"),
    ]);

    let err = lightarchitects_gateway::core_tools::ui::dispatch(
        "ui_focus_pillar",
        json!({"pillar": "SEC"}),
    )
    .await
    .expect_err("5xx response must bubble up as an error");

    let msg = format!("{err}");
    assert!(
        msg.contains("500") || msg.to_lowercase().contains("internal"),
        "error must reference the status / reason: {msg}"
    );
}

#[tokio::test]
async fn wire_dispatch_sends_notify_message_payload() {
    let _env_lock = ENV_LOCK.lock().expect("lock");
    let (base_url, state) = spawn_mock_webshell().await;

    let _env = EnvGuard::set(&[
        ("LA_GUI_URL", &base_url),
        ("LA_BUILD_ID", "build-notify-001"),
        ("LA_NOTIFY_TOKEN", "tok-2"),
    ]);

    lightarchitects_gateway::core_tools::ui::dispatch(
        "ui_notify",
        json!({"level": "warn", "message": "build drift detected"}),
    )
    .await
    .expect("dispatch ok");

    let captured = state.captured.lock().expect("lock");
    let call = captured.last().expect("at least one POST");
    assert_eq!(call.body["type"], "notify");
    assert_eq!(call.body["level"], "warn");
    assert_eq!(call.body["message"], "build drift detected");
}
