//! Sibling subprocess spawner and MCP proxy.
//!
//! Each call to [`call_sibling`](crate::spawner::call_sibling) spawns a fresh child process for the target sibling,
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
//! All siblings (CORSO, EVA, SOUL, QUANTUM, SERAPH, AYIN) use newline-delimited
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
//! | [`crate::error::GatewayError::SiblingNotEnabled`] | Sibling disabled in config |

use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;
use tracing::{debug, instrument, warn};

use crate::config::GatewayConfig;
use crate::error::GatewayError;
use crate::governance;

/// Maximum time to wait for a single MCP response from a sibling.
const MCP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Minimum JSON-RPC id to use for the `tools/call` request.
const CALL_ID: u64 = 2;

/// Spawn a sibling binary, execute one MCP `tools/call`, and return the result.
///
/// The sibling is identified by `sibling_name`. The `action` field is forwarded
/// inside the sibling's tool arguments alongside any extra `params`.
///
/// # Governance
///
/// Trust and scope checks are run **before** the subprocess is spawned. If the
/// governance layer rejects the call, no child process is created.
///
/// # Errors
///
/// - [`GatewayError::SiblingNotEnabled`] — sibling not enabled in config.
/// - [`GatewayError::Governance`] — trust or scope check failed.
/// - [`GatewayError::SpawnFailed`] — binary not found or OS spawn error.
/// - [`GatewayError::McpProtocol`] — handshake or response parsing failure.
#[instrument(skip(config, params), fields(sibling = sibling_name, action))]
pub async fn call_sibling(
    sibling_name: &str,
    action: &str,
    params: Value,
    config: &GatewayConfig,
) -> Result<Value, GatewayError> {
    // 1. Lookup sibling config — reject early if not present or disabled.
    let sibling_cfg = config
        .siblings
        .get(sibling_name)
        .ok_or_else(|| GatewayError::SiblingNotEnabled(sibling_name.to_owned()))?;

    if !sibling_cfg.enabled {
        return Err(GatewayError::SiblingNotEnabled(sibling_name.to_owned()));
    }

    // 2. Governance: trust + scope enforcement before any subprocess is created.
    governance::enforce(
        sibling_name,
        sibling_cfg.trust,
        sibling_cfg.scope,
        action,
        &params,
    )?;

    // 3. Resolve binary path and verify existence (QUANTUM B1 recommendation).
    let binary_path = sibling_cfg.binary_path();
    if !binary_path.is_file() {
        warn!(
            sibling = sibling_name,
            path = %binary_path.display(),
            "sibling binary not found"
        );
        return Err(GatewayError::SpawnFailed {
            sibling: sibling_name.to_owned(),
            reason: format!(
                "binary not found at '{}'. Build and deploy {sibling_name} first.",
                binary_path.display()
            ),
        });
    }

    // 4. Spawn the sibling process.
    let mut child = spawn_sibling(&binary_path, sibling_name)?;

    // 5. Take stdin/stdout handles before executing — these are moved into helpers.
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| GatewayError::McpProtocol {
            sibling: sibling_name.to_owned(),
            reason: "failed to open stdin pipe".to_owned(),
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| GatewayError::McpProtocol {
            sibling: sibling_name.to_owned(),
            reason: "failed to open stdout pipe".to_owned(),
        })?;

    let mut writer = tokio::io::BufWriter::new(stdin);
    let mut reader = BufReader::new(stdout);

    // 6. MCP initialize handshake.
    mcp_initialize(&mut writer, &mut reader, sibling_name).await?;

    // 7. Build the tools/call arguments: {action, params}.
    let mut arguments = serde_json::Map::new();
    arguments.insert("action".to_owned(), Value::String(action.to_owned()));

    // Merge params into arguments (flattened — same level as action).
    if let Value::Object(extra) = params {
        for (k, v) in extra {
            arguments.insert(k, v);
        }
    }

    let tool_name = sibling_cfg.tool_name.clone();

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

    debug!(sibling = sibling_name, tool = %tool_name, "sending tools/call");
    write_line(&mut writer, &call_req, sibling_name).await?;

    // 9. Read the tools/call response.
    let response = read_response(&mut reader, CALL_ID, sibling_name).await?;

    // 10. Extract result or propagate error.
    let result = extract_result(response, sibling_name)?;

    // Child drops here — the OS SIGKILL handles cleanup.
    Ok(result)
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Spawn the sibling binary with stdin/stdout pipes.
///
/// Sets `LIGHTARCHITECTS_AUTOMATED=1` in the child environment so siblings
/// know they are being called from the gateway (not interactively). This
/// signals HITL gates to auto-approve or skip — there is no human at the
/// other end of a subprocess pipe.
fn spawn_sibling(binary_path: &std::path::Path, sibling_name: &str) -> Result<Child, GatewayError> {
    Command::new(binary_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .env("LIGHTARCHITECTS_AUTOMATED", "1")
        .spawn()
        .map_err(|e| GatewayError::SpawnFailed {
            sibling: sibling_name.to_owned(),
            reason: e.to_string(),
        })
}

/// Send the MCP `initialize` request and read + discard the response.
async fn mcp_initialize(
    writer: &mut tokio::io::BufWriter<tokio::process::ChildStdin>,
    reader: &mut BufReader<tokio::process::ChildStdout>,
    sibling_name: &str,
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

    write_line(writer, &init_req, sibling_name).await?;

    // Read the initialize response — we don't validate capabilities here.
    let _init_response = read_response(reader, 1, sibling_name).await?;

    // Send the initialized notification (required by MCP spec before tools/call).
    let initialized = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    write_line(writer, &initialized, sibling_name).await?;

    Ok(())
}

/// Write a JSON value as a newline-terminated line to the writer.
async fn write_line(
    writer: &mut tokio::io::BufWriter<tokio::process::ChildStdin>,
    value: &Value,
    sibling_name: &str,
) -> Result<(), GatewayError> {
    let mut line = serde_json::to_string(value).map_err(GatewayError::Json)?;
    line.push('\n');

    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| GatewayError::McpProtocol {
            sibling: sibling_name.to_owned(),
            reason: format!("write error: {e}"),
        })?;

    writer
        .flush()
        .await
        .map_err(|e| GatewayError::McpProtocol {
            sibling: sibling_name.to_owned(),
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
    sibling_name: &str,
) -> Result<Value, GatewayError> {
    let mut line = String::new();

    let result = timeout(MCP_RESPONSE_TIMEOUT, async {
        loop {
            line.clear();
            let n = reader
                .read_line(&mut line)
                .await
                .map_err(|e| GatewayError::McpProtocol {
                    sibling: sibling_name.to_owned(),
                    reason: format!("read error: {e}"),
                })?;

            if n == 0 {
                return Err(GatewayError::McpProtocol {
                    sibling: sibling_name.to_owned(),
                    reason: "sibling closed stdout unexpectedly".to_owned(),
                });
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let value: Value = if let Ok(v) = serde_json::from_str(trimmed) {
                v
            } else {
                debug!(
                    sibling = sibling_name,
                    "skipping non-JSON line from sibling"
                );
                continue;
            };

            // Skip notifications (no `id` field).
            match value.get("id") {
                Some(Value::Number(n)) if n.as_u64() == Some(expected_id) => {
                    return Ok(value);
                }
                Some(Value::Number(_)) => {
                    // A response for a different id — not expected but skip it.
                    debug!(
                        sibling = sibling_name,
                        "skipping response for unexpected id"
                    );
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
            sibling: sibling_name.to_owned(),
            reason: format!(
                "timed out waiting for response ({}s)",
                MCP_RESPONSE_TIMEOUT.as_secs()
            ),
        }),
    }
}

/// Extract the `result` field from a successful JSON-RPC response, or convert
/// a JSON-RPC error response into a [`GatewayError::McpProtocol`].
fn extract_result(response: Value, sibling_name: &str) -> Result<Value, GatewayError> {
    if let Some(error) = response.get("error") {
        return Err(GatewayError::McpProtocol {
            sibling: sibling_name.to_owned(),
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
            sibling: sibling_name.to_owned(),
            reason: "response missing 'result' field".to_owned(),
        })
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GatewayConfig;

    #[tokio::test]
    async fn call_sibling_fails_for_unknown_sibling() {
        let cfg = GatewayConfig::default();
        let err = call_sibling("nonexistent", "query", json!({}), &cfg)
            .await
            .unwrap_err();
        assert!(
            matches!(err, GatewayError::SiblingNotEnabled(_)),
            "expected SiblingNotEnabled, got {err:?}"
        );
    }

    #[tokio::test]
    async fn call_sibling_fails_for_disabled_sibling() {
        let cfg = GatewayConfig::default();
        // QUANTUM is disabled in default config.
        let err = call_sibling("quantum", "scan", json!({}), &cfg)
            .await
            .unwrap_err();
        assert!(
            matches!(err, GatewayError::SiblingNotEnabled(_)),
            "expected SiblingNotEnabled for disabled sibling, got {err:?}"
        );
    }

    #[tokio::test]
    async fn call_sibling_fails_gracefully_when_binary_missing() {
        // Override the binary path to a path that is guaranteed not to exist,
        // making the test deterministic regardless of local deployment state.
        let mut cfg = GatewayConfig::default();
        if let Some(c) = cfg.siblings.get_mut("corso") {
            c.binary = "/nonexistent/path/corso-binary-absent".to_owned();
        }
        let err = call_sibling("corso", "guard", json!({}), &cfg)
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
