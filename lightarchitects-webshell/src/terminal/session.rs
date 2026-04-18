//! Per-connection PTY session lifecycle.
//!
//! [`run_session`] is the public entry point called by the WebSocket handler
//! after a successful upgrade. It:
//!
//! 1. Opens a PTY pair via [`portable_pty::native_pty_system`].
//! 2. Spawns the configured host command (`claude` by default) into the slave.
//! 3. Drops the slave fd in the parent — the child owns its copy.
//! 4. Bridges bytes bidirectionally:
//!    - PTY stdout → WebSocket binary frames (via `spawn_blocking` reader task).
//!    - WebSocket binary frames → PTY stdin (via `spawn_blocking` writer task).
//!    - WebSocket JSON text frames → PTY resize via [`ClientMessage`].
//! 5. On WS close or child exit: SIGTERM → 2 s wait → SIGKILL → reap.
//!
//! `portable-pty`'s `pre_exec` handles `setsid()` + `TIOCSCTTY` natively
//! (plan ref RG1-A1), so we need no session-leadership code of our own.
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
use tokio::sync::{mpsc, oneshot};
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
/// The `_guard` is dropped when this future resolves, decrementing the
/// shared session counter. Errors are logged; the caller does not need to
/// inspect the outcome.
///
/// ## `build` parameter
///
/// - `None` → legacy single-shot mode (`/api/terminal/ws`): spawn the
///   configured `host_cmd` in the `Config.cwd` with only `TOKEN_ENV`.
/// - `Some(session)` → per-build mode (`/api/builds/:id/terminal/ws`):
///   spawn in `session.cwd`, append `session.build_argv()` arguments,
///   inject `session.build_spawn_env(gui_url)` (adds `LA_BUILD_ID`,
///   `LA_NOTIFY_TOKEN`, `LA_GUI_URL`, and `ANTHROPIC_*` overrides for
///   Ollama backends), and write a project-scoped `.mcp.json` registering
///   the local gateway as `lightarchitects-gui-bridge`.
#[instrument(skip_all, name = "pty_session")]
pub async fn run_session(
    socket: WebSocket,
    config: Arc<Config>,
    build: Option<Arc<BuildSession>>,
    _guard: SessionGuard,
) {
    if let Err(e) = run_inner(socket, &config, build.as_deref()).await {
        warn!(error = %e, "PTY session terminated with error");
    }
}

/// Core session logic extracted for testability and to keep `run_session`'s
/// error handling separate from the I/O machinery.
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
    // Expose the webshell auth token so the child can call `/api/control`.
    host_builder.env(crate::config::TOKEN_ENV, &config.token);

    if let Some(session) = build {
        // Per-build mode: cwd from session, argv + env vars from BuildSession,
        // and a project-scoped `.mcp.json` registering the gateway under
        // `lightarchitects-gui-bridge`.
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
        // Legacy single-shot mode: use the globally configured cwd.
        host_builder.cwd(&config.cwd);
    }
    let child = slave.spawn_command(host_builder)?;

    // Close the slave fd in the parent.  The child has its own copy; closing
    // ours ensures the master side sees EOF when the child exits.
    drop(slave);

    let pid = child.process_id();
    let mut killer = child.clone_killer();

    // ── 3. Set up I/O channels ────────────────────────────────────────────────
    let pty_reader = master.try_clone_reader()?;
    let pty_writer = master.take_writer()?;

    // PTY stdout → channel → WS (bounded: back-pressure if browser is slow)
    let (pty_out_tx, mut pty_out_rx) = mpsc::channel::<Vec<u8>>(128);
    // WS binary → channel → PTY stdin
    let (pty_in_tx, pty_in_rx) = mpsc::channel::<Vec<u8>>(128);
    // Child exit notification
    let (exit_tx, exit_rx) = oneshot::channel::<()>();

    // ── 4. Blocking background tasks ─────────────────────────────────────────
    spawn_pty_reader(pty_reader, pty_out_tx);
    spawn_pty_writer(pty_writer, pty_in_rx);
    spawn_child_waiter(child, exit_tx);

    // ── 5. Async event loop ───────────────────────────────────────────────────
    let (mut ws_sink, mut ws_stream) = socket.split();
    tokio::pin!(exit_rx);

    loop {
        tokio::select! {
            // PTY stdout → browser as binary frames
            maybe_bytes = pty_out_rx.recv() => {
                let Some(bytes) = maybe_bytes else { break };
                if ws_sink.send(Message::Binary(bytes.into())).await.is_err() {
                    break;
                }
            }

            // Browser → PTY (binary bytes) or control (JSON text)
            maybe_msg = ws_stream.next() => {
                match maybe_msg {
                    None | Some(Ok(Message::Close(_)) | Err(_)) => break,
                    Some(Ok(Message::Binary(b))) => {
                        let _ = pty_in_tx.send(b.to_vec()).await;
                    }
                    Some(Ok(Message::Text(t))) => {
                        apply_control_message(&*master, &t);
                    }
                    // Ping/Pong frames: tungstenite handles Ping→Pong echo; no action needed.
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                }
            }

            // Child exited — send Close frame and end the loop
            _ = &mut exit_rx => {
                let _ = ws_sink.send(Message::Close(None)).await;
                break;
            }
        }
    }

    // ── 6. Shutdown: SIGTERM → 2 s → SIGKILL ─────────────────────────────────
    // Drop the write channel so the PTY writer task drains and exits cleanly.
    drop(pty_in_tx);
    terminate_child(pid, &mut *killer).await;
    info!("PTY session closed");
    Ok(())
}

/// Resolve the gateway binary path and attempt to write a project-scoped
/// `.mcp.json` into the build's cwd. Silently skips on any resolution or
/// I/O failure — the round trip still works via the user's global MCP
/// registration; the local `.mcp.json` just names this instance distinctly
/// so its env vars (`LA_BUILD_ID`, `LA_NOTIFY_TOKEN`, `LA_GUI_URL`) reach
/// the right gateway process.
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

/// Spawns a blocking thread that reads PTY stdout and forwards chunks to `tx`.
///
/// The task exits when the PTY fd returns EOF (child closed) or the channel
/// is closed (session dropped).
fn spawn_pty_reader(reader: Box<dyn Read + Send>, tx: mpsc::Sender<Vec<u8>>) {
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

/// Spawns a blocking thread that receives bytes from `rx` and writes to the PTY.
///
/// The task exits when `rx` is closed (channel dropped from the async side).
fn spawn_pty_writer(writer: Box<dyn Write + Send>, rx: mpsc::Receiver<Vec<u8>>) {
    drop(tokio::task::spawn_blocking(move || {
        let mut writer = writer;
        let mut rx = rx;
        while let Some(bytes) = rx.blocking_recv() {
            if writer.write_all(&bytes).is_err() {
                break;
            }
            // Flush to avoid buffering latency on interactive keystrokes.
            let _ = writer.flush();
        }
        debug!("PTY writer task exited");
    }));
}

/// Spawns a blocking thread that waits for `child` to exit.
///
/// Sends on `tx` when the wait completes so the async event loop can issue
/// a graceful WS Close frame.
fn spawn_child_waiter(child: Box<dyn portable_pty::Child + Send>, tx: oneshot::Sender<()>) {
    drop(tokio::task::spawn_blocking(move || {
        let mut child = child;
        let _ = child.wait();
        let _ = tx.send(());
        debug!("child waiter task: child exited");
    }));
}

/// Parses a JSON control text frame and applies it to the PTY.
///
/// Unknown or malformed JSON is silently ignored — the browser may send
/// frames from a newer client against an older server.
///
/// Takes `dyn MasterPty + Send` because `PtyPair::master` is
/// `Box<dyn MasterPty + Send>` and the `+ Send` marker is part of the
/// concrete vtable pointer type.
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

/// Sends SIGTERM to the child process, waits 2 seconds, then SIGKILL.
///
/// On non-Unix platforms there is no SIGTERM concept; the function falls
/// back directly to `killer.kill()` (SIGKILL via `portable-pty`).
async fn terminate_child(pid: Option<u32>, killer: &mut dyn portable_pty::ChildKiller) {
    #[cfg(unix)]
    {
        use nix::{
            sys::signal::{Signal, kill},
            unistd::Pid,
        };

        if let Some(raw_pid) = pid {
            if let Ok(signed_pid) = i32::try_from(raw_pid) {
                // SIGTERM gives the host command a chance to flush state before
                // we forcibly remove it with SIGKILL below.
                let _ = kill(Pid::from_raw(signed_pid), Signal::SIGTERM);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    // Force-kill anything still running (no-op if already exited).
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
        // "cols" present but "rows" absent — must fail cleanly.
        let json = r#"{"type":"resize","cols":80}"#;
        assert!(serde_json::from_str::<ClientMessage>(json).is_err());
    }

    #[test]
    fn client_message_malformed_json_is_error() {
        assert!(serde_json::from_str::<ClientMessage>("not json").is_err());
    }
}
