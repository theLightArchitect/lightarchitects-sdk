//! `tools` — unified meta-tool following the sibling pattern.
//!
//! One tool, one schema: `{action, params, sibling?}`. Core actions (read, write,
//! bash, etc.) dispatch to the gateway's own handlers. Everything else auto-routes
//! to the correct sibling via the orchestrate routing table. The `sibling` param is
//! optional — only needed to disambiguate the rare action-name collision (e.g.,
//! "search" = core ripgrep vs SOUL vault search).
//!
//! The special action `"list"` returns a catalog of all available actions grouped
//! by category.

use serde_json::{Value, json};

use super::{
    ask_user, ayin_http, bash, canon_check, canon_evaluate, discover, edit, glob, import_adapter,
    initialize, orchestrate, read, search, text_result, write,
};
use crate::config::GatewayConfig;
use crate::error::GatewayError;

// ── Core action names ─────────────────────────────────────────────────────────

/// Actions handled directly by the gateway (no sibling needed).
const CORE_ACTIONS: &[&str] = &[
    "read",
    "write",
    "edit",
    "bash",
    "search",
    "glob",
    "discover",
    "ask_user",
    "initialize",
    "import",
    "canon_check",
    "canon_evaluate",
];

/// Arena action names — not available in this release.
/// Kept as a const for clear unavailability messaging.
const ARENA_ACTIONS: &[&str] = &[
    "harness", "forge", "spar", "judge", "triumph", "inspect", "unleash", "check", "trial",
    "summon",
];

/// Check whether `action` is a core action (handled by the gateway itself).
fn is_core_action(action: &str) -> bool {
    CORE_ACTIONS.contains(&action)
}

// ── List action ───────────────────────────────────────────────────────────────

/// Build the action catalog for `action: "list"`.
///
/// Sibling action lists are generated from SDK enums — not hardcoded.
fn list_actions(config: &GatewayConfig) -> Result<Value, GatewayError> {
    let mut sibling_section = serde_json::Map::new();

    for (name, cfg) in &config.siblings {
        let status = if cfg.enabled { "enabled" } else { "disabled" };
        let actions = orchestrate::routable_actions_for(name);
        sibling_section.insert(
            name.clone(),
            json!({
                "status": status,
                "role": cfg.role,
                "tool_name": cfg.tool_name,
                "actions": actions,
            }),
        );
    }

    let catalog = json!({
        "core": {
            "read":           "Read file contents (path, offset?, limit?)",
            "write":          "Create or overwrite a file (path, content)",
            "edit":           "String replacement in a file (path, old_string, new_string, replace_all?)",
            "bash":           "Execute a shell command (command, timeout_ms?, cwd?)",
            "search":         "Search file contents via ripgrep (pattern, path?, glob?, case_insensitive?)",
            "glob":           "Find files matching a pattern (pattern, path?)",
            "discover":       "Report gateway version, tools, and sibling status",
            "ask_user":       "Present a question to the user (question, options?)",
            "canon_check":    "Validate a decision against the canon registry (decision, verbose?)",
            "canon_evaluate": "Evaluate a canon candidate against 5-criteria framework (candidate)",
        },
        "setup": {
            "initialize":     "Interactive gateway setup wizard (step?)",
            "import":         "Import content from external systems (source, path?, format?)",
        },
        "siblings": sibling_section,
        "ayin": {
            "note": "AYIN actions use HTTP transport to localhost:3742 (not MCP subprocess).",
            "sessions":      "List all trace sessions",
            "spans":         "Load TraceSpan data for a session (actor, date)",
            "conversations": "Load conversation/decision traces (date)",
        },
        "routing": {
            "note": "Non-core actions auto-route to the correct sibling by action keyword. Pass 'sibling' only to override when ambiguous.",
            "priority": "QUANTUM > CORSO > SERAPH > EVA > SOUL > AYIN",
            "total_routable_actions": orchestrate::total_routable_action_count(),
            "examples": [
                {"action": "guard",     "routes_to": "corso"},
                {"action": "scout",     "routes_to": "corso"},
                {"action": "visualize", "routes_to": "eva"},
                {"action": "helix",     "routes_to": "soul"},
                {"action": "triage",    "routes_to": "quantum"},
                {"action": "research",  "routes_to": "quantum (priority over soul)"},
                {"action": "status",    "routes_to": "seraph"},
                {"action": "sessions",  "routes_to": "ayin (HTTP)"},
            ],
        },
    });

    Ok(text_result(serde_json::to_string_pretty(&catalog)?))
}

// ── Main handler ──────────────────────────────────────────────────────────────

/// Execute the `tools` meta-tool.
///
/// Dispatches by `action`:
/// 1. `"list"` → return the action catalog.
/// 2. Core action name → dispatch to the gateway's own handler.
/// 3. Everything else → forward through the orchestrate auto-routing table.
///
/// The `sibling` and `params` fields are extracted and forwarded as needed.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] if `action` is absent.
/// Propagates errors from the dispatched handler.
pub async fn run(arguments: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let action = match arguments.get("action").and_then(Value::as_str) {
        Some(a) => a.to_owned(),
        None => return Err(GatewayError::MissingParam("action")),
    };

    // Extract params (optional — defaults to empty object).
    let params = arguments
        .get("params")
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

    // 1. List action — return the catalog.
    if action == "list" {
        return list_actions(config);
    }

    // 2. Core actions — dispatch to the gateway's own handler.
    if is_core_action(&action) {
        return dispatch_core(&action, params, config).await;
    }

    // 3. Arena actions — not available in this release.
    if ARENA_ACTIONS.contains(&action.as_str()) {
        return Ok(text_result(
            "Arena actions are not available in this release. \
             They will be enabled when the Arena binary ships.",
        ));
    }

    // 4. AYIN actions — HTTP transport, not MCP subprocess.
    //    AYIN runs as a LaunchAgent HTTP server at localhost:3742. Intercept
    //    its routable actions here to avoid the subprocess spawner (which
    //    would fail because AYIN's binary is an HTTP server, not an MCP
    //    stdio server).
    if ayin_http::is_ayin_action(&action) {
        return ayin_http::dispatch(&action, params).await;
    }

    // 5. Everything else — forward through orchestrate.
    //    Build the orchestrate params: {action, sibling?, params}.
    let mut orchestrate_params = serde_json::Map::new();
    orchestrate_params.insert("action".to_owned(), Value::String(action));

    // Forward explicit sibling override if provided.
    if let Some(sibling) = arguments.get("sibling") {
        orchestrate_params.insert("sibling".to_owned(), sibling.clone());
    }

    orchestrate_params.insert("params".to_owned(), params);

    orchestrate::run(Value::Object(orchestrate_params), config).await
}

/// Dispatch a core action to the appropriate handler.
async fn dispatch_core(
    action: &str,
    params: Value,
    config: &GatewayConfig,
) -> Result<Value, GatewayError> {
    match action {
        "read" => read::run(params),
        "write" => write::run(params),
        "edit" => edit::run(params),
        "bash" => bash::run(params).await,
        "search" => search::run(params).await,
        "glob" => glob::run(params).await,
        "discover" => discover::run(params, config),
        "ask_user" => ask_user::run(params),
        "canon_check" => canon_check::run(params, config),
        "canon_evaluate" => canon_evaluate::run(params, config),
        "initialize" => initialize::run(params, config).await,
        "import" => import_adapter::run(params, config),
        _ => Err(GatewayError::UnknownTool(action.to_owned())),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_actions_recognized() {
        assert!(is_core_action("read"));
        assert!(is_core_action("bash"));
        assert!(!is_core_action("guard"));
        assert!(!is_core_action("helix"));
        assert!(!is_core_action("forge"));
        // canon_check and canon_evaluate are now gateway-native core actions.
        assert!(is_core_action("canon_check"));
        assert!(is_core_action("canon_evaluate"));
    }

    #[test]
    fn arena_actions_not_core() {
        for action in ARENA_ACTIONS {
            assert!(
                !is_core_action(action),
                "arena action '{action}' should not be a core action"
            );
        }
    }

    #[test]
    fn list_action_returns_catalog() {
        let cfg = GatewayConfig::default();
        let result = list_actions(&cfg).expect("list");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("read"), "core actions missing");
        assert!(text.contains("siblings"), "siblings missing");
        assert!(text.contains("routing"), "routing examples missing");
    }

    #[tokio::test]
    async fn missing_action_returns_error() {
        let cfg = GatewayConfig::default();
        let err = run(json!({}), &cfg).await.unwrap_err();
        assert!(matches!(err, GatewayError::MissingParam("action")));
    }

    #[tokio::test]
    async fn core_action_dispatches_to_handler() {
        let cfg = GatewayConfig::default();
        // discover is a core action that works without file I/O.
        let result = run(json!({"action": "discover"}), &cfg).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("gateway_version"),
            "discover should return gateway info"
        );
    }

    #[tokio::test]
    async fn non_core_action_routes_to_orchestrate() {
        let cfg = GatewayConfig::default();
        // "guard" should route to CORSO (enabled in default config).
        // It won't actually succeed (binary might not exist in test env),
        // but it should NOT be caught by core dispatch.
        let result = run(json!({"action": "guard", "params": {}}), &cfg).await;
        // Either spawns CORSO or fails with SpawnFailed — NOT UnknownTool.
        match result {
            Ok(_) => {}                                 // CORSO responded (binary exists)
            Err(GatewayError::SpawnFailed { .. }) => {} // Expected — binary path issue in test
            Err(GatewayError::McpProtocol { .. }) => {} // Expected — CORSO might not respond
            Err(GatewayError::Governance { .. }) => {}  // Expected — possible governance block
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_action_works() {
        let cfg = GatewayConfig::default();
        let result = run(json!({"action": "list"}), &cfg).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("core"), "list should include core section");
        assert!(
            text.contains("canon_check"),
            "list should include canon_check in core"
        );
        assert!(
            !text.contains("\"arena\""),
            "list should not include arena section"
        );
    }

    #[tokio::test]
    async fn sibling_override_forwarded() {
        let cfg = GatewayConfig::default();
        // Explicit sibling should be forwarded to orchestrate.
        // Using a disabled sibling to get a predictable error.
        let result = run(
            json!({"action": "scan", "sibling": "quantum", "params": {}}),
            &cfg,
        )
        .await;
        // QUANTUM is disabled in default config — should get disabled response or error.
        match result {
            Ok(v) => {
                let text = v["content"][0]["text"].as_str().unwrap_or("");
                assert!(
                    text.contains("not_enabled") || text.contains("disabled"),
                    "expected disabled sibling response"
                );
            }
            Err(GatewayError::SiblingNotEnabled(_)) => {} // Also acceptable
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn arena_action_returns_unavailable_message() {
        let cfg = GatewayConfig::default();
        let result = run(json!({"action": "forge", "params": {}}), &cfg)
            .await
            .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("not available in this release"),
            "arena actions should return unavailability message, got: {text}"
        );
    }

    #[tokio::test]
    async fn ayin_sessions_dispatches_via_http() {
        let cfg = GatewayConfig::default();
        let result = run(json!({"action": "sessions"}), &cfg).await;
        match result {
            Ok(_) => {} // AYIN is running — response is valid
            Err(GatewayError::Internal(msg)) => {
                // AYIN not running — verify clear error, not UnknownTool
                assert!(
                    msg.contains("AYIN"),
                    "error should reference AYIN, got: {msg}"
                );
            }
            Err(other) => panic!("expected AYIN HTTP error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn ayin_spans_requires_params() {
        let cfg = GatewayConfig::default();
        let err = run(json!({"action": "spans"}), &cfg).await.unwrap_err();
        assert!(
            matches!(err, GatewayError::MissingParam("actor")),
            "expected MissingParam(actor), got {err:?}"
        );
    }

    #[tokio::test]
    async fn ayin_conversations_requires_date() {
        let cfg = GatewayConfig::default();
        let err = run(json!({"action": "conversations"}), &cfg)
            .await
            .unwrap_err();
        assert!(
            matches!(err, GatewayError::MissingParam("date")),
            "expected MissingParam(date), got {err:?}"
        );
    }

    #[tokio::test]
    async fn ayin_action_not_treated_as_core() {
        // AYIN actions should not be in the core action list.
        assert!(!is_core_action("sessions"));
        assert!(!is_core_action("spans"));
        assert!(!is_core_action("conversations"));
    }

    #[tokio::test]
    async fn list_includes_ayin_section() {
        let cfg = GatewayConfig::default();
        let result = run(json!({"action": "list"}), &cfg).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("ayin"),
            "list catalog should include ayin section"
        );
        assert!(
            text.contains("sessions"),
            "list catalog should include sessions action"
        );
    }
}
