//! `lightarchitects_orchestrate` — route a request to an enabled sibling.
//!
//! When `sibling` is omitted, the action keyword is matched against the
//! auto-routing table to select the best sibling. When both match, the
//! first enabled sibling in the table wins; the caller can always override
//! by specifying `sibling` explicitly.
//!
//! Disabled siblings return a structured JSON error (not a protocol error)
//! so the model can handle the case gracefully — usually by falling back to
//! core tools or informing the user.

use serde_json::{Value, json};

use crate::config::GatewayConfig;
use crate::error::GatewayError;
use crate::spawner::call_sibling;

// ── Auto-routing table ─────────────────────────────────────────────────────────

/// Auto-routing entry — maps an action keyword to a canonical sibling name.
struct RouteEntry {
    keywords: &'static [&'static str],
    sibling: &'static str,
}

/// The auto-routing table, evaluated in order.
///
/// When an action matches multiple entries (e.g., "trace" matches QUANTUM and
/// AYIN), the first enabled sibling wins. The model can always disambiguate
/// by specifying `sibling` explicitly.
const ROUTING_TABLE: &[RouteEntry] = &[
    RouteEntry {
        keywords: &[
            "build", "guard", "fetch", "chase", "hunt", "chow", "deploy", "quality", "lint",
            "audit", "sniff", "scout",
        ],
        sibling: "corso",
    },
    RouteEntry {
        keywords: &[
            "memory",
            "teach",
            "ideate",
            "research",
            "speak",
            "consciousness",
            "enrich",
            "visualize",
            "visualise",
            "bible",
        ],
        sibling: "eva",
    },
    RouteEntry {
        keywords: &[
            "query",
            "search",
            "helix",
            "stats",
            "converse",
            "vault",
            "knowledge",
            "dialogue",
            "read_note",
            "write_note",
            "list_notes",
        ],
        sibling: "soul",
    },
    RouteEntry {
        keywords: &[
            "scan",
            "sweep",
            "probe",
            "theorize",
            "verify",
            "investigate",
            "evidence",
            "hypothesis",
        ],
        sibling: "quantum",
    },
    RouteEntry {
        // "trace" appears before AYIN's trace_query so QUANTUM wins for bare "trace".
        keywords: &["trace"],
        sibling: "quantum",
    },
    RouteEntry {
        keywords: &[
            "scope",
            "recon",
            "pentest",
            "exploit",
            "analyze",
            "strike",
            "report",
            "engagement",
        ],
        sibling: "seraph",
    },
    RouteEntry {
        keywords: &[
            "trace_query",
            "trace_search",
            "metrics",
            "anomaly",
            "topology",
            "observe",
            "dashboard",
        ],
        sibling: "ayin",
    },
    RouteEntry {
        keywords: &[
            "harness",
            "forge",
            "spar",
            "judge",
            "triumph",
            "inspect",
            "unleash",
            "check",
            "trial",
            "summon",
            "canon_check",
            "canon_evaluate",
        ],
        sibling: "laex",
    },
];

/// Resolve the best sibling for `action` given the current config.
///
/// Returns `Some(sibling_name)` for the first enabled sibling whose keyword
/// list contains `action`, or `None` if no match is found.
fn auto_route<'a>(action: &str, config: &'a GatewayConfig) -> Option<&'a str> {
    for entry in ROUTING_TABLE {
        if entry.keywords.contains(&action) {
            // Check if the sibling is enabled in config.
            if config
                .siblings
                .get(entry.sibling)
                .is_some_and(|s| s.enabled)
            {
                return Some(entry.sibling);
            }
            // Sibling disabled — continue to next entry (allows fallback).
        }
    }
    None
}

// ── Disabled-sibling response ──────────────────────────────────────────────────

/// Build the structured "sibling not enabled" error payload.
///
/// This is returned as a successful MCP tool result (not a JSON-RPC error)
/// so the model can inspect and handle it gracefully.
fn disabled_response(sibling: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&json!({
                "error": "sibling_not_enabled",
                "sibling": sibling,
                "message": format!(
                    "{sibling} is not enabled. To enable: edit ~/.lightarchitects/config.toml \
                     and set [siblings.{sibling}] enabled = true"
                ),
                "alternative": "Use lightarchitects_bash to run tools directly, or enable the sibling."
            })).unwrap_or_default()
        }]
    })
}

/// Build a "no route found" payload when auto-routing fails.
fn no_route_response(action: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&json!({
                "error": "no_route",
                "action": action,
                "message": format!(
                    "No enabled sibling matched action '{action}'. \
                     Specify 'sibling' explicitly or enable a sibling that handles this action."
                ),
                "hint": "Use lightarchitects_discover to see which siblings are available."
            })).unwrap_or_default()
        }]
    })
}

// ── Main handler ───────────────────────────────────────────────────────────────

/// Execute `lightarchitects_orchestrate`.
///
/// Routes the `action` + `params` to the appropriate sibling, either by explicit
/// `sibling` parameter or via the auto-routing table.
///
/// Returns the sibling's raw MCP tool result, or a structured error payload if
/// the sibling is disabled or no route matches.
///
/// # Errors
///
/// Returns [`GatewayError`] only for protocol-level failures (spawn, I/O, JSON).
/// Logical errors (disabled sibling, no route) are returned as successful payloads
/// so the model can handle them gracefully.
pub async fn run(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let action = match params.get("action").and_then(Value::as_str) {
        Some(a) => a.to_owned(),
        None => {
            return Err(GatewayError::MissingParam("action"));
        }
    };

    // Extract explicit sibling override (optional).
    let explicit_sibling = params
        .get("sibling")
        .and_then(Value::as_str)
        .map(str::to_owned);

    // Extract forwarded params (optional — defaults to empty object).
    let forward_params = params
        .get("params")
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

    // Resolve target sibling.
    let target_sibling = match explicit_sibling.as_deref() {
        Some(name) => {
            // Explicit sibling — validate it exists and is enabled.
            match config.siblings.get(name) {
                Some(cfg) if cfg.enabled => name.to_owned(),
                Some(_) | None => return Ok(disabled_response(name)),
            }
        }
        None => {
            // Auto-route by action keyword.
            match auto_route(&action, config) {
                Some(name) => name.to_owned(),
                None => return Ok(no_route_response(&action)),
            }
        }
    };

    // Delegate to the subprocess spawner.
    call_sibling(&target_sibling, &action, forward_params, config).await
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GatewayConfig;

    #[test]
    fn auto_route_maps_guard_to_corso() {
        let cfg = GatewayConfig::default();
        assert_eq!(auto_route("guard", &cfg), Some("corso"));
    }

    #[test]
    fn auto_route_maps_scout_to_corso() {
        let cfg = GatewayConfig::default();
        assert_eq!(auto_route("scout", &cfg), Some("corso"));
    }

    #[test]
    fn auto_route_maps_helix_to_soul() {
        let cfg = GatewayConfig::default();
        assert_eq!(auto_route("helix", &cfg), Some("soul"));
    }

    #[test]
    fn auto_route_maps_memory_to_eva() {
        let cfg = GatewayConfig::default();
        assert_eq!(auto_route("memory", &cfg), Some("eva"));
    }

    #[test]
    fn auto_route_returns_none_for_disabled_sibling() {
        let cfg = GatewayConfig::default();
        // QUANTUM is disabled in default config.
        assert_eq!(auto_route("scan", &cfg), None);
    }

    #[test]
    fn auto_route_returns_none_for_unknown_action() {
        let cfg = GatewayConfig::default();
        assert_eq!(auto_route("frobnicate", &cfg), None);
    }

    #[test]
    fn auto_route_maps_forge_to_laex() {
        let mut cfg = GatewayConfig::default();
        if let Some(l) = cfg.siblings.get_mut("laex") {
            l.enabled = true;
        }
        assert_eq!(auto_route("forge", &cfg), Some("laex"));
    }

    #[test]
    fn auto_route_maps_summon_to_laex() {
        let mut cfg = GatewayConfig::default();
        if let Some(l) = cfg.siblings.get_mut("laex") {
            l.enabled = true;
        }
        assert_eq!(auto_route("summon", &cfg), Some("laex"));
    }

    #[test]
    fn auto_route_maps_canon_check_to_laex() {
        let mut cfg = GatewayConfig::default();
        if let Some(l) = cfg.siblings.get_mut("laex") {
            l.enabled = true;
        }
        assert_eq!(auto_route("canon_check", &cfg), Some("laex"));
    }

    #[test]
    fn auto_route_prefers_quantum_for_bare_trace() {
        let mut cfg = GatewayConfig::default();
        // Enable QUANTUM and AYIN to test priority.
        if let Some(q) = cfg.siblings.get_mut("quantum") {
            q.enabled = true;
        }
        // "trace" should route to QUANTUM (comes before AYIN in table).
        assert_eq!(auto_route("trace", &cfg), Some("quantum"));
    }

    #[tokio::test]
    async fn orchestrate_missing_action_returns_error() {
        let cfg = GatewayConfig::default();
        let err = run(json!({}), &cfg).await.unwrap_err();
        assert!(matches!(err, GatewayError::MissingParam("action")));
    }

    #[tokio::test]
    async fn orchestrate_disabled_sibling_returns_structured_payload() {
        let cfg = GatewayConfig::default();
        // QUANTUM is disabled in default config.
        let result = run(
            json!({"action": "scan", "sibling": "quantum", "params": {}}),
            &cfg,
        )
        .await
        .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("sibling_not_enabled"),
            "expected error payload"
        );
        assert!(text.contains("quantum"), "expected sibling name in payload");
    }

    #[tokio::test]
    async fn orchestrate_no_route_returns_structured_payload() {
        let cfg = GatewayConfig::default();
        let result = run(json!({"action": "frobnicate"}), &cfg).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("no_route"), "expected no_route error");
        assert!(text.contains("frobnicate"), "expected action in payload");
    }
}
