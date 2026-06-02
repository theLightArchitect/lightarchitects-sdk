//! Subprocess bridge — spawns `lightarchitects run --output-format stream-json`
//! and translates NDJSON stdout into `AgentEvent` broadcasts.
//!
//! ## Lifecycle
//!
//! ```text
//! lazy_init() on first SSE/WS connect
//!   → spawn lightarchitects run --output-format stream-json --cwd <build.cwd>
//!   → stdout task parses NDJSON → AgentEvent → event_tx
//!   → stdin task reads control_rx → NDJSON → cli stdin
//!   → on bridge drop: SIGKILL child, close channels
//! ```
//!
//! ## NDJSON line format (lightarchitects-cli → bridge)
//!
//! The CLI emits its own format; `translate_cli_line` maps it to `AgentEvent`.
//! Final turn result arrives as `{"type":"result","subtype":"success","result":"..."}`;
//! this is translated to `AgentEvent::Text { chunk }` + `AgentEvent::Complete`.

use std::process::Stdio;
use std::sync::Arc;

use dashmap::DashMap;
use secrecy::ExposeSecret;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{info, warn};

use crate::config::AgentSession;
use crate::copilot::{resolve_binary, resolve_mistral_api_key};
use crate::session::BuildSession;

use super::protocol::{AgentEvent, ControlMessage};

/// Maximum length of a single NDJSON line from the CLI stdout.
///
/// Lines exceeding this are truncated and discarded as a `DoS` defence.
const MAX_NDJSON_LINE: usize = 1_048_576; // 1 MiB

/// Spawn the agent bridge for `session` and wire it into `event_tx`/`control_rx`.
///
/// Returns the child process handle so the caller can store it for lifecycle
/// management (kill on session drop).  Background tasks are spawned for
/// stdout parsing and stdin writing.
///
/// # Fallback behaviour
///
/// If the CLI binary does not exist or spawn fails, the function returns
/// `None` and emits an `AgentEvent::Error` on `event_tx`.
pub async fn spawn_bridge(
    session: &BuildSession,
    event_tx: broadcast::Sender<AgentEvent>,
    control_tx: mpsc::Sender<ControlMessage>,
    control_rx: mpsc::Receiver<ControlMessage>,
    permission_queue: Arc<DashMap<String, oneshot::Sender<bool>>>,
) -> Option<Child> {
    let is_vibe = matches!(session.agent, AgentSession::MistralVibe(_));
    let binary = if is_vibe {
        resolve_binary("vibe-acp")
    } else {
        resolve_binary("lightarchitects")
    };
    let mut cmd = tokio::process::Command::new(&binary);

    // Validate cwd before passing it to the child.
    let workdir = match validate_cwd(&session.cwd).await {
        Ok(path) => path,
        Err(e) => {
            warn!(error = %e, cwd = %session.cwd.display(), "invalid cwd for bridge");
            let _ = event_tx.send(AgentEvent::Error {
                message: e,
                recoverable: Some(false),
            });
            return None;
        }
    };

    if is_vibe {
        // vibe-acp: ACP mode, pure NDJSON stdin/stdout — no CLI path flags.
        cmd.current_dir(&workdir);
    } else {
        // `--stream-events` was the original flag but the CLI uses the subcommand form:
        // `lightarchitects run --output-format stream-json --cwd <dir> --build-id <uuid>`.
        // The run-loop reads prompts from stdin and emits NDJSON lines to stdout until EOF.
        cmd.arg("run")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--cwd")
            .arg(&workdir)
            .arg("--build-id")
            .arg(session.build_id.to_string());
    }

    // NOTE: ANTHROPIC_API_KEY is intentionally NOT injected here.
    // The CLI resolves its own credentials (keychain / config file) so that
    // a compromised agent process cannot exfiltrate the key via /proc/self/environ.
    // See C4 in agent module security review.

    cmd.kill_on_drop(true)
        .env_clear()
        .env("PATH", crate::copilot::augmented_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    // Whitelist safe env vars if present in parent.
    for key in [
        "HOME",
        "USER",
        "SHELL",
        "RUST_LOG",
        "LLM_BACKEND",
        "OLLAMA_BASE_URL",
        "OLLAMA_MODEL",
    ] {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    for (key, val) in std::env::vars() {
        if key.starts_with("LA_") || key.starts_with("LIGHTARCHITECTS_") {
            cmd.env(key, val);
        }
    }
    // Inject Mistral API key explicitly for vibe-acp — env_clear above strips it.
    if is_vibe {
        if let Some(key) = resolve_mistral_api_key() {
            cmd.env("MISTRAL_API_KEY", key.expose_secret());
        }
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to spawn lightarchitects-cli bridge");
            let _ = event_tx.send(AgentEvent::Error {
                message: "bridge_spawn_failed".to_owned(),
                recoverable: Some(false),
            });
            return None;
        }
    };

    let Some(stdin) = child.stdin.take() else {
        warn!("child stdin not piped after spawn");
        return None;
    };
    let Some(stdout) = child.stdout.take() else {
        warn!("child stdout not piped after spawn");
        return None;
    };

    info!(build_id = %session.build_id, binary = %binary, "agent bridge spawned");

    // ── stdout parsing task ───────────────────────────────────────────────────
    let event_tx_stdout = event_tx.clone();
    tokio::spawn(parse_stdout(
        stdout,
        event_tx_stdout,
        control_tx,
        permission_queue,
    ));

    // ── stdin / control task ────────────────────────────────────────────────────
    tokio::spawn(drive_stdin(stdin, control_rx));

    Some(child)
}

/// Parse NDJSON lines from the CLI stdout and emit `AgentEvent`s.
///
/// Reads stdout in 8 KiB chunks and enforces `MAX_NDJSON_LINE` *before*
/// accumulation so a malicious or buggy child cannot OOM the webshell
/// with a single unbounded line.
async fn parse_stdout(
    mut stdout: ChildStdout,
    event_tx: broadcast::Sender<AgentEvent>,
    control_tx: mpsc::Sender<ControlMessage>,
    permission_queue: Arc<DashMap<String, oneshot::Sender<bool>>>,
) {
    let mut chunk = [0u8; 8192];
    let mut line = Vec::with_capacity(4096);
    let mut saw_complete = false;
    let mut overflow = false;

    loop {
        match stdout.read(&mut chunk).await {
            Ok(0) => {
                if !line.is_empty() && !overflow {
                    process_line(
                        &line,
                        &event_tx,
                        &mut saw_complete,
                        &permission_queue,
                        &control_tx,
                    );
                }
                if !saw_complete {
                    let _ = event_tx.send(AgentEvent::Complete {
                        reason: super::protocol::TerminationReason::Error {
                            message: "stdout closed before completion".to_owned(),
                        },
                    });
                }
                break;
            }
            Ok(n) => {
                for &b in &chunk[..n] {
                    if b == b'\n' {
                        if !overflow && !line.is_empty() {
                            process_line(
                                &line,
                                &event_tx,
                                &mut saw_complete,
                                &permission_queue,
                                &control_tx,
                            );
                        }
                        line.clear();
                        overflow = false;
                    } else if line.len() < MAX_NDJSON_LINE {
                        line.push(b);
                    } else {
                        overflow = true;
                    }
                }
                if overflow && line.is_empty() {
                    // We are in overflow state and just hit a newline
                    // (handled above); if still overflowing mid-line,
                    // warn once per line.
                    warn!("NDJSON line exceeds max length; discarding");
                    let _ = event_tx.send(AgentEvent::Error {
                        message: "NDJSON line too long".to_owned(),
                        recoverable: Some(true),
                    });
                }
            }
            Err(e) => {
                warn!(error = %e, "stdout read error");
                let _ = event_tx.send(AgentEvent::Error {
                    message: "stdout read error".to_owned(),
                    recoverable: Some(false),
                });
                let _ = event_tx.send(AgentEvent::Complete {
                    reason: super::protocol::TerminationReason::Error {
                        message: "stdout read error".to_owned(),
                    },
                });
                break;
            }
        }
    }
}

/// Parse one accumulated NDJSON line and broadcast it.
///
/// When `AgentEvent::PermissionRequest` is parsed, the `call_id` is inserted
/// into `permission_queue` with a `oneshot::Sender<bool>`.  A resolution task
/// is spawned that awaits the bool and writes the CLI-format approve/deny JSON
/// to `control_tx` (routed to the CLI's `read_control_stdin`).
fn process_line(
    raw: &[u8],
    event_tx: &broadcast::Sender<AgentEvent>,
    saw_complete: &mut bool,
    permission_queue: &Arc<DashMap<String, oneshot::Sender<bool>>>,
    control_tx: &mpsc::Sender<ControlMessage>,
) {
    let Ok(line) = std::str::from_utf8(raw) else {
        let _ = event_tx.send(AgentEvent::Text {
            chunk: String::from_utf8_lossy(raw).into_owned(),
        });
        return;
    };
    if line.trim().is_empty() {
        return;
    }
    match serde_json::from_str::<AgentEvent>(line) {
        Ok(ev) => {
            if matches!(ev, AgentEvent::Complete { .. }) {
                *saw_complete = true;
            }
            // Wire permission requests into the approval queue before broadcasting.
            if let AgentEvent::PermissionRequest { ref call_id, .. } = ev {
                if permission_queue.len() < super::MAX_PENDING_PERMISSIONS {
                    let (tx, rx) = oneshot::channel::<bool>();
                    permission_queue.insert(call_id.clone(), tx);
                    let call_id_owned = call_id.clone();
                    let ctrl = control_tx.clone();
                    tokio::spawn(async move {
                        let approved = rx.await.unwrap_or(false);
                        let msg = if approved {
                            ControlMessage::ApprovePermission {
                                request_id: call_id_owned,
                            }
                        } else {
                            ControlMessage::DenyPermission {
                                request_id: call_id_owned,
                                reason: None,
                            }
                        };
                        let _ = ctrl.send(msg).await;
                    });
                } else {
                    warn!(call_id = %call_id, "permission_queue at capacity — request will timeout (fail-secure)");
                }
            }
            let _ = event_tx.send(ev);
        }
        Err(_) => {
            // The CLI's `run --output-format stream-json` emits a different wire
            // format than `AgentEvent`. Translate known CLI event shapes before
            // falling back to raw text display.
            match translate_cli_line(line) {
                Some(CliLineKind::Result { text }) => {
                    if !text.is_empty() {
                        let _ = event_tx.send(AgentEvent::Text { chunk: text });
                    }
                    *saw_complete = true;
                    let _ = event_tx.send(AgentEvent::Complete {
                        reason: super::protocol::TerminationReason::Complete,
                    });
                }
                Some(CliLineKind::Error { message }) => {
                    let _ = event_tx.send(AgentEvent::Error {
                        message,
                        recoverable: Some(false),
                    });
                }
                Some(CliLineKind::Ignore) => {} // mode, context, lifecycle noise
                None => {
                    // Unknown format — display raw for operator debugging.
                    let _ = event_tx.send(AgentEvent::Text {
                        chunk: format!("{line}\n"),
                    });
                }
            }
        }
    }
}

/// Classify a CLI-format NDJSON line that did not parse as `AgentEvent`.
///
/// The `lightarchitects` CLI's `run --output-format stream-json` loop emits a
/// different wire format than the webshell's `AgentEvent` enum.  This function
/// maps the known CLI event shapes so `process_line` can emit the correct
/// `AgentEvent` variants rather than displaying raw JSON to the operator.
enum CliLineKind {
    Result { text: String },
    Error { message: String },
    Ignore,
}

fn translate_cli_line(line: &str) -> Option<CliLineKind> {
    let val: serde_json::Value = serde_json::from_str(line).ok()?;
    match val.get("type")?.as_str()? {
        "result" => {
            let subtype = val
                .get("subtype")
                .and_then(|s| s.as_str())
                .unwrap_or("success");
            if subtype == "error" {
                Some(CliLineKind::Error {
                    message: val
                        .get("error")
                        .and_then(|e| e.as_str())
                        .unwrap_or("unknown error")
                        .to_owned(),
                })
            } else {
                // Prefer "result" field; fall back to "text" for older CLI builds.
                let text = val
                    .get("result")
                    .or_else(|| val.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_owned();
                Some(CliLineKind::Result { text })
            }
        }
        // Lifecycle / context events emitted by the CLI loop — not meaningful to the browser.
        "mode" | "context" | "strategy_step" | "strategy_halt" | "hitl_request" => {
            Some(CliLineKind::Ignore)
        }
        _ => None,
    }
}

/// Read `ControlMessage`s from the control channel and write NDJSON to stdin.
async fn drive_stdin(stdin: ChildStdin, mut control_rx: mpsc::Receiver<ControlMessage>) {
    let mut writer = BufWriter::new(stdin);
    while let Some(msg) = control_rx.recv().await {
        // `run_stream_json_loop` reads stdin lines raw — plain text passes
        // directly as the user prompt.  Other control messages remain JSON.
        let line = match msg {
            ControlMessage::SendMessage { text } => format!("{text}\n"),
            ControlMessage::ApprovePermission { request_id } => {
                // CLI reads {"type":"approve","call_id":"..."} per StreamingApprovalGate wire format.
                format!(
                    "{}\n",
                    serde_json::json!({"type":"approve","call_id":request_id})
                )
            }
            ControlMessage::DenyPermission { request_id, .. } => {
                format!(
                    "{}\n",
                    serde_json::json!({"type":"deny","call_id":request_id})
                )
            }
            ControlMessage::Interrupt => {
                format!("{}\n", serde_json::json!({"action":"interrupt"}))
            }
            ControlMessage::Steer { text } => {
                format!("{}\n", serde_json::json!({"action":"steer","text":text}))
            }
            ControlMessage::SetSystemPrompt { text } => {
                format!(
                    "{}\n",
                    serde_json::json!({"action":"set_system_prompt","text":text})
                )
            }
            ControlMessage::ExecutePlan => {
                format!("{}\n", serde_json::json!({"action":"execute_plan"}))
            }
            ControlMessage::Ping => continue,
        };
        if let Err(e) = writer.write_all(line.as_bytes()).await {
            warn!(error = %e, "bridge stdin write failed");
            break;
        }
        if let Err(e) = writer.flush().await {
            warn!(error = %e, "bridge stdin flush failed");
            break;
        }
    }
}

/// Fallback single-shot bridge for when the CLI does not support streaming.
///
/// Spawns `lightarchitects-cli run <message>` per `SendMessage` control and
/// emits the response as `Text` + `Complete`.  Runs in the caller's task
/// (not background) so it can be swapped in when streaming is unavailable.
///
/// # Security
///
/// Does NOT pass `--yes`; the CLI's standard permission-approval flow is used.
/// If the bridge is dead and the user sends a `SendMessage`, the fallback
/// requires explicit tool approval just like the streaming path.
pub async fn run_fallback_turn(
    session: &BuildSession,
    event_tx: broadcast::Sender<AgentEvent>,
    text: &str,
) {
    let binary = resolve_binary("lightarchitects");

    // Validate cwd before passing it to the child.
    let cwd = match validate_cwd(&session.cwd).await {
        Ok(path) => path,
        Err(e) => {
            warn!(error = %e, cwd = %session.cwd.display(), "invalid cwd for fallback");
            let _ = event_tx.send(AgentEvent::Error {
                message: e,
                recoverable: Some(false),
            });
            return;
        }
    };

    let output = match tokio::process::Command::new(&binary)
        .arg("run")
        .arg(text)
        .arg("--no-splash")
        .arg("--cwd")
        .arg(&cwd)
        .env("PATH", crate::copilot::augmented_path())
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!(error = %e, "failed to spawn fallback cli");
            let _ = event_tx.send(AgentEvent::Error {
                message: "bridge_run_failed".to_owned(),
                recoverable: Some(true),
            });
            return;
        }
    };

    let success = output.status.success();
    let code = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stdout.trim().is_empty() {
        stderr.into_owned()
    } else {
        stdout.into_owned()
    };

    let _ = event_tx.send(AgentEvent::Text { chunk: text });

    if success {
        let _ = event_tx.send(AgentEvent::Complete {
            reason: super::protocol::TerminationReason::Complete,
        });
    } else {
        warn!(code = ?code, "fallback cli exited with non-zero code");
        let _ = event_tx.send(AgentEvent::Error {
            message: "bridge_cli_exit_error".to_owned(),
            recoverable: Some(true),
        });
        let _ = event_tx.send(AgentEvent::Complete {
            reason: super::protocol::TerminationReason::Error {
                message: "bridge_cli_exit_error".to_owned(),
            },
        });
    }
}

/// Validate that `cwd` is a safe working directory for the agent child.
///
/// 1. Canonicalises the path (resolves symlinks) — async to avoid blocking
///    the Tokio runtime.
/// 2. Verifies it is an existing directory.
///
/// Error messages are intentionally generic to avoid leaking absolute paths
/// or OS-level details to the browser.
async fn validate_cwd(cwd: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let canon = tokio::fs::canonicalize(cwd)
        .await
        .map_err(|_| "cwd path is invalid or inaccessible".to_string())?;
    if !canon.is_dir() {
        return Err("cwd is not a directory".to_string());
    }
    Ok(canon)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::needless_raw_string_hashes
)]
mod tests {
    use super::*;
    use dashmap::DashMap;
    use std::sync::Arc;
    use tokio::sync::{broadcast, mpsc, oneshot};

    /// Helper: build a throwaway `permission_queue` + `control_tx` for `process_line` tests.
    fn dummy_pq_ctrl() -> (
        Arc<DashMap<String, oneshot::Sender<bool>>>,
        mpsc::Sender<ControlMessage>,
    ) {
        let pq = Arc::new(DashMap::new());
        let (ctrl_tx, _ctrl_rx) = mpsc::channel(4);
        (pq, ctrl_tx)
    }

    #[tokio::test]
    async fn validate_cwd_rejects_missing_path() {
        let result = validate_cwd(std::path::Path::new("/nonexistent/path/12345")).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "cwd path is invalid or inaccessible");
    }

    #[tokio::test]
    async fn validate_cwd_accepts_existing_dir() {
        let tmp = std::env::temp_dir();
        let result = validate_cwd(&tmp).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_dir());
    }

    #[tokio::test]
    async fn validate_cwd_rejects_file() {
        let tmpfile = std::env::temp_dir().join(format!("la-test-file-{}", uuid::Uuid::new_v4()));
        std::fs::write(&tmpfile, "x").unwrap();
        let result = validate_cwd(&tmpfile).await;
        std::fs::remove_file(&tmpfile).unwrap();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "cwd is not a directory");
    }

    #[test]
    fn process_line_parses_valid_agent_event() {
        let (tx, mut rx) = broadcast::channel(4);
        let (pq, ctrl) = dummy_pq_ctrl();
        let mut saw = false;
        let json = r#"{"type":"text","chunk":"hello"}"#;
        process_line(json.as_bytes(), &tx, &mut saw, &pq, &ctrl);
        assert!(rx.try_recv().is_ok());
        assert!(!saw);
    }

    #[test]
    fn process_line_sets_saw_complete_on_complete_event() {
        let (tx, _rx) = broadcast::channel(4);
        let (pq, ctrl) = dummy_pq_ctrl();
        let mut saw = false;
        let json = r#"{"type":"complete","reason":{"kind":"complete"}}"#;
        process_line(json.as_bytes(), &tx, &mut saw, &pq, &ctrl);
        assert!(saw);
    }

    #[test]
    fn process_line_emits_text_on_unrecognised_json() {
        let (tx, mut rx) = broadcast::channel(4);
        let (pq, ctrl) = dummy_pq_ctrl();
        let mut saw = false;
        let json = r"not json";
        process_line(json.as_bytes(), &tx, &mut saw, &pq, &ctrl);
        let ev = rx.try_recv().expect("text event emitted");
        assert!(matches!(ev, AgentEvent::Text { .. }));
    }

    #[test]
    fn process_line_emits_text_on_invalid_utf8() {
        let (tx, mut rx) = broadcast::channel(4);
        let (pq, ctrl) = dummy_pq_ctrl();
        let mut saw = false;
        let bytes = vec![0x80, 0x81, 0x82];
        process_line(&bytes, &tx, &mut saw, &pq, &ctrl);
        let ev = rx.try_recv().expect("text event emitted");
        assert!(matches!(ev, AgentEvent::Text { .. }));
    }

    #[tokio::test]
    async fn process_line_inserts_permission_request_into_queue() {
        let (tx, mut rx) = broadcast::channel(4);
        let (pq, ctrl) = dummy_pq_ctrl();
        let mut saw = false;
        let json = r#"{"type":"permission_request","call_id":"test-call-id","tool":"Bash","summary":"rm -rf /","agent_id":"agent-1","timeout_secs":300}"#;
        process_line(json.as_bytes(), &tx, &mut saw, &pq, &ctrl);
        // Event should be broadcast.
        assert!(rx.try_recv().is_ok());
        // permission_queue should contain the call_id.
        assert!(pq.contains_key("test-call-id"), "call_id must be queued");
    }
}
