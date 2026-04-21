//! Copilot chat handler — per-turn subprocess spawning with disk-persistent sessions.
//!
//! `Lightarchitects` backend: each HTTP request spawns a fresh `claude --print` process.
//! Session continuity via `--session-id` (Turn 1) / `--resume` (Turn 2+) with disk persistence.
//!
//! `Codex` backend: each HTTP request spawns `codex exec` (Turn 1) or
//! `codex exec resume <thread_id>` (Turn 2+) with disk-persistent session continuity.
//!
//! `LightarchitectsNative` backend: persistent subprocess with piped I/O.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter, Lines},
    process::{Child, ChildStdin, ChildStdout},
};
use uuid::Uuid;

use crate::{
    auth,
    config::{AgentSession, ClaudeBackend, CodexBackend},
    server::AppState,
    session::BuildSession,
};

/// JSON body for `POST /api/builds/:id/copilot`.
#[derive(Debug, Deserialize)]
pub struct CopilotRequest {
    /// User message text (may include injected build context from the frontend).
    pub message: String,
}

/// Per-session agent state held behind `tokio::sync::Mutex<Option<CopilotProcess>>`.
///
/// **`Lightarchitects`**, **`Codex`**: only `session_id` is populated; stdin/stdout/child are `None`.
/// Per-turn processes are short-lived and not stored here.
///
/// **`LightarchitectsNative`**: all fields populated; child is killed on drop via
/// `kill_on_drop(true)` (RAII cleanup).
pub struct CopilotProcess {
    /// Session ID for conversation continuity: passed as `--resume` on the next turn
    /// (`Lightarchitects`) or extracted from stdout (`Codex`/`LightarchitectsNative`).
    pub session_id: Option<String>,
    /// Persistent stdin (`Codex`, `LightarchitectsNative` only).
    stdin: Option<BufWriter<ChildStdin>>,
    /// Persistent stdout reader (`Codex`, `LightarchitectsNative` only).
    stdout: Option<Lines<BufReader<ChildStdout>>>,
    /// Subprocess handle — `kill_on_drop(true)` sends SIGKILL on drop.
    _child: Option<Child>,
}

impl CopilotProcess {
    /// Seed a copilot slot with a pre-existing session UUID so the next
    /// turn resumes that conversation (`claude --resume <id>` or
    /// `codex exec resume <id>`). No subprocess is spawned — Lightarchitects
    /// and Codex backends re-spawn per turn and only need `session_id`.
    #[must_use]
    pub fn seed_from_session_id(session_id: String) -> Self {
        Self {
            session_id: Some(session_id),
            stdin: None,
            stdout: None,
            _child: None,
        }
    }
}

/// `POST /api/builds/:id/copilot` — dispatch to subprocess or HTTP backend.
pub async fn copilot_chat_handler(
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(body): Json<CopilotRequest>,
) -> impl IntoResponse {
    let authz = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !auth::validate_bearer(authz, &state.config.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(session) = state.builds.get(id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "build_not_found" })),
        )
            .into_response();
    };
    let result = match &session.agent {
        AgentSession::Lightarchitects(ClaudeBackend::Ollama(cfg)) => {
            // Stateless HTTP backend — existing behaviour.
            call_ollama(&cfg.base_url, &cfg.model, &cfg.auth_token, &body.message).await
        }
        // Per-turn or persistent subprocess: Lightarchitects(Anthropic/OllamaLaunch), Codex(*), Native.
        AgentSession::Lightarchitects(
            ClaudeBackend::Anthropic | ClaudeBackend::OllamaLaunch(_),
        )
        | AgentSession::Codex(_)
        | AgentSession::LightarchitectsNative(_) => {
            call_subprocess(&body.message, &session.copilot_proc, &session).await
        }
    };
    match result {
        Ok(text) => (StatusCode::OK, Json(json!({ "response": text }))).into_response(),
        Err(reason) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "provider_error", "reason": reason })),
        )
            .into_response(),
    }
}

/// Detect end-of-turn from an NDJSON line for the `LightarchitectsNative` backend.
///
/// Returns `Some(text)` when `line` is the `{"type":"result","subtype":"success"}` event.
/// Returns `None` for any other line (keep reading).
fn parse_turn_end(line: &str, _session: &BuildSession) -> Option<String> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
        return None;
    };
    if val["type"].as_str() == Some("result") && val["subtype"].as_str() == Some("success") {
        Some(val["result"].as_str().unwrap_or("").to_owned())
    } else {
        None
    }
}

/// Spawn one turn of a `claude --print` subprocess for `Lightarchitects` backends.
///
/// Uses `--output-format stream-json --verbose` (required combination for `--print`).
/// Turn 1 (no `prev_session_id`): claude assigns a new session UUID returned in the result.
/// Turn 2+ (`prev_session_id` is `Some`): `--resume <id>` continues the prior conversation
/// from disk — giving full multi-turn context without a persistent subprocess.
///
/// # Errors
///
/// Returns a descriptive string on spawn failure, non-zero exit, or missing result event.
async fn run_print_turn(
    message: &str,
    session: &BuildSession,
    prev_session_id: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let AgentSession::Lightarchitects(backend) = &session.agent else {
        return Err("run_print_turn: not a Lightarchitects session".to_owned());
    };

    let mut c = tokio::process::Command::new("claude");
    for arg in session.build_argv() {
        c.arg(arg);
    }
    // --verbose is mandatory when combining --print with --output-format stream-json.
    c.arg("--output-format").arg("stream-json");
    c.arg("--verbose");
    c.arg("--dangerously-skip-permissions");
    c.arg("--print").arg("-p").arg(message);
    // Pin the child's working directory to the build's cwd. This matters
    // critically for `--resume <id>`: claude derives the on-disk session
    // file path from the cwd's project hash, so a child spawned in the
    // wrong directory will look in the wrong project folder and exit 1
    // when the UUID isn't found. Turn-to-turn continuity within a single
    // webshell run works with inherited cwd by accident; session-sync
    // (resuming a session created in a different process tree) exposes
    // the need to set it explicitly.
    c.current_dir(&session.cwd);
    c.env_remove("ANTHROPIC_API_KEY");
    match backend {
        ClaudeBackend::OllamaLaunch(lc) => {
            c.env("ANTHROPIC_BASE_URL", &lc.base_url);
            c.env("ANTHROPIC_AUTH_TOKEN", "ollama");
            c.env("ANTHROPIC_API_KEY", "");
            c.env("ANTHROPIC_DEFAULT_SONNET_MODEL", &lc.model);
            c.env("ANTHROPIC_DEFAULT_OPUS_MODEL", &lc.model);
            c.env("ANTHROPIC_DEFAULT_HAIKU_MODEL", &lc.model);
            c.arg("--model").arg(&lc.model);
        }
        ClaudeBackend::Anthropic | ClaudeBackend::Ollama(_) => {}
    }
    if let Some(id) = prev_session_id {
        c.arg("--resume").arg(id);
    }
    c.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = c.output().await.map_err(|e| format!("spawn claude: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut result_text: Option<String> = None;
    let mut found_session_id: Option<String> = None;

    for line in stdout.lines() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(id) = val["session_id"].as_str() {
                found_session_id = Some(id.to_owned());
            }
            if val["type"].as_str() == Some("result") && val["subtype"].as_str() == Some("success")
            {
                result_text = Some(val["result"].as_str().unwrap_or("").to_owned());
            }
        }
    }

    result_text.map(|t| (t, found_session_id)).ok_or_else(|| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.trim().is_empty() {
            format!("no result event in claude output (exit {})", output.status)
        } else {
            format!("claude: {}", stderr.trim())
        }
    })
}

/// Spawn one turn of `codex exec` for `Codex` backends.
///
/// Turn 1 (no `prev_session_id`): `codex exec "message" --json --skip-git-repo-check
/// --dangerously-bypass-approvals-and-sandbox -m <model>`.
/// Turn 2+ (`prev_session_id` is `Some`): `codex exec resume <id> "message" --json ...`.
/// Session continuity via `thread_id` extracted from `{"type":"thread.started"}` event.
///
/// # Errors
///
/// Returns a descriptive string on spawn failure, non-zero exit, or missing result.
async fn run_codex_turn(
    message: &str,
    session: &BuildSession,
    prev_session_id: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let AgentSession::Codex(cfg) = &session.agent else {
        return Err("run_codex_turn: not a Codex session".to_owned());
    };

    let mut c = tokio::process::Command::new("codex");
    if let Some(id) = prev_session_id {
        c.arg("exec").arg("resume").arg(id).arg(message);
    } else {
        c.arg("exec").arg(message);
    }
    c.arg("--json")
        .arg("--skip-git-repo-check")
        .arg("--dangerously-bypass-approvals-and-sandbox");
    match &cfg.backend {
        // OpenAi: defer to ~/.codex/config.toml for model selection.
        // Passing -m overrides the user's config and may fail if the model
        // name doesn't match the account type (e.g. "o3" on ChatGPT accounts).
        CodexBackend::OpenAi => {}
        CodexBackend::OllamaLaunch(lc) => {
            c.arg("-m").arg(&cfg.model);
            c.env("OPENAI_BASE_URL", format!("{}/v1", lc.base_url));
            c.env("OPENAI_API_KEY", "ollama");
        }
    }
    // Pin the child's working directory to the build's cwd — same reason
    // as run_print_turn: `codex exec resume <id>` looks up the session
    // file relative to the current project, so cwd must match what the
    // session was originally created in.
    c.current_dir(&session.cwd);
    c.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = c.output().await.map_err(|e| format!("spawn codex: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut thread_id: Option<String> = None;
    let mut text = String::new();
    let mut turn_done = false;
    let mut turn_error: Option<String> = None;

    for line in stdout.lines() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if val["type"].as_str() == Some("thread.started") {
                if let Some(id) = val["thread_id"].as_str() {
                    thread_id = Some(id.to_owned());
                }
            }
            if val["type"].as_str() == Some("item.completed")
                && val["item"]["type"].as_str() == Some("agent_message")
            {
                if let Some(t) = val["item"]["text"].as_str() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(t);
                }
            }
            if val["type"].as_str() == Some("turn.completed") {
                turn_done = true;
            }
            // turn.failed: surface the model/API error directly.
            if val["type"].as_str() == Some("turn.failed") {
                let msg = val["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown turn failure");
                turn_error = Some(msg.to_owned());
            }
        }
    }

    if let Some(err) = turn_error {
        return Err(format!("codex turn failed: {err}"));
    }
    if turn_done {
        Ok((text, thread_id))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(if stderr.trim().is_empty() {
            format!("no turn.completed in codex output (exit {})", output.status)
        } else {
            format!("codex: {}", stderr.trim())
        })
    }
}

/// Spawn a persistent agent subprocess for the `LightarchitectsNative` backend.
///
/// | Session | Binary | Extra env |
/// |---------|--------|-----------|
/// | `LightarchitectsNative` | `<cfg.binary>` | none |
///
/// # Errors
///
/// Returns a descriptive string if the subprocess cannot be spawned or if
/// stdin/stdout handles are unavailable.
fn spawn_copilot(session: &BuildSession) -> Result<CopilotProcess, String> {
    let mut cmd = match &session.agent {
        // lÆx0 native binary — reads prompts from stdin, emits stream-json NDJSON.
        // build_argv() is intentionally NOT passed: lÆx0 does not accept
        // Claude Code-specific flags (--add-dir, --agent, --allowedTools).
        AgentSession::LightarchitectsNative(cfg) => {
            let mut c = tokio::process::Command::new(&cfg.binary);
            c.arg("run").arg("--output-format").arg("stream-json");
            c
        }
        _ => return Err("spawn_copilot called for non-persistent-subprocess backend".to_owned()),
    };

    cmd.kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn agent: {e}"))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "agent stdin unavailable".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "agent stdout unavailable".to_owned())?;

    Ok(CopilotProcess {
        session_id: None,
        stdin: Some(BufWriter::new(stdin)),
        stdout: Some(BufReader::new(stdout).lines()),
        _child: Some(child),
    })
}

/// Send `message` to the agent and return its response.
///
/// `Lightarchitects`: spawns a fresh `claude --print` per turn; session continuity via
/// `--resume` with disk persistence.
///
/// `Codex`: spawns `codex exec` (Turn 1) or `codex exec resume` (Turn 2+); session
/// continuity via `thread_id` with disk persistence.
///
/// `LightarchitectsNative`: writes to a persistent subprocess stdin and reads
/// until the EOT marker.  Spawns lazily on first call or after a crash.
///
/// The mutex serializes turns — correct for a sequential chat UI.
///
/// # Errors
///
/// Returns a descriptive string on spawn failure, process death, or missing result.
/// Public entry point for dispatch — routes a prompt through the copilot
/// subprocess. Same as the internal `call_subprocess` used by `copilot_chat_handler`.
pub async fn call_subprocess_public(
    message: &str,
    proc_lock: &tokio::sync::Mutex<Option<CopilotProcess>>,
    session: &BuildSession,
) -> Result<String, String> {
    call_subprocess(message, proc_lock, session).await
}

async fn call_subprocess(
    message: &str,
    proc_lock: &tokio::sync::Mutex<Option<CopilotProcess>>,
    session: &BuildSession,
) -> Result<String, String> {
    let mut guard = proc_lock.lock().await;

    // Per-turn path for Lightarchitects (claude --print + disk-persistent sessions).
    if matches!(&session.agent, AgentSession::Lightarchitects(_)) {
        let prev_session_id = guard
            .as_ref()
            .and_then(|p| p.session_id.as_deref())
            .map(ToOwned::to_owned);

        let (text, new_session_id) =
            run_print_turn(message, session, prev_session_id.as_deref()).await?;

        if let Some(ref mut proc) = *guard {
            proc.session_id = new_session_id;
        } else {
            *guard = Some(CopilotProcess {
                session_id: new_session_id,
                stdin: None,
                stdout: None,
                _child: None,
            });
        }

        return Ok(text);
    }

    // Per-turn path for Codex (codex exec + disk-persistent sessions).
    if matches!(&session.agent, AgentSession::Codex(_)) {
        let prev_session_id = guard
            .as_ref()
            .and_then(|p| p.session_id.as_deref())
            .map(ToOwned::to_owned);

        let (text, new_session_id) =
            run_codex_turn(message, session, prev_session_id.as_deref()).await?;

        if let Some(ref mut proc) = *guard {
            proc.session_id = new_session_id;
        } else {
            *guard = Some(CopilotProcess {
                session_id: new_session_id,
                stdin: None,
                stdout: None,
                _child: None,
            });
        }

        return Ok(text);
    }

    // Persistent subprocess path — LightarchitectsNative only.
    if guard.is_none() {
        *guard = Some(spawn_copilot(session)?);
    }

    let proc = guard
        .as_mut()
        .ok_or_else(|| "copilot process unavailable".to_owned())?;

    let msg_bytes = [message.as_bytes(), b"\n"].concat();
    {
        let stdin = proc
            .stdin
            .as_mut()
            .ok_or_else(|| "no stdin for persistent subprocess".to_owned())?;
        stdin
            .write_all(&msg_bytes)
            .await
            .map_err(|e| format!("stdin write: {e}"))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("stdin flush: {e}"))?;
    }

    let result_text: Option<String> = loop {
        // Borrow proc.stdout only within this inner block to allow accessing
        // proc.session_id (a different field) in the match arms below.
        let next_line = if let Some(stdout) = proc.stdout.as_mut() {
            stdout.next_line().await
        } else {
            *guard = None;
            return Err("no stdout for persistent subprocess".to_owned());
        };
        match next_line {
            Ok(Some(line)) if !line.is_empty() => {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                    if let Some(id) = val["session_id"].as_str() {
                        proc.session_id = Some(id.to_owned());
                    }
                }
                if let Some(text) = parse_turn_end(&line, session) {
                    break Some(text);
                }
            }
            Ok(None) => {
                *guard = None;
                return Err("agent process exited unexpectedly".to_owned());
            }
            Ok(Some(_)) => {}
            Err(e) => {
                *guard = None;
                return Err(format!("stdout read: {e}"));
            }
        }
    };

    result_text.ok_or_else(|| "no result in agent stream output".to_owned())
}

/// POST to Ollama-compatible `/v1/chat/completions` endpoint.
///
/// # Errors
///
/// Returns a descriptive string on network failure or unexpected response shape.
async fn call_ollama(
    base_url: &str,
    model: &str,
    auth_token: &str,
    message: &str,
) -> Result<String, String> {
    let mut builder = reqwest::Client::new()
        .post(format!("{base_url}/v1/chat/completions"))
        .json(&json!({
            "model": model,
            "messages": [{ "role": "user", "content": message }],
        }));
    if auth_token != "ollama" {
        builder = builder.header("authorization", format!("Bearer {auth_token}"));
    }
    let res = builder.send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        let code = res.status().as_u16();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Ollama {code}: {body}"));
    }
    let val: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    val["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "unexpected Ollama response shape".to_owned())
}
