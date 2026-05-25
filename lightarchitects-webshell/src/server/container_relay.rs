//! WebSocket relay between browser and a running agent container.
//!
//! ## Auth
//!
//! Same `Sec-WebSocket-Protocol: bearer.<token>` scheme as the native PTY
//! handler. The token is validated before the WebSocket upgrade.
//!
//! ## Container ID validation
//!
//! The `:id` path segment is validated against `^[a-f0-9]{12,64}$` before
//! any subprocess call — rejects non-hex or suspiciously short IDs with 400.
//!
//! ## Relay lifecycle
//!
//! 1. Readiness probe — `docker inspect --format {{.State.Running}}` polled
//!    at 2-second intervals for up to 10 seconds.
//! 2. `docker exec -i <id> /bin/sh` — raw byte pipe (no PTY allocated).
//! 3. Two interleaved relay halves: docker stdout → WebSocket binary frames;
//!    WebSocket text/binary frames → docker stdin.
//! 4. Drop guard — on disconnect: `docker stop --time 3` then `docker rm -f`.
//! 5. Background reaper — 5-minute interval, label-based cleanup of orphaned
//!    exited containers (`managed-by=la-hitl`).

use std::{process::Stdio, sync::OnceLock};

use axum::{
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use regex::Regex;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::{Duration, sleep, timeout},
};

use crate::{auth, server::AppState};

/// Regex for Docker container IDs: lowercase hex, 12–64 characters.
fn container_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-f0-9]{12,64}$").expect("valid static regex"))
}

/// Axum handler for `GET /api/terminal/container/:id`.
///
/// Validates the container ID format, authenticates via
/// `Sec-WebSocket-Protocol: bearer.<token>`, then upgrades to WebSocket and
/// runs the byte-pipe relay.
pub async fn ws_relay_handler(
    Path(container_id): Path<String>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Response {
    // Allowlist guard: reject non-hex or out-of-range IDs before any auth.
    if !container_id_re().is_match(&container_id) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let subproto = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth::validate_ws_subprotocol(subproto, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    ws.protocols([subproto.to_owned()])
        .on_upgrade(move |socket| async move {
            relay(socket, container_id).await;
        })
}

/// Bridge a WebSocket connection to `docker exec -i <id> /bin/sh`.
async fn relay(socket: WebSocket, container_id: String) {
    let span = tracing::info_span!("container.exec_session", container_id = %container_id);
    let _enter = span.enter();

    if !wait_for_running(&container_id).await {
        tracing::warn!(
            container_id = %container_id,
            "container not running after 10s — aborting relay"
        );
        return;
    }

    let mut child = match Command::new("docker")
        .args(["exec", "-i", &container_id, "/bin/sh"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(container_id = %container_id, error = %e, "docker exec spawn failed");
            return;
        }
    };

    let mut stdin = child.stdin.take().expect("stdin is piped");
    let mut stdout = child.stdout.take().expect("stdout is piped");

    let (mut ws_sink, mut ws_stream) = socket.split();

    // oneshot: stdout task signals EOF to the main relay loop.
    let (done_tx, mut done_rx) = tokio::sync::oneshot::channel::<()>();

    let container_id_stdout = container_id.clone();
    let stdout_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if ws_sink
                        .send(Message::Binary(buf[..n].to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
        tracing::debug!(container_id = %container_id_stdout, "container stdout closed");
        let _ = done_tx.send(());
    });

    // Drop guard: stop + remove container when relay ends (fires on any exit path).
    let _guard = ContainerDropGuard::new(container_id.clone());

    // Main relay loop: WebSocket → docker stdin.
    loop {
        tokio::select! {
            _ = &mut done_rx => break,  // container exited.
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Binary(b))) => {
                        if stdin.write_all(&b).await.is_err() { break; }
                    }
                    Some(Ok(Message::Text(t))) => {
                        if stdin.write_all(t.as_bytes()).await.is_err() { break; }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}  // Ping/Pong handled by axum internally.
                    Some(Err(e)) => {
                        tracing::debug!(container_id = %container_id, error = %e, "ws recv error");
                        break;
                    }
                }
            }
        }
    }

    // Close stdin → signals /bin/sh to exit → stdout EOF → done_tx fires.
    drop(stdin);
    stdout_task.abort();
    let _ = child.wait().await;

    tracing::info!(container_id = %container_id, "relay session closed");
}

/// Poll `docker inspect` until the container reports `Running = true`.
///
/// Times out after 10 seconds. Polls every 2 seconds.
async fn wait_for_running(container_id: &str) -> bool {
    timeout(Duration::from_secs(10), async {
        loop {
            let running = Command::new("docker")
                .args(["inspect", "--format", "{{.State.Running}}", container_id])
                .output()
                .await
                .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "true");
            if running {
                return;
            }
            sleep(Duration::from_secs(2)).await;
        }
    })
    .await
    .is_ok()
}

/// RAII guard: stops and removes the container when the relay session ends.
///
/// Uses `docker stop --time 3` (3-second grace period) then `docker rm -f`.
/// The cleanup is fire-and-forget — it runs inside the existing tokio runtime
/// so the relay call-stack can return immediately.
struct ContainerDropGuard {
    container_id: String,
}

impl ContainerDropGuard {
    fn new(container_id: String) -> Self {
        Self { container_id }
    }
}

impl Drop for ContainerDropGuard {
    fn drop(&mut self) {
        let id = self.container_id.clone();
        let _ = tokio::spawn(async move {
            let _ = Command::new("docker")
                .args(["stop", "--time", "3", &id])
                .output()
                .await;
            let _ = Command::new("docker")
                .args(["rm", "-f", &id])
                .output()
                .await;
            tracing::info!(container_id = %id, "container stopped and removed");
        });
    }
}

/// Spawn the background reaper task.
///
/// Removes exited containers labelled `managed-by=la-hitl` every 5 minutes.
/// Label-based discovery means it survives gateway restarts — no in-memory
/// state is required.
pub fn spawn_reaper() {
    tokio::spawn(async {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Ok(out) = Command::new("docker")
                .args([
                    "ps",
                    "-a",
                    "-q",
                    "--filter",
                    "label=managed-by=la-hitl",
                    "--filter",
                    "status=exited",
                ])
                .output()
                .await
            else {
                continue;
            };

            let stdout_str = String::from_utf8_lossy(&out.stdout);
            let ids: Vec<&str> = stdout_str.split_whitespace().collect();
            if ids.is_empty() {
                continue;
            }

            let mut args = vec!["rm", "-f"];
            args.extend_from_slice(&ids);
            let _ = Command::new("docker").args(&args).output().await;
            tracing::info!(
                count = ids.len(),
                "reaper removed exited la-hitl containers"
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::container_id_re;

    #[test]
    fn accepts_full_sha() {
        let sha = "a".repeat(64);
        assert!(container_id_re().is_match(&sha));
    }

    #[test]
    fn accepts_short_id() {
        let short = "abc123def456";
        assert!(container_id_re().is_match(short));
    }

    #[test]
    fn rejects_uppercase() {
        assert!(!container_id_re().is_match("ABC123DEF456"));
    }

    #[test]
    fn rejects_non_hex() {
        assert!(!container_id_re().is_match("la-my-container"));
    }

    #[test]
    fn rejects_too_short() {
        assert!(!container_id_re().is_match("abc1234")); // 7 chars
    }

    #[test]
    fn rejects_too_long() {
        let long = "a".repeat(65);
        assert!(!container_id_re().is_match(&long));
    }
}
