//! Per-connection PTY session lifecycle.
//!
//! ## Persistent PTY (per-build mode)
//!
//! When a `build` session is supplied, the PTY process lives for the lifetime
//! of the `BuildSession`, not the WebSocket connection.
//!
//! ```text
//! first WS connect  → ensure_pty_started() → spawns claude, wires channels
//! WS disconnect     → attach_ws() returns   → child keeps running
//! second WS connect → ensure_pty_started() no-op → attach_ws() resumes output
//! child exits       → pty_exited notified   → all WS subscribers close
//! ```
//!
//! The PTY stdout bytes are broadcast on `BuildSession::pty_output_tx`. Each
//! connected WebSocket subscribes independently; missed frames (e.g. while no
//! tab is open) are silently dropped from the ring-buffer — the child process
//! is unaffected.
//!
//! ## Legacy mode
//!
//! `GET /api/terminal/ws` (no build id) uses the old single-shot behaviour:
//! PTY spawned on connect, killed on disconnect.
//!
//! ## Frame format
//!
//! | Direction | Frame type | Content |
//! |-----------|-----------|---------|
//! | server → browser | `Binary` | Raw PTY stdout bytes |
//! | browser → server | `Binary` | Keystrokes / paste (raw PTY stdin) |
//! | browser → server | `Text` | JSON control: `{"type":"resize","cols":N,"rows":N}` or `{"type":"ping"}` |

use std::{
    io::{Read, Write},
    sync::Arc,
};

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, info, instrument, warn};

use crate::{config::Config, mcp_config, session::BuildSession, terminal::ws::SessionGuard};

/// Size of the blocking read buffer for PTY stdout.
const PTY_BUF: usize = 4096;

/// JSON control messages the browser sends as text frames.
///
/// Binary frames are forwarded verbatim as PTY stdin bytes; this enum
/// covers the structured control path.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    /// Resize the PTY to the given terminal dimensions.
    Resize {
        /// Number of columns.
        cols: u16,
        /// Number of rows.
        rows: u16,
    },
    /// No-op keepalive sent by the browser's reconnect logic.
    Ping,
}

/// Runs one PTY session for the lifetime of `socket`.
///
/// - `Some(session)` → persistent mode: PTY stays alive across WS reconnects.
/// - `None` → legacy single-shot mode: PTY tied to WS lifetime.
///
/// The `_guard` is dropped when this future resolves, decrementing the
/// shared session counter.
#[instrument(skip_all, name = "pty_session")]
pub async fn run_session(
    socket: WebSocket,
    config: Arc<Config>,
    build: Option<Arc<BuildSession>>,
    _guard: SessionGuard,
) {
    let result = if let Some(session) = build {
        run_persistent(socket, &config, &session).await
    } else {
        // Legacy path — no session, PTY dies with WS.
        run_inner(socket, &config, None).await
    };
    if let Err(e) = result {
        warn!(error = %e, "PTY session terminated with error");
    }
    info!("PTY session closed");
}

// ── Persistent-mode helpers ───────────────────────────────────────────────────

/// Persistent PTY entry point: ensure the child is running, then attach WS.
async fn run_persistent(
    socket: WebSocket,
    config: &Config,
    session: &BuildSession,
) -> Result<(), anyhow::Error> {
    ensure_pty_started(session, config).await?;
    attach_ws(socket, session).await
}

/// Start the PTY process for `session` if it is not already running.
///
/// Idempotent — a concurrent second call will block on the mutex and
/// observe `is_some()` once the first call completes.
async fn ensure_pty_started(session: &BuildSession, config: &Config) -> Result<(), anyhow::Error> {
    let mut pty_in_guard = session.pty_input_tx.lock().await;
    if pty_in_guard.is_some() {
        return Ok(()); // already running
    }

    // ── Open PTY pair ─────────────────────────────────────────────────────────
    let pty_sys = native_pty_system();
    let pair = pty_sys.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let master = pair.master;
    let slave = pair.slave;

    // ── Build command ─────────────────────────────────────────────────────────
    let mut host_builder = CommandBuilder::new(&config.host_cmd);
    host_builder.env(crate::config::TOKEN_ENV, &config.token);
    host_builder.cwd(&session.cwd);
    for arg in session.build_argv() {
        host_builder.arg(arg);
    }
    let gui_url = format!("http://127.0.0.1:{}", config.port);
    for (k, v) in session.build_spawn_env(&gui_url) {
        host_builder.env(k, v);
    }
    maybe_write_mcp_json(session, &gui_url);

    let child = slave.spawn_command(host_builder)?;
    drop(slave);

    // ── Wire up I/O ───────────────────────────────────────────────────────────
    let pty_reader = master.try_clone_reader()?;
    let pty_writer = master.take_writer()?;

    // Grab the killer before moving master into the session.
    let killer = child.clone_killer();

    // Store master for resize; store killer for cleanup.
    #[allow(clippy::unwrap_used)]
    {
        *session.pty_master.lock().unwrap() = Some(master);
        *session.child_killer.lock().unwrap() = Some(killer);
    }

    let (pty_in_tx, pty_in_rx) = mpsc::channel::<Vec<u8>>(128);

    // PTY stdout → broadcast channel (shared across WS subscribers).
    let output_tx = session.pty_output_tx.clone();
    let exited = Arc::clone(&session.pty_exited);
    spawn_broadcast_reader(pty_reader, output_tx, exited);

    // PTY stdin ← mpsc channel (each WS connection sends a clone of pty_in_tx).
    spawn_pty_writer(pty_writer, pty_in_rx);

    *pty_in_guard = Some(pty_in_tx);
    Ok(())
}

/// Attach one WebSocket to a running PTY session.
///
/// Returns when the WS closes or the PTY child exits. Does NOT kill the child.
async fn attach_ws(socket: WebSocket, session: &BuildSession) -> Result<(), anyhow::Error> {
    let mut pty_rx = session.pty_output_tx.subscribe();

    // Clone the stdin sender so this WS can write to the shared PTY stdin.
    let pty_in_tx: Option<mpsc::Sender<Vec<u8>>> = {
        let guard = session.pty_input_tx.lock().await;
        guard.as_ref().cloned()
    };

    let (mut ws_sink, mut ws_stream) = socket.split();
    let exited = Arc::clone(&session.pty_exited);

    loop {
        tokio::select! {
            // PTY stdout → browser
            maybe_bytes = pty_rx.recv() => {
                match maybe_bytes {
                    Ok(bytes) => {
                        if ws_sink.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    // Lagged: ring-buffer overflow — skip missed frames.
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    // Sender dropped — reader task exited, child is gone.
                    Err(broadcast::error::RecvError::Closed) => {
                        let _ = ws_sink.send(Message::Close(None)).await;
                        break;
                    }
                }
            }

            // Browser → PTY
            maybe_msg = ws_stream.next() => {
                match maybe_msg {
                    None | Some(Ok(Message::Close(_)) | Err(_)) => break,
                    Some(Ok(Message::Binary(b))) => {
                        if let Some(ref tx) = pty_in_tx {
                            let _ = tx.send(b.to_vec()).await;
                        }
                    }
                    Some(Ok(Message::Text(t))) => {
                        apply_control_message_session(session, &t);
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                }
            }

            // Child exited — close gracefully
            () = exited.notified() => {
                let _ = ws_sink.send(Message::Close(None)).await;
                break;
            }
        }
    }

    Ok(())
}

/// Core session logic for the legacy (no-build) path.
async fn run_inner(
    socket: WebSocket,
    config: &Config,
    build: Option<&BuildSession>,
) -> Result<(), anyhow::Error> {
    // ── 1. Open PTY pair ──────────────────────────────────────────────────────
    let pty_sys = native_pty_system();
    let pair = pty_sys.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let master = pair.master;
    let slave = pair.slave;

    // ── 2. Spawn host command ─────────────────────────────────────────────────
    let mut host_builder = CommandBuilder::new(&config.host_cmd);
    host_builder.env(crate::config::TOKEN_ENV, &config.token);

    if let Some(session) = build {
        host_builder.cwd(&session.cwd);
        for arg in session.build_argv() {
            host_builder.arg(arg);
        }
        let gui_url = format!("http://127.0.0.1:{}", config.port);
        for (k, v) in session.build_spawn_env(&gui_url) {
            host_builder.env(k, v);
        }
        maybe_write_mcp_json(session, &gui_url);
    } else {
        host_builder.cwd(&config.cwd);
    }
    let child = slave.spawn_command(host_builder)?;
    drop(slave);

    let pid = child.process_id();
    let mut killer = child.clone_killer();

    // ── 3. Set up I/O channels ────────────────────────────────────────────────
    let pty_reader = master.try_clone_reader()?;
    let pty_writer = master.take_writer()?;

    let (pty_out_tx, mut pty_out_rx) = mpsc::channel::<Vec<u8>>(128);
    let (pty_in_tx, pty_in_rx) = mpsc::channel::<Vec<u8>>(128);
    let (exit_tx, exit_rx) = oneshot::channel::<()>();

    // ── 4. Blocking background tasks ─────────────────────────────────────────
    spawn_mpsc_reader(pty_reader, pty_out_tx);
    spawn_pty_writer(pty_writer, pty_in_rx);
    spawn_child_waiter(child, exit_tx);

    // ── 5. Async event loop ───────────────────────────────────────────────────
    let (mut ws_sink, mut ws_stream) = socket.split();
    tokio::pin!(exit_rx);

    loop {
        tokio::select! {
            maybe_bytes = pty_out_rx.recv() => {
                let Some(bytes) = maybe_bytes else { break };
                if ws_sink.send(Message::Binary(bytes.into())).await.is_err() {
                    break;
                }
            }
            maybe_msg = ws_stream.next() => {
                match maybe_msg {
                    None | Some(Ok(Message::Close(_)) | Err(_)) => break,
                    Some(Ok(Message::Binary(b))) => {
                        let _ = pty_in_tx.send(b.to_vec()).await;
                    }
                    Some(Ok(Message::Text(t))) => {
                        apply_control_message(&*master, &t);
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                }
            }
            _ = &mut exit_rx => {
                let _ = ws_sink.send(Message::Close(None)).await;
                break;
            }
        }
    }

    // ── 6. Shutdown ───────────────────────────────────────────────────────────
    drop(pty_in_tx);
    terminate_child(pid, &mut *killer).await;
    Ok(())
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Write the project-scoped `.mcp.json` for a build session. Silently skips
/// on any I/O or resolution failure.
fn maybe_write_mcp_json(session: &BuildSession, gui_url: &str) {
    let Some(root) = lightarchitects::core::paths::root() else {
        debug!("no LA root — skipping .mcp.json write");
        return;
    };
    let gateway_bin = root.join("bin").join("lightarchitects");
    if !gateway_bin.is_file() {
        debug!(path = %gateway_bin.display(), "gateway binary not deployed — skipping .mcp.json write");
        return;
    }
    let build_id = session.build_id.to_string();
    let notify_hex = session.notify_token_hex();
    match mcp_config::write_mcp_json(&session.cwd, &gateway_bin, gui_url, &build_id, &notify_hex) {
        Ok(path) => info!(path = %path.display(), "wrote project-scoped .mcp.json"),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            warn!(error = %e, "refused to overwrite existing .mcp.json");
        }
        Err(e) => warn!(error = %e, "failed to write .mcp.json — continuing without"),
    }
}

/// PTY reader for persistent mode: forwards bytes to the broadcast channel.
/// Calls `exited.notify_waiters()` on EOF so WS subscribers can close.
fn spawn_broadcast_reader(
    reader: Box<dyn Read + Send>,
    tx: broadcast::Sender<Vec<u8>>,
    exited: Arc<tokio::sync::Notify>,
) {
    drop(tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = [0u8; PTY_BUF];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    // Ignore Err(NoReceivers) — the process stays alive even
                    // when no WS tab is open.
                    let _ = tx.send(buf[..n].to_vec());
                }
            }
        }
        exited.notify_waiters();
        debug!("PTY broadcast reader task exited");
    }));
}

/// PTY reader for legacy mode: forwards bytes to an mpsc channel.
fn spawn_mpsc_reader(reader: Box<dyn Read + Send>, tx: mpsc::Sender<Vec<u8>>) {
    drop(tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = [0u8; PTY_BUF];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
        debug!("PTY reader task exited");
    }));
}

/// Blocking writer task: drains `rx` into the PTY stdin.
fn spawn_pty_writer(writer: Box<dyn Write + Send>, rx: mpsc::Receiver<Vec<u8>>) {
    drop(tokio::task::spawn_blocking(move || {
        let mut writer = writer;
        let mut rx = rx;
        while let Some(bytes) = rx.blocking_recv() {
            if writer.write_all(&bytes).is_err() {
                break;
            }
            let _ = writer.flush();
        }
        debug!("PTY writer task exited");
    }));
}

/// Blocking waiter task: sends on `tx` when the child exits.
fn spawn_child_waiter(child: Box<dyn portable_pty::Child + Send>, tx: oneshot::Sender<()>) {
    drop(tokio::task::spawn_blocking(move || {
        let mut child = child;
        let _ = child.wait();
        let _ = tx.send(());
        debug!("child waiter task: child exited");
    }));
}

/// Apply a JSON control frame in persistent mode (resize via session's master).
fn apply_control_message_session(session: &BuildSession, msg: &str) {
    #[allow(clippy::unwrap_used)]
    let guard = session.pty_master.lock().unwrap();
    if let Some(ref master) = *guard {
        apply_control_message(master.as_ref(), msg);
    }
}

/// Parse and apply a JSON control frame to a PTY master.
fn apply_control_message(master: &(dyn MasterPty + Send), msg: &str) {
    match serde_json::from_str::<ClientMessage>(msg) {
        Ok(ClientMessage::Resize { cols, rows }) => {
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
        Ok(ClientMessage::Ping) | Err(_) => {}
    }
}

/// SIGTERM → 2 s → SIGKILL (legacy mode only).
async fn terminate_child(pid: Option<u32>, killer: &mut dyn portable_pty::ChildKiller) {
    #[cfg(unix)]
    {
        use nix::{
            sys::signal::{Signal, kill},
            unistd::Pid,
        };

        if let Some(raw_pid) = pid {
            if let Ok(signed_pid) = i32::try_from(raw_pid) {
                let _ = kill(Pid::from_raw(signed_pid), Signal::SIGTERM);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    let _ = killer.kill();
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ── ClientMessage deserialization ─────────────────────────────────────────

    #[test]
    fn client_message_resize_round_trips() {
        let json = r#"{"type":"resize","cols":120,"rows":40}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(
            msg,
            ClientMessage::Resize {
                cols: 120,
                rows: 40
            }
        ));
    }

    #[test]
    fn client_message_ping_round_trips() {
        let json = r#"{"type":"ping"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::Ping));
    }

    #[test]
    fn client_message_unknown_type_is_error() {
        let json = r#"{"type":"unknown"}"#;
        assert!(serde_json::from_str::<ClientMessage>(json).is_err());
    }

    #[test]
    fn client_message_resize_missing_field_is_error() {
        let json = r#"{"type":"resize","cols":80}"#;
        assert!(serde_json::from_str::<ClientMessage>(json).is_err());
    }

    #[test]
    fn client_message_malformed_json_is_error() {
        assert!(serde_json::from_str::<ClientMessage>("not json").is_err());
    }
}
