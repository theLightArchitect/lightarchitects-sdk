//! Sibling subprocess spawner and MCP proxy.
//!
//! Each call to [`call_agent`](crate::spawner::call_agent) spawns a fresh child process for the target route,
//! performs the MCP `initialize` handshake, sends a `tools/call` request, and returns
//! the result. The child process is killed when it drops out of scope.
//!
//! # Design: per-call spawn
//!
//! Spawning fresh per-call avoids shared state, process-pool management, and the
//! race conditions QUANTUM B2 noted for concurrent callers. Each call is independent:
//! spawn → init → call → kill. Callers in `orchestrate` serialise naturally.
//!
//! # MCP framing
//!
//! All routes (CORSO, EVA, SOUL, QUANTUM, SERAPH, AYIN) use newline-delimited
//! JSON-RPC 2.0 over stdio — one JSON object per line, no Content-Length framing.
//! SERAPH uses Content-Length framing in production, but its Mac bridge (the MCP
//! binary Claude Code uses) speaks newline-framed stdio. This spawner uses newline
//! framing unconditionally.
//!
//! # Error surface
//!
//! | Error | Meaning |
//! |---|---|
//! | [`crate::error::GatewayError::SpawnFailed`] | Binary not found or OS spawn error |
//! | [`crate::error::GatewayError::McpProtocol`] | Timeout, malformed JSON, or unexpected response |
//! | [`crate::error::GatewayError::AgentNotEnabled`] | Sibling disabled in config |

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;
use tracing::{debug, info, instrument, warn};

use lightarchitects::ayin::span::{Actor, TraceContext, TraceOutcome};

use crate::config::GatewayConfig;
use crate::error::GatewayError;
use crate::governance;

/// Per-process automation token — generated once at startup.
static AUTOMATION_TOKEN: OnceLock<String> = OnceLock::new();

/// Maximum time to wait for a single MCP response from a route.
const MCP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Minimum JSON-RPC id to use for the `tools/call` request.
const CALL_ID: u64 = 2;

/// AYIN span name for gateway routing decisions.
const SPAN_GATEWAY_ROUTE: &str = "gateway.route";

/// Spawn a route binary, execute one MCP `tools/call`, and return the result.
///
/// The route is identified by `agent_name`. The `action` field is forwarded
/// inside the agent's tool arguments alongside any extra `params`.
///
/// # Governance
///
/// Trust and scope checks are run **before** the subprocess is spawned. If the
/// governance layer rejects the call, no child process is created.
///
/// # Errors
///
/// - [`GatewayError::AgentNotEnabled`] — route not enabled in config.
/// - [`GatewayError::Governance`] — trust or scope check failed.
/// - [`GatewayError::SpawnFailed`] — binary not found or OS spawn error.
/// - [`GatewayError::McpProtocol`] — handshake or response parsing failure.
#[instrument(skip(config, params), fields(route = agent_name, action, preset = %crate::core_tools::preset::active_preset_name()))]
pub async fn call_agent(
    agent_name: &str,
    action: &str,
    params: Value,
    config: &GatewayConfig,
) -> Result<Value, GatewayError> {
    let call_start = Instant::now();

    // 1. Lookup agent config — reject early if not present or disabled.
    let agent_cfg = config
        .agents
        .get(agent_name)
        .ok_or_else(|| GatewayError::AgentNotEnabled(agent_name.to_owned()))?;

    if !agent_cfg.enabled {
        return Err(GatewayError::AgentNotEnabled(agent_name.to_owned()));
    }

    // 2. Governance: trust + scope enforcement before any subprocess is created.
    governance::enforce(
        agent_name,
        agent_cfg.trust,
        agent_cfg.scope,
        action,
        &params,
    )?;

    // 3. Resolve binary path and verify existence (QUANTUM B1 recommendation).
    let binary_path = agent_cfg.binary_path();
    if !binary_path.is_file() {
        warn!(
            route = agent_name,
            path = %binary_path.display(),
            "route binary not found"
        );
        return Err(GatewayError::SpawnFailed {
            agent: agent_name.to_owned(),
            reason: format!("binary not found. Build and deploy {agent_name} first."),
        });
    }

    // 3b. Binary integrity verification — if checksum is configured, verify before spawn.
    if let Some(expected) = &agent_cfg.checksum {
        let actual = sha256_file(&binary_path, agent_name)?;
        if actual != *expected {
            return Err(GatewayError::SpawnFailed {
                agent: agent_name.to_owned(),
                reason: format!("binary checksum mismatch: expected {expected}, got {actual}"),
            });
        }
        debug!(route = agent_name, "binary checksum verified");
    }

    // 4. Spawn the route process.
    let mut child = spawn_agent(&binary_path, agent_name, &config.api_keys)?;

    // 5. Take stdin/stdout handles before executing — these are moved into helpers.
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| GatewayError::McpProtocol {
            agent: agent_name.to_owned(),
            reason: "failed to open stdin pipe".to_owned(),
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| GatewayError::McpProtocol {
            agent: agent_name.to_owned(),
            reason: "failed to open stdout pipe".to_owned(),
        })?;

    let mut writer = tokio::io::BufWriter::new(stdin);
    let mut reader = BufReader::new(stdout);

    // 6. MCP initialize handshake.
    mcp_initialize(&mut writer, &mut reader, agent_name).await?;

    // 7. Build the tools/call arguments.
    //
    // Most siblings expect {"action": "...", "params": {...}} (nested params).
    // QUANTUM is an exception: its MCP adapter reads keys like "query" and
    // "template" at the top-level of the tool args (and optionally a "params"
    // key for execute-style operations).
    let mut arguments = serde_json::Map::new();
    arguments.insert("action".to_owned(), Value::String(action.to_owned()));

    if agent_name == "quantum" {
        // Flatten forwarded params into top-level args (shallow merge).
        if let Value::Object(map) = params {
            for (k, v) in map {
                arguments.insert(k, v);
            }
        } else {
            // Defensive: should always be an object; preserve it if not.
            arguments.insert("params".to_owned(), params);
        }
    } else {
        arguments.insert("params".to_owned(), params);
    }

    let tool_name = agent_cfg.tool_name.clone();

    // 8. Send tools/call.
    let call_req = json!({
        "jsonrpc": "2.0",
        "id": CALL_ID,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments,
        }
    });

    debug!(route = agent_name, tool = %tool_name, "sending tools/call");
    write_line(&mut writer, &call_req, agent_name).await?;

    // 9. Read the tools/call response.
    let response = read_response(&mut reader, CALL_ID, agent_name).await?;

    // 10. Extract result or propagate error.
    let result = extract_result(response, agent_name)?;

    // 11. Emit timing — tracing log + AYIN routing decision span (SB-6).
    let elapsed_ms = u64::try_from(call_start.elapsed().as_millis()).unwrap_or(u64::MAX);
    info!(
        route = agent_name,
        action,
        preset = %crate::core_tools::preset::active_preset_name(),
        elapsed_ms,
        "agent call completed"
    );
    emit_routing_span(agent_name, action, elapsed_ms, TraceOutcome::Continue);

    // Child drops here — the OS SIGKILL handles cleanup.
    Ok(result)
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Spawn the route binary with stdin/stdout pipes.
///
/// Sets `LIGHTARCHITECTS_AUTOMATED` to a random nonce (32-byte hex) generated
/// at gateway startup. Siblings verify the token is a 64-char hex string
/// rather than a simple `"1"` or `"true"`, preventing trivial HITL bypass
/// from malicious processes that guess the env var name.
fn spawn_agent(
    binary_path: &std::path::Path,
    agent_name: &str,
    api_keys: &HashMap<String, String>,
) -> Result<Child, GatewayError> {
    let token = automation_token();
    let mut cmd = Command::new(binary_path);
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .env("LIGHTARCHITECTS_AUTOMATED", &token)
        // §N.1 / SG-3: siblings must not inherit the gateway's HMAC pepper.
        .env_remove("ARENA_PEPPER");

    // Some siblings use clap env var bindings for `--no-color` and expect a
    // boolean string. `NO_COLOR=1` is common, but may not parse as bool.
    if let Ok(v) = std::env::var("NO_COLOR") {
        if v == "1" {
            cmd.env("NO_COLOR", "true");
        }
    }
    // Inject API keys from keys.toml — only when not already present in the
    // process environment (env vars from .mcp.json always take priority).
    for (k, v) in api_keys {
        if std::env::var(k).is_err() {
            cmd.env(k, v);
        }
    }

    // Some sibling binaries are multi-command CLIs; the MCP server lives behind a
    // subcommand.
    if agent_name == "quantum" {
        cmd.arg("mcp-server");
    }
    cmd.spawn().map_err(|e| GatewayError::SpawnFailed {
        agent: agent_name.to_owned(),
        reason: crate::core_tools::security::sanitize_error(&e.to_string()),
    })
}

/// Initialise the automation token (call once from `main`).
///
/// Generates a 64-char hex nonce using system time and PID as entropy.
/// Subsequent calls are no-ops — the token is immutable once set.
pub fn init_automation_token() {
    AUTOMATION_TOKEN.get_or_init(generate_automation_token);
}

/// Return the gateway's automation token.
///
/// Falls back to a freshly generated token if [`init_automation_token`] was
/// never called (should not happen in normal operation).
fn automation_token() -> String {
    AUTOMATION_TOKEN
        .get()
        .cloned()
        .unwrap_or_else(generate_automation_token)
}

/// Generate a 64-char hex automation token using CSPRNG entropy.
///
/// Uses `lightarchitects::crypto::random::generate_hex` which sources 32 bytes
/// from the OS CSPRNG (`rand::thread_rng` backed by `getrandom`). This is
/// safe for HITL gate tokens — an attacker on the same host cannot predict
/// or reconstruct the value.
#[must_use]
pub fn generate_automation_token() -> String {
    lightarchitects::crypto::random::generate_hex(32)
}

/// Send the MCP `initialize` request and read + discard the response.
async fn mcp_initialize(
    writer: &mut tokio::io::BufWriter<tokio::process::ChildStdin>,
    reader: &mut BufReader<tokio::process::ChildStdout>,
    agent_name: &str,
) -> Result<(), GatewayError> {
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "lightarchitects",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    });

    write_line(writer, &init_req, agent_name).await?;

    // Read the initialize response — we don't validate capabilities here.
    let _init_response = read_response(reader, 1, agent_name).await?;

    // Send the initialized notification (required by MCP spec before tools/call).
    let initialized = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    write_line(writer, &initialized, agent_name).await?;

    Ok(())
}

/// Write a JSON value as a newline-terminated line to the writer.
async fn write_line(
    writer: &mut tokio::io::BufWriter<tokio::process::ChildStdin>,
    value: &Value,
    agent_name: &str,
) -> Result<(), GatewayError> {
    let mut line = serde_json::to_string(value).map_err(GatewayError::Json)?;
    line.push('\n');

    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| GatewayError::McpProtocol {
            agent: agent_name.to_owned(),
            reason: format!("write error: {e}"),
        })?;

    writer
        .flush()
        .await
        .map_err(|e| GatewayError::McpProtocol {
            agent: agent_name.to_owned(),
            reason: format!("flush error: {e}"),
        })?;

    Ok(())
}

/// Read one JSON-RPC response from the reader, matching the given `expected_id`.
///
/// Skips lines that are not parseable JSON or that do not have an `id` field
/// (notifications). Times out after [`MCP_RESPONSE_TIMEOUT`].
async fn read_response(
    reader: &mut BufReader<tokio::process::ChildStdout>,
    expected_id: u64,
    agent_name: &str,
) -> Result<Value, GatewayError> {
    let mut line = String::new();

    let result = timeout(MCP_RESPONSE_TIMEOUT, async {
        loop {
            line.clear();
            let n = reader
                .read_line(&mut line)
                .await
                .map_err(|e| GatewayError::McpProtocol {
                    agent: agent_name.to_owned(),
                    reason: format!("read error: {e}"),
                })?;

            if n == 0 {
                return Err(GatewayError::McpProtocol {
                    agent: agent_name.to_owned(),
                    reason: "route closed stdout unexpectedly".to_owned(),
                });
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let value: Value = if let Ok(v) = serde_json::from_str(trimmed) {
                v
            } else {
                debug!(route = agent_name, "skipping non-JSON line from route");
                continue;
            };

            // Skip notifications (no `id` field).
            match value.get("id") {
                Some(Value::Number(n)) if n.as_u64() == Some(expected_id) => {
                    return Ok(value);
                }
                Some(Value::Number(_)) => {
                    // A response for a different id — not expected but skip it.
                    debug!(route = agent_name, "skipping response for unexpected id");
                }
                _ => {
                    // Notification — skip.
                }
            }
        }
    })
    .await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(GatewayError::McpProtocol {
            agent: agent_name.to_owned(),
            reason: format!(
                "timed out waiting for response ({}s)",
                MCP_RESPONSE_TIMEOUT.as_secs()
            ),
        }),
    }
}

/// Extract the `result` field from a successful JSON-RPC response, or convert
/// a JSON-RPC error response into a [`GatewayError::McpProtocol`].
fn extract_result(response: Value, agent_name: &str) -> Result<Value, GatewayError> {
    if let Some(error) = response.get("error") {
        return Err(GatewayError::McpProtocol {
            agent: agent_name.to_owned(),
            reason: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown MCP error")
                .to_owned(),
        });
    }

    response
        .get("result")
        .cloned()
        .ok_or_else(|| GatewayError::McpProtocol {
            agent: agent_name.to_owned(),
            reason: "response missing 'result' field".to_owned(),
        })
}

/// Compute the SHA-256 digest of a file using the system `shasum` command.
///
/// Returns the lowercase hex digest string.
///
/// # Errors
///
/// Returns [`GatewayError::SpawnFailed`] if the file cannot be read or
/// `shasum` cannot be executed.
fn sha256_file(path: &std::path::Path, agent_name: &str) -> Result<String, GatewayError> {
    let output = std::process::Command::new("shasum")
        .args(["-a", "256", &path.to_string_lossy()])
        .output()
        .map_err(|e| GatewayError::SpawnFailed {
            agent: agent_name.to_owned(),
            reason: format!("shasum failed: {e}"),
        })?;

    if !output.status.success() {
        return Err(GatewayError::SpawnFailed {
            agent: agent_name.to_owned(),
            reason: "shasum returned non-zero exit code".to_owned(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split_whitespace()
        .next()
        .map(String::from)
        .ok_or_else(|| GatewayError::SpawnFailed {
            agent: agent_name.to_owned(),
            reason: "shasum produced no output".to_owned(),
        })
}

// ── AYIN routing span helpers ──────────────────────────────────────────────────

/// Emit a `gateway.route` AYIN span for a completed sibling tool call.
///
/// Fire-and-forget via `tokio::spawn` — never blocks the caller.
/// Only emits when a tokio runtime is active (i.e., during normal gateway
/// operation; skipped in sync test contexts that don't call this path).
fn emit_routing_span(agent_name: &str, action: &str, latency_ms: u64, outcome: TraceOutcome) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let agent_name = agent_name.to_owned();
    let action = action.to_owned();
    handle.spawn(async move {
        let ctx = TraceContext::new(Actor::new("gateway"), SPAN_GATEWAY_ROUTE).decision(
            "route_selected",
            &action,
            &agent_name,
            Some(1.0),
            latency_ms,
        );
        let ctx = match ctx {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "AYIN route decision point failed");
                return;
            }
        };
        let metadata = serde_json::json!({
            "lasdlc.route": &agent_name,
            "lasdlc.latency_ms": latency_ms,
            "gateway.action": &action,
        });
        let ctx = ctx.metadata(metadata).outcome(outcome);
        write_routing_span(ctx).await;
    });
}

/// Write a completed routing span to the AYIN traces directory.
async fn write_routing_span(ctx: TraceContext) {
    let span = match ctx.finish() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "AYIN route span build failed");
            return;
        }
    };
    let base = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("lightarchitects/soul/helix/ayin/traces");
    let dir = base
        .join(span.actor.as_str())
        .join(span.timestamp.format("%Y-%m-%d").to_string());
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        tracing::warn!(error = %e, "AYIN route dir create failed");
        return;
    }
    let safe_action = span.action.replace('/', "_");
    let id_str = span.id.to_string();
    let filename = format!(
        "{}-{}-{}.json",
        span.timestamp.format("%H-%M-%S"),
        safe_action,
        &id_str[..8]
    );
    match serde_json::to_vec(&span) {
        Ok(bytes) => {
            if let Err(e) = tokio::fs::write(dir.join(&filename), bytes).await {
                tracing::warn!(error = %e, "AYIN route span write failed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "AYIN route span serialize failed"),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::config::GatewayConfig;

    #[tokio::test]
    async fn call_agent_fails_for_unknown_route() {
        let cfg = GatewayConfig::default();
        let err = call_agent("nonexistent", "query", json!({}), &cfg)
            .await
            .unwrap_err();
        assert!(
            matches!(err, GatewayError::AgentNotEnabled(_)),
            "expected AgentNotEnabled, got {err:?}"
        );
    }

    #[tokio::test]
    async fn call_agent_fails_for_disabled_agent() {
        let cfg = GatewayConfig::default();
        // QUANTUM is disabled in default config.
        let err = call_agent("quantum", "scan", json!({}), &cfg)
            .await
            .unwrap_err();
        assert!(
            matches!(err, GatewayError::AgentNotEnabled(_)),
            "expected AgentNotEnabled for disabled agent, got {err:?}"
        );
    }

    #[tokio::test]
    async fn call_agent_fails_gracefully_when_binary_missing() {
        // Override the binary path to a path that is guaranteed not to exist,
        // making the test deterministic regardless of local deployment state.
        let mut cfg = GatewayConfig::default();
        if let Some(c) = cfg.agents.get_mut("corso") {
            c.binary = "/nonexistent/path/corso-binary-absent".to_owned();
        }
        let err = call_agent("corso", "guard", json!({}), &cfg)
            .await
            .unwrap_err();
        // Either SpawnFailed (binary missing) or Governance — both are acceptable.
        assert!(
            matches!(
                err,
                GatewayError::SpawnFailed { .. } | GatewayError::Governance { .. }
            ),
            "unexpected error variant: {err:?}"
        );
    }

    #[test]
    fn extract_result_returns_ok_for_success_response() {
        let response =
            json!({"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"ok"}]}});
        let result = extract_result(response, "soul").unwrap();
        assert_eq!(result["content"][0]["text"], "ok");
    }

    #[test]
    fn extract_result_returns_err_for_error_response() {
        let response =
            json!({"jsonrpc":"2.0","id":2,"error":{"code":-32603,"message":"Tool failed"}});
        let err = extract_result(response, "soul").unwrap_err();
        assert!(matches!(err, GatewayError::McpProtocol { .. }));
    }
}
