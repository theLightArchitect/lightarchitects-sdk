//! End-to-end test: spawn the `lightarchitects` binary and exercise the full
//! MCP JSON-RPC protocol over stdio.
//!
//! These tests are integration tests that require a compiled binary — Cargo
//! builds it automatically before running `cargo test`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Path to the compiled `lightarchitects` binary, resolved by Cargo at compile time.
const GATEWAY_BIN: &str = env!("CARGO_BIN_EXE_lightarchitects");

/// Expected number of tools exposed by the gateway via `tools/list`.
/// Only the unified `tools` meta-tool is advertised.
const EXPECTED_TOOL_COUNT: usize = 1;

// ── Helper ────────────────────────────────────────────────────────────────────

/// Write one JSON-RPC request line to `writer` and read the response from `reader`.
fn rpc(writer: &mut impl Write, reader: &mut impl BufRead, request: &str) -> serde_json::Value {
    writeln!(writer, "{request}").expect("write request");
    writer.flush().expect("flush");
    let mut line = String::new();
    reader.read_line(&mut line).expect("read response");
    serde_json::from_str(line.trim()).expect("parse JSON-RPC response")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Full MCP handshake → tools/list → tools/call round-trip.
///
/// Verifies:
/// 1. `initialize` returns the MCP protocol version.
/// 2. `tools/list` returns exactly [`EXPECTED_TOOL_COUNT`] tools, all prefixed `lightarchitects_`.
/// 3. `tools/call lightarchitects_discover` succeeds and returns text content.
#[test]
fn mcp_full_protocol_round_trip() {
    let mut child = Command::new(GATEWAY_BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null()) // suppress INFO logs on stderr
        .spawn()
        .expect("spawn gateway binary");

    let mut stdin = child.stdin.take().expect("take stdin");
    let stdout = child.stdout.take().expect("take stdout");
    let mut reader = BufReader::new(stdout);

    // ── Step 1: MCP initialize handshake ──────────────────────────────────────
    let init_resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    assert_eq!(
        init_resp["result"]["protocolVersion"], "2024-11-05",
        "unexpected MCP protocol version"
    );
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "lightarchitects");

    // ── Step 2: tools/list (single meta-tool only) ─────────────────────────
    let list_resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    );
    let tools = list_resp["result"]["tools"]
        .as_array()
        .expect("tools must be an array");
    assert_eq!(
        tools.len(),
        EXPECTED_TOOL_COUNT,
        "expected {EXPECTED_TOOL_COUNT} tools, got {}",
        tools.len()
    );
    assert_eq!(
        tools[0]["name"], "tools",
        "the single advertised tool must be 'tools'"
    );

    // ── Step 3: tools/call lightarchitects_discover ───────────────────────────
    let call_resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"lightarchitects_discover","arguments":{}}}"#,
    );
    let text = call_resp["result"]["content"][0]["text"]
        .as_str()
        .expect("discover must return text content");
    // Response is JSON; verify it contains gateway-specific keys.
    assert!(
        text.contains("gateway_version"),
        "discover response should contain 'gateway_version' field"
    );
    assert!(
        text.contains("core_tools"),
        "discover response should contain 'core_tools' field"
    );

    // ── Cleanup ───────────────────────────────────────────────────────────────
    child.kill().ok();
    child.wait().ok();
}

/// `--config` flag: passing a valid config path puts the binary into MCP server mode.
///
/// Verifies that `--config <path>` is stripped before subcommand dispatch and that
/// MCP server mode still responds correctly when the flag is present.
#[test]
fn config_flag_does_not_break_mcp_mode() {
    // Write a minimal config to a temp file.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    writeln!(tmp.as_file(), "[gateway]\nversion = \"1.0.0\"").expect("write config");

    let mut child = Command::new(GATEWAY_BIN)
        .arg("--config")
        .arg(tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn with --config");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    let resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");

    child.kill().ok();
    child.wait().ok();
}
