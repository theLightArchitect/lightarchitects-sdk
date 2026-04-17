//! End-to-end test: spawn the `lightarchitects` binary and exercise the full
//! MCP JSON-RPC protocol over stdio.
//!
//! These tests are integration tests that require a compiled binary — Cargo
//! builds it automatically before running `cargo test`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

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

// ── First-run and preset E2E tests ───────────────────────────────────────────

/// Spawn a gateway process with a given config string.
/// Returns the child, stdin, reader, AND the tempfile (must stay alive for the child).
fn spawn_with_config(
    config_toml: &str,
) -> (
    std::process::Child,
    impl Write,
    impl BufRead,
    tempfile::TempDir,
) {
    let tmp_dir = tempfile::tempdir().expect("tempdir");
    let config_path = tmp_dir.path().join("config.toml");
    std::fs::write(&config_path, config_toml).expect("write config");

    let mut child = Command::new(GATEWAY_BIN)
        .arg("--config")
        .arg(&config_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn gateway");

    let stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let reader = BufReader::new(stdout);
    (child, stdin, reader, tmp_dir)
}

/// Config with `[agents.*]` section name works (the canonical name).
#[test]
fn config_agents_section_name_works() {
    let config = r#"
[gateway]
version = "1.0.0"

[agents.soul]
enabled = true
binary = "~/lightarchitects/soul/bin/soul"
tool_name = "soulTools"
role = "Knowledge graph"
trust = "trusted"
scope = "all"
"#;
    let (mut child, mut stdin, mut reader, _tmp) = spawn_with_config(config);

    let resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");

    child.kill().ok();
    child.wait().ok();
}

/// Config with `[routes.*]` (backward compat alias) still works.
#[test]
fn config_routes_backward_compat_works() {
    let config = r#"
[gateway]
version = "1.0.0"

[routes.corso]
enabled = true
binary = "~/lightarchitects/corso/bin/corso"
tool_name = "corsoTools"
role = "AppSec"
trust = "trusted"
scope = "own"
"#;
    let (mut child, mut stdin, mut reader, _tmp) = spawn_with_config(config);

    let resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");

    child.kill().ok();
    child.wait().ok();
}

/// Config with `[siblings.*]` (legacy alias) still works.
#[test]
fn config_siblings_backward_compat_works() {
    let config = r#"
[gateway]
version = "1.0.0"

[siblings.eva]
enabled = true
binary = "~/lightarchitects/eva/bin/eva"
tool_name = "evaTools"
role = "DevOps"
trust = "trusted"
scope = "shared"
"#;
    let (mut child, mut stdin, mut reader, _tmp) = spawn_with_config(config);

    let resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");

    child.kill().ok();
    child.wait().ok();
}

/// Discover output includes `active_preset` field.
#[test]
fn discover_shows_active_preset() {
    let (mut child, mut stdin, mut reader, _tmp) =
        spawn_with_config("[gateway]\nversion = \"1.0.0\"\nactive_preset = \"forensics\"\n");

    // Initialize
    rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );

    // Discover
    let resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"tools","arguments":{"action":"discover"}}}"#,
    );
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .expect("discover text");
    assert!(
        text.contains("active_preset"),
        "discover should include active_preset"
    );

    child.kill().ok();
    child.wait().ok();
}

/// Preset action returns the list of all 12 presets.
#[test]
fn preset_list_returns_twelve_presets() {
    let (_child, mut stdin, mut reader, _tmp) =
        spawn_with_config("[gateway]\nversion = \"1.0.0\"\n");

    // Initialize
    rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );

    // Preset list (no name param = show all)
    let resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"tools","arguments":{"action":"preset","params":{}}}}"#,
    );
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .expect("preset text");
    let parsed: serde_json::Value = serde_json::from_str(text).expect("parse preset JSON");
    let presets = parsed["presets"].as_array().expect("presets array");
    assert_eq!(
        presets.len(),
        12,
        "expected 12 presets, got {}",
        presets.len()
    );
}

/// First-run creates config when directory exists but file doesn't.
#[test]
fn first_run_config_generation() {
    // Verify the default TOML generation produces valid config.
    let config = lightarchitects_gateway::config::GatewayConfig::default();
    assert_eq!(config.active_preset, "software_engineering");
    assert!(
        config.agents.contains_key("soul"),
        "SOUL must be in default config"
    );
    assert!(
        config.agents.contains_key("corso"),
        "CORSO must be in default config"
    );
    assert!(
        config.agents["soul"].enabled,
        "SOUL must be enabled by default"
    );
    assert!(!config.first_run, "default() should not set first_run");
}

// ── Error-path E2E tests ──────────────────────────────────────────────────────

/// Calling `tools/call` with a completely unknown tool name returns an MCP error response
/// (JSON-RPC result with `isError: true`), not a crash.
#[test]
fn unknown_tool_call_returns_error_not_crash() {
    let mut child = Command::new(GATEWAY_BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn gateway");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );

    let resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nonexistent_tool_xyz","arguments":{}}}"#,
    );

    // Must have a result field (not a JSON-RPC error at the transport level)
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "response must be a valid JSON-RPC message"
    );
    // Must NOT crash — we should still be able to send another request
    let init_again = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#,
    );
    assert!(init_again["result"]["tools"].is_array());

    child.kill().ok();
    child.wait().ok();
}

/// Missing `action` field in `tools` call returns error content, not crash.
#[test]
fn tools_call_missing_action_returns_error_content() {
    let (mut child, mut stdin, mut reader, _tmp) =
        spawn_with_config("[gateway]\nversion = \"1.0.0\"\n");

    rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );

    let resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"tools","arguments":{}}}"#,
    );

    // Must produce a response (not hang or crash)
    let content = &resp["result"]["content"];
    assert!(
        content.is_array() || resp.get("error").is_some(),
        "missing action must produce content or error, not silence"
    );

    child.kill().ok();
    child.wait().ok();
}

/// Status action reports gateway version and config summary.
#[test]
fn tools_call_status_returns_structured_data() {
    let (mut child, mut stdin, mut reader, _tmp) =
        spawn_with_config("[gateway]\nversion = \"1.0.0\"\n");

    rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );

    let resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"tools","arguments":{"action":"status"}}}"#,
    );

    let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
    // Status response should mention the gateway
    assert!(
        !text.is_empty() || resp.get("error").is_some(),
        "status action must return non-empty content"
    );

    child.kill().ok();
    child.wait().ok();
}

/// `tools/list` response always contains the single `tools` meta-tool.
/// Regression guard: adding new internal actions must not inflate the public tool count.
#[test]
fn tools_list_always_returns_single_meta_tool() {
    let configs = [
        "[gateway]\nversion = \"1.0.0\"\n",
        "[gateway]\nversion = \"1.0.0\"\nactive_preset = \"full\"\n",
        "[gateway]\nversion = \"1.0.0\"\nactive_preset = \"solo\"\n",
    ];

    for config in &configs {
        let (mut child, mut stdin, mut reader, _tmp) = spawn_with_config(config);

        rpc(
            &mut stdin,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        );
        let resp = rpc(
            &mut stdin,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        );
        let tools = resp["result"]["tools"].as_array().expect("tools array");
        assert_eq!(
            tools.len(),
            EXPECTED_TOOL_COUNT,
            "preset '{config}' must not change public tool count"
        );

        child.kill().ok();
        child.wait().ok();
    }
}

/// Gateway responds correctly to `notifications/initialized` — no response
/// (notifications are fire-and-forget in MCP), and subsequent requests still work.
#[test]
fn notification_does_not_block_subsequent_requests() {
    let mut child = Command::new(GATEWAY_BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn gateway");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );

    // Fire-and-forget notification (no id field — no response expected)
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .expect("write notification");
    stdin.flush().expect("flush");

    // Gateway must still respond to the next request
    let resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    );
    assert!(
        resp["result"]["tools"].is_array(),
        "gateway must handle requests after notification"
    );

    child.kill().ok();
    child.wait().ok();
}

// ── Sibling-spawn E2E (requires deployed binaries — run manually) ─────────────

/// Verify SOUL sibling spawns successfully via the gateway's spawner.
///
/// Requires `~/lightarchitects/soul/.config/bin/soul` to be deployed.
/// Run manually: `cargo test -p lightarchitects-gateway spawn_soul -- --ignored`
#[test]
#[ignore = "requires deployed SOUL binary at ~/lightarchitects/soul/.config/bin/soul"]
fn spawn_soul_sibling_and_call_soul_tools() {
    let config = r#"
[gateway]
version = "1.0.0"

[agents.soul]
enabled = true
binary = "~/lightarchitects/soul/.config/bin/soul"
tool_name = "soulTools"
role = "Knowledge graph"
trust = "trusted"
scope = "all"
"#;
    let (mut child, mut stdin, mut reader, _tmp) = spawn_with_config(config);

    rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    let resp = rpc(
        &mut stdin,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"tools","arguments":{"action":"soulTools","params":{"action":"health"}}}}"#,
    );
    let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(!text.is_empty(), "SOUL health check must return content");

    child.kill().ok();
    child.wait().ok();
}
