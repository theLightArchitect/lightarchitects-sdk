//! MCP server: stdin/stdout JSON-RPC loop, tool registry, and dispatch.

use serde_json::{Value, json};
use tracing::instrument;

use crate::config::GatewayConfig;
use crate::core_tools;
use crate::error::GatewayError;

// ── Tool schema definitions ───────────────────────────────────────────────────

/// Tool definitions returned by `tools/list` — only the unified `tools` meta-tool.
///
/// Individual `lightarchitects_*` tools are still routable via `tools/call` for
/// backward compatibility (CLI use, direct invocation), but they are not
/// advertised in the MCP tool list.
#[must_use]
pub fn tool_definitions() -> Vec<Value> {
    vec![meta_tool_def()]
}

/// All tool definitions including individual `lightarchitects_*` tools.
///
/// Used internally by the dispatch table and validation tests — NOT exposed
/// via `tools/list`.
#[must_use]
pub fn all_tool_definitions() -> Vec<Value> {
    let mut tools = vec![meta_tool_def()];
    tools.extend(file_tool_defs());
    tools.extend(platform_tool_defs());
    tools.extend(squad_tool_defs());
    tools
}

/// The unified `tools` meta-tool — single entry point following the agent pattern.
fn meta_tool_def() -> Value {
    json!({
        "name": "tools",
        "description": "Light Architects gateway — single entry point for all operations. Use action='list' to discover all 60+ available actions.\n\nCore actions (handled by gateway):\n• read — Read file contents. params: {path, offset?, limit?}\n• write — Create/overwrite file. params: {path, content}\n• edit — String replacement. params: {path, old_string, new_string, replace_all?}\n• bash — Execute shell command. params: {command, timeout_ms?, cwd?}\n• search — Ripgrep file search. params: {pattern, path?, glob?, case_insensitive?}\n• glob — Find files by pattern. params: {pattern, path?}\n• discover — Gateway version, tools, agent status. params: none\n• ask_user — Prompt user for input. params: {question, options?}\n• initialize — Setup wizard. params: {step?}\n• import — Import from external systems. params: {source, path?, format?}\n• canon_check — Extract canon headers and present for review (file-based, not AI reasoning). params: {decision, verbose?}\n• canon_evaluate — Return blank 5-criteria evaluation template (scores are null, not computed). params: {candidate}\n\nRouted actions (auto-routed by SDK action enums, priority: QUANTUM > CORSO > SERAPH > EVA > SOUL > AYIN):\n• CORSO (19): sniff, guard, fetch, chase, scout, code_review, generate_code, search_code, find_symbol, get_outline, get_references, analyze_architecture, prove, optimize, deploy, rollback, manage_logs, strike, watch\n• EVA (9): visualize, ideate, bible_search, bible_reflect, teach, remember, crystallize, celebrate, mindfulness\n• SOUL (14): read_note, write_note, list_notes, manifest, ingest, search, helix, query, query_frontmatter, stats, voice, converse, chat, research\n• QUANTUM (9): triage, sweep, trace, probe, theorize, verify, close, quick, research\n• SERAPH (6): status, investigate_start, investigate_advance, investigate_close, investigate_report, vault_sync\n• AYIN (3): sessions, spans, conversations\n\nCollisions: 'research' routes to QUANTUM (priority). Pass 'agent' to override.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform. Use 'list' to discover all available actions."
                },
                "params": {
                    "type": "object",
                    "description": "Parameters for the selected action."
                },
                "agent": {
                    "type": "string",
                    "description": "Optional: override auto-routing to force a specific target (corso, eva, soul, quantum, seraph, ayin).",
                    "enum": ["corso", "eva", "soul", "quantum", "seraph", "ayin"]
                }
            },
            "required": ["action"]
        }
    })
}

/// File operation tool definitions: read, write, edit, bash, search, glob.
fn file_tool_defs() -> Vec<Value> {
    vec![
        json!({"name": "lightarchitects_read", "description": "Read file contents. Returns text content with line numbers.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string", "description": "Absolute or relative file path. ~/  prefix is expanded."}, "offset": {"type": "integer", "description": "1-indexed first line to return (optional)."}, "limit": {"type": "integer", "description": "Maximum number of lines to return (optional)."}}, "required": ["path"]}}),
        json!({"name": "lightarchitects_write", "description": "Create or overwrite a file. Parent directories are created automatically.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string", "description": "Destination file path. ~/ prefix is expanded."}, "content": {"type": "string", "description": "File content to write."}}, "required": ["path", "content"]}}),
        json!({"name": "lightarchitects_edit", "description": "Perform an exact string replacement in a file.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string", "description": "File to edit. ~/ prefix is expanded."}, "old_string": {"type": "string", "description": "Exact text to find and replace."}, "new_string": {"type": "string", "description": "Replacement text."}, "replace_all": {"type": "boolean", "description": "Replace every occurrence (default false). When false, fails if old_string is not unique."}}, "required": ["path", "old_string", "new_string"]}}),
        json!({"name": "lightarchitects_bash", "description": "Execute a shell command and return its output (stdout + stderr) and exit code.", "inputSchema": {"type": "object", "properties": {"command": {"type": "string", "description": "Shell command to execute."}, "timeout_ms": {"type": "integer", "description": "Abort timeout in milliseconds (default 120000)."}, "cwd": {"type": "string", "description": "Working directory for the command (optional)."}}, "required": ["command"]}}),
        json!({"name": "lightarchitects_search", "description": "Search file contents using ripgrep (rg), with grep fallback.", "inputSchema": {"type": "object", "properties": {"pattern": {"type": "string", "description": "Regex pattern to search for."}, "path": {"type": "string", "description": "Directory or file to search (default: cwd)."}, "glob": {"type": "string", "description": "File glob filter, e.g. \"*.rs\"."}, "case_insensitive": {"type": "boolean", "description": "Case-insensitive search (default false)."}}, "required": ["pattern"]}}),
        json!({"name": "lightarchitects_glob", "description": "Find files matching a glob pattern.", "inputSchema": {"type": "object", "properties": {"pattern": {"type": "string", "description": "Glob pattern, e.g. \"**/*.rs\" or \"*.toml\"."}, "path": {"type": "string", "description": "Base directory to search (default: cwd)."}}, "required": ["pattern"]}}),
    ]
}

/// Platform tool definitions: discover, `ask_user`, orchestrate.
fn platform_tool_defs() -> Vec<Value> {
    vec![
        json!({"name": "lightarchitects_discover", "description": "Report gateway version, available core tools, and agent status.", "inputSchema": {"type": "object", "properties": {}}}),
        json!({"name": "lightarchitects_ask_user", "description": "Present a question to the user. Writes to stderr so the host can intercept and collect a response.", "inputSchema": {"type": "object", "properties": {"question": {"type": "string", "description": "Question to ask the user."}, "options": {"type": "array", "items": {"type": "string"}, "description": "Optional list of allowed answer choices."}}, "required": ["question"]}}),
        json!({
            "name": "lightarchitects_orchestrate",
            "description": "Route a request to a Light Architects target (CORSO, EVA, SOUL, QUANTUM, SERAPH, AYIN). Auto-routes by action keyword if agent is not specified. Returns a structured error when the target is not enabled.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "description": "Action to perform, e.g. 'guard', 'memory', 'query', 'helix', 'scan', 'metrics'."
                    },
                    "agent": {
                        "type": "string",
                        "description": "Target to route to (optional — auto-routes if omitted).",
                        "enum": ["corso", "eva", "soul", "quantum", "seraph", "ayin"]
                    },
                    "params": {
                        "type": "object",
                        "description": "Action-specific parameters forwarded to the target's MCP tool."
                    }
                },
                "required": ["action"]
            }
        }),
    ]
}

/// Squad tool definitions: `canon_check`, `canon_evaluate`, initialize, import.
fn squad_tool_defs() -> Vec<Value> {
    vec![
        json!({"name": "lightarchitects_canon_check", "description": "Check a decision against all ratified Light Architects canons. Returns canon headers from the registry file for the caller to evaluate — this is file-based extraction, not AI reasoning. Full semantic evaluation requires the LÆX model (not available in v1).", "inputSchema": {"type": "object", "properties": {"decision": {"type": "string", "description": "The decision or proposed action to evaluate against canon."}, "verbose": {"type": "boolean", "description": "Include raw canon registry content alongside headers (default false)."}}, "required": ["decision"]}}),
        json!({"name": "lightarchitects_canon_evaluate", "description": "Return a blank 5-criteria evaluation template for a proposed canon candidate: convergent_evidence, biblical_grounding, decision_shaping, pressure_tested, kevin_ratifies. Scores are null — the gateway provides the framework, not the evaluation. Automated scoring requires the LÆX model (not available in v1).", "inputSchema": {"type": "object", "properties": {"candidate": {"type": "string", "description": "The proposed canon statement to evaluate."}}, "required": ["candidate"]}}),
        json!({"name": "lightarchitects_initialize", "description": "Interactive setup wizard for the Light Architects squad. Steps: detect (environment scan), draft (generate config from preset), apply (write config to disk), view (read current config).", "inputSchema": {"type": "object", "properties": {"step": {"type": "string", "description": "Wizard step to run.", "enum": ["detect", "draft", "apply", "view"]}, "preset": {"type": "string", "description": "Starter pack name (for draft/apply). Options: software_engineering, security, research, full_squad, lean.", "enum": ["software_engineering", "security", "research", "full_squad", "lean"]}, "vault_path": {"type": "string", "description": "Vault root path override (for draft/apply, default ~/.soul/helix)."}, "dry_run": {"type": "boolean", "description": "Preview without writing to disk (for apply, default false)."}}, "required": ["step"]}}),
        json!({"name": "lightarchitects_import", "description": "Import content from external systems. Adapters: obsidian/markdown (scan directory for .md files, extract H1 titles), mcp (generate a [agents.<name>] TOML block for a custom agent).", "inputSchema": {"type": "object", "properties": {"adapter": {"type": "string", "description": "Import adapter to use.", "enum": ["obsidian", "markdown", "mcp"]}, "path": {"type": "string", "description": "Directory to scan (required for obsidian/markdown adapters)."}, "name": {"type": "string", "description": "New agent name (required for mcp adapter)."}, "binary": {"type": "string", "description": "Binary path for the new agent (optional, for mcp adapter)."}, "tool_name": {"type": "string", "description": "MCP tool name for the new agent (optional, for mcp adapter)."}, "role": {"type": "string", "description": "Human-readable description of the agent's role (optional, for mcp adapter)."}}, "required": ["adapter"]}}),
    ]
}

// ── MCP server loop ───────────────────────────────────────────────────────────

/// Run the MCP server: read JSON-RPC from stdin, write responses to stdout.
///
/// Exits cleanly when stdin is closed (EOF).
///
/// # Errors
///
/// Returns [`GatewayError::Io`] only for fatal I/O failures on stdout. Individual
/// request errors are encoded as JSON-RPC error responses and do not terminate
/// the loop.
pub async fn run(config: &GatewayConfig) -> Result<(), GatewayError> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut stdout = tokio::io::stdout();
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            // EOF — stdin closed, clean shutdown.
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = handle_line(trimmed, config).await;
        if let Some(resp) = response {
            let mut out = serde_json::to_string(&resp)?;
            out.push('\n');
            stdout.write_all(out.as_bytes()).await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

/// Parse one JSON-RPC line and produce an optional response value.
///
/// Returns `None` for notifications (no `id` field), which require no response.
#[instrument(skip(config))]
async fn handle_line(line: &str, config: &GatewayConfig) -> Option<Value> {
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(error_response(
                Value::Null,
                -32_700,
                &format!("Parse error: {e}"),
            ));
        }
    };

    let id = request.get("id").cloned().unwrap_or(Value::Null);
    // Notifications have no `id`; do not respond to them.
    if id.is_null() && request.get("id").is_none() {
        return None;
    }

    let method = request["method"].as_str().unwrap_or("");
    match method {
        "initialize" => Some(handle_initialize(id)),
        "notifications/initialized" => None,
        "tools/list" => Some(handle_tools_list(id)),
        "tools/call" => Some(handle_tools_call(id, &request, config).await),
        _ => Some(error_response(
            id,
            -32_601,
            &format!("Method not found: {method}"),
        )),
    }
}

/// Respond to the MCP `initialize` handshake.
fn handle_initialize(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {
                "name": "lightarchitects",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

/// Respond to `tools/list` with all tool definitions.
fn handle_tools_list(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {"tools": tool_definitions()}
    })
}

/// Dispatch a `tools/call` request to the appropriate core-tool handler.
async fn handle_tools_call(id: Value, request: &Value, config: &GatewayConfig) -> Value {
    let name = match request["params"]["name"].as_str() {
        Some(n) => n.to_owned(),
        None => {
            return error_response(id, -32_602, "Missing tool name in params");
        }
    };
    let params = request["params"]["arguments"].clone();

    match dispatch(&name, params, config).await {
        Ok(result) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        }),
        Err(e) => error_response(id, -32_603, &e.to_string()),
    }
}

/// Route a tool call to the correct handler.
///
/// # Errors
///
/// Propagates any [`GatewayError`] from the individual tool handlers, plus
/// [`GatewayError::UnknownTool`] for unrecognised names.
#[tracing::instrument(skip(params, config), fields(tool = tool_name))]
async fn dispatch(
    tool_name: &str,
    params: Value,
    config: &GatewayConfig,
) -> Result<Value, GatewayError> {
    match tool_name {
        "tools" => core_tools::meta::run(params, config).await,
        "lightarchitects_read" => core_tools::read::run(params, config),
        "lightarchitects_write" => core_tools::write::run(params, config),
        "lightarchitects_edit" => core_tools::edit::run(params, config),
        "lightarchitects_bash" => core_tools::bash::run(params).await,
        "lightarchitects_search" => core_tools::search::run(params, config).await,
        "lightarchitects_glob" => core_tools::glob::run(params, config).await,
        "lightarchitects_discover" => core_tools::discover::run(params, config),
        "lightarchitects_ask_user" => core_tools::ask_user::run(params),
        "lightarchitects_orchestrate" => core_tools::orchestrate::run(params, config).await,
        "lightarchitects_canon_check" => core_tools::canon_check::run(params, config),
        "lightarchitects_canon_evaluate" => core_tools::canon_evaluate::run(params, config),
        "lightarchitects_initialize" => core_tools::initialize::run(params, config).await,
        "lightarchitects_import" => core_tools::import_adapter::run(params, config),
        _ => Err(GatewayError::UnknownTool(tool_name.to_owned())),
    }
}

/// Build a JSON-RPC error response.
fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message}
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definitions_has_one_entry() {
        assert_eq!(tool_definitions().len(), 1);
        assert_eq!(tool_definitions()[0]["name"], "tools");
    }

    #[test]
    fn all_tool_definitions_has_fourteen_entries() {
        assert_eq!(all_tool_definitions().len(), 14);
    }

    #[test]
    fn all_tool_names_valid() {
        for tool in all_tool_definitions() {
            let name = tool["name"].as_str().unwrap();
            assert!(
                name == "tools" || name.starts_with("lightarchitects_"),
                "tool {name} has invalid name"
            );
        }
    }

    #[tokio::test]
    async fn handle_initialize_returns_capabilities() {
        let cfg = GatewayConfig::default();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let resp = handle_line(&req.to_string(), &cfg).await.unwrap();
        assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
    }

    #[tokio::test]
    async fn handle_tools_list_returns_single_meta_tool() {
        let cfg = GatewayConfig::default();
        let req = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"});
        let resp = handle_line(&req.to_string(), &cfg).await.unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "tools");
    }

    #[tokio::test]
    async fn unknown_method_returns_error() {
        let cfg = GatewayConfig::default();
        let req = json!({"jsonrpc":"2.0","id":3,"method":"nonexistent"});
        let resp = handle_line(&req.to_string(), &cfg).await.unwrap();
        assert!(resp["error"]["code"].as_i64().is_some());
    }

    #[tokio::test]
    async fn notification_returns_none() {
        let cfg = GatewayConfig::default();
        let req = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        let resp = handle_line(&req.to_string(), &cfg).await;
        assert!(resp.is_none());
    }
}
