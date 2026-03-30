//! `lightarchitects_orchestrate` — route a request to an enabled sibling.
//!
//! Routing is driven by the canonical action enums in the SDK sibling crates.
//! When `sibling` is omitted, the action string is parsed against each sibling's
//! enum in priority order. When both match, the first enabled sibling wins; the
//! caller can always override by specifying `sibling` explicitly.
//!
//! Priority order: QUANTUM > CORSO > SERAPH > EVA > SOUL > AYIN.
//! This ensures QUANTUM's `research` wins over SOUL's, and CORSO's domain-heavy
//! actions come before SOUL's generic names (search, query, stats).

use serde_json::{Value, json};

use lightarchitects_ayin::AyinAction;
use lightarchitects_corso::CorsoAction;
use lightarchitects_eva::EvaAction;
use lightarchitects_quantum::QuantumAction;
use lightarchitects_seraph::SeraphAction;
use lightarchitects_soul::SoulAction;

use crate::config::GatewayConfig;
use crate::error::GatewayError;
use crate::spawner::call_sibling;

// ── Auto-routing via SDK enums ───────────────────────────────────────────────

/// Sibling routing entry: name, parse-and-check function.
struct SiblingRoute {
    name: &'static str,
    matches: fn(&str) -> bool,
}

/// Check whether `action` parses as a routable action for the given enum.
fn is_routable_quantum(action: &str) -> bool {
    action
        .parse::<QuantumAction>()
        .is_ok_and(|a| a.is_gateway_routable())
}

/// Check whether `action` parses as a routable CORSO action.
fn is_routable_corso(action: &str) -> bool {
    action
        .parse::<CorsoAction>()
        .is_ok_and(|a| a.is_gateway_routable())
}

/// Check whether `action` parses as a routable SERAPH action.
fn is_routable_seraph(action: &str) -> bool {
    action
        .parse::<SeraphAction>()
        .is_ok_and(|a| a.is_gateway_routable())
}

/// Check whether `action` parses as a routable EVA action.
fn is_routable_eva(action: &str) -> bool {
    action
        .parse::<EvaAction>()
        .is_ok_and(|a| a.is_gateway_routable())
}

/// Check whether `action` parses as a routable SOUL action.
fn is_routable_soul(action: &str) -> bool {
    action
        .parse::<SoulAction>()
        .is_ok_and(|a| a.is_gateway_routable())
}

/// Check whether `action` parses as a routable AYIN action.
fn is_routable_ayin(action: &str) -> bool {
    action
        .parse::<AyinAction>()
        .is_ok_and(|a| a.is_gateway_routable())
}

/// Priority-ordered sibling routing table.
///
/// Order: QUANTUM > CORSO > SERAPH > EVA > SOUL > AYIN.
///
/// Rationale:
/// - QUANTUM first: its `research` must win over SOUL's `research`.
/// - CORSO second: domain-heavy security/ops actions.
/// - SERAPH third: pentest investigation actions.
/// - EVA fourth: creative/consciousness actions.
/// - SOUL fifth: generic vault names (search, query, stats) only match if
///   no other sibling claims them.
/// - AYIN last: observability (sessions, spans, conversations).
const SIBLING_ROUTES: &[SiblingRoute] = &[
    SiblingRoute {
        name: "quantum",
        matches: is_routable_quantum,
    },
    SiblingRoute {
        name: "corso",
        matches: is_routable_corso,
    },
    SiblingRoute {
        name: "seraph",
        matches: is_routable_seraph,
    },
    SiblingRoute {
        name: "eva",
        matches: is_routable_eva,
    },
    SiblingRoute {
        name: "soul",
        matches: is_routable_soul,
    },
    SiblingRoute {
        name: "ayin",
        matches: is_routable_ayin,
    },
];

/// Resolve the best sibling for `action` given the current config.
///
/// Parses `action` against each sibling's canonical enum in priority order
/// (QUANTUM > CORSO > SERAPH > EVA > SOUL > AYIN). Returns `Some(name)`
/// for the first enabled sibling whose enum recognises `action` as routable,
/// or `None` if no match is found.
fn auto_route<'a>(action: &str, config: &'a GatewayConfig) -> Option<&'a str> {
    for route in SIBLING_ROUTES {
        if (route.matches)(action) {
            if let Some(cfg) = config.siblings.get(route.name) {
                if cfg.enabled {
                    return Some(route.name);
                }
            }
            // Sibling disabled or absent — continue to next (allows fallback).
        }
    }
    None
}

/// Return the total number of gateway-routable actions across all siblings.
#[must_use]
pub fn total_routable_action_count() -> usize {
    QuantumAction::ALL_ROUTABLE.len()
        + CorsoAction::ALL_ROUTABLE.len()
        + SeraphAction::ALL_ROUTABLE.len()
        + EvaAction::ALL_ROUTABLE.len()
        + SoulAction::ALL_ROUTABLE.len()
        + AyinAction::ALL_ROUTABLE.len()
}

/// Collect all routable action names for a given sibling.
///
/// Returns an empty slice for unknown sibling names.
#[must_use]
pub fn routable_actions_for(sibling: &str) -> Vec<&'static str> {
    match sibling {
        "quantum" => QuantumAction::ALL_ROUTABLE
            .iter()
            .map(QuantumAction::as_str)
            .collect(),
        "corso" => CorsoAction::ALL_ROUTABLE
            .iter()
            .map(CorsoAction::as_str)
            .collect(),
        "seraph" => SeraphAction::ALL_ROUTABLE
            .iter()
            .map(SeraphAction::as_str)
            .collect(),
        "eva" => EvaAction::ALL_ROUTABLE
            .iter()
            .map(EvaAction::as_str)
            .collect(),
        "soul" => SoulAction::ALL_ROUTABLE
            .iter()
            .map(SoulAction::as_str)
            .collect(),
        "ayin" => AyinAction::ALL_ROUTABLE
            .iter()
            .map(AyinAction::as_str)
            .collect(),
        _ => Vec::new(),
    }
}

// ── Disabled-sibling response ────────────────────────────────────────────────

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

// ── Main handler ─────────────────────────────────────────────────────────────

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
            // Auto-route by action keyword via SDK enums.
            match auto_route(&action, config) {
                Some(name) => name.to_owned(),
                None => return Ok(no_route_response(&action)),
            }
        }
    };

    // Delegate to the subprocess spawner.
    call_sibling(&target_sibling, &action, forward_params, config).await
}

// ── Tests ────────────────────────────────────────────────────────────────────

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
    fn auto_route_maps_visualize_to_eva() {
        let cfg = GatewayConfig::default();
        assert_eq!(auto_route("visualize", &cfg), Some("eva"));
    }

    #[test]
    fn auto_route_returns_none_for_disabled_sibling() {
        let cfg = GatewayConfig::default();
        // QUANTUM is disabled in default config; "triage" is QUANTUM-only.
        assert_eq!(auto_route("triage", &cfg), None);
    }

    #[test]
    fn auto_route_returns_none_for_unknown_action() {
        let cfg = GatewayConfig::default();
        assert_eq!(auto_route("frobnicate", &cfg), None);
    }

    #[test]
    fn auto_route_returns_none_for_arena_actions() {
        // Arena actions are not in any SDK enum.
        let mut cfg = GatewayConfig::default();
        if let Some(l) = cfg.siblings.get_mut("laex") {
            l.enabled = true;
        }
        assert_eq!(auto_route("forge", &cfg), None);
        assert_eq!(auto_route("summon", &cfg), None);
        assert_eq!(auto_route("canon_check", &cfg), None);
    }

    #[test]
    fn auto_route_prefers_quantum_for_research() {
        let mut cfg = GatewayConfig::default();
        // Enable QUANTUM to test priority over SOUL.
        if let Some(q) = cfg.siblings.get_mut("quantum") {
            q.enabled = true;
        }
        // "research" exists in both QUANTUM and SOUL.
        // QUANTUM has higher priority and should win.
        assert_eq!(auto_route("research", &cfg), Some("quantum"));
    }

    #[test]
    fn auto_route_research_falls_back_to_soul_when_quantum_disabled() {
        let cfg = GatewayConfig::default();
        // QUANTUM is disabled in default config; SOUL is enabled.
        assert_eq!(auto_route("research", &cfg), Some("soul"));
    }

    #[test]
    fn auto_route_prefers_quantum_for_trace() {
        let mut cfg = GatewayConfig::default();
        if let Some(q) = cfg.siblings.get_mut("quantum") {
            q.enabled = true;
        }
        // "trace" is a QUANTUM workflow action.
        assert_eq!(auto_route("trace", &cfg), Some("quantum"));
    }

    #[test]
    fn all_sdk_routable_actions_route_correctly() {
        // Enable all siblings.
        let mut cfg = GatewayConfig::default();
        for (_, sib) in cfg.siblings.iter_mut() {
            sib.enabled = true;
        }

        // Verify every routable action for each sibling resolves to the
        // correct sibling (accounting for priority — some actions may route
        // to a higher-priority sibling instead).
        let expected: &[(&str, &[&str])] = &[
            (
                "quantum",
                &[
                    "triage", "sweep", "trace", "probe", "theorize", "verify", "close", "quick",
                    "research",
                ],
            ),
            (
                "corso",
                &[
                    "sniff",
                    "guard",
                    "fetch",
                    "chase",
                    "scout",
                    "code_review",
                    "generate_code",
                    "search_code",
                    "find_symbol",
                    "get_outline",
                    "get_references",
                    "analyze_architecture",
                    "prove",
                    "optimize",
                    "deploy",
                    "rollback",
                    "manage_logs",
                    "strike",
                    "watch",
                ],
            ),
            (
                "seraph",
                &[
                    "status",
                    "investigate_start",
                    "investigate_advance",
                    "investigate_close",
                    "investigate_report",
                    "vault_sync",
                ],
            ),
            (
                "eva",
                &[
                    "visualize",
                    "ideate",
                    "bible_search",
                    "bible_reflect",
                    "teach",
                    "remember",
                    "crystallize",
                    "celebrate",
                    "mindfulness",
                ],
            ),
            (
                "soul",
                &[
                    "read_note",
                    "write_note",
                    "list_notes",
                    "manifest",
                    "ingest",
                    "search",
                    "helix",
                    "query",
                    "query_frontmatter",
                    "stats",
                    "voice",
                    "converse",
                    "chat",
                    // "research" routes to QUANTUM (higher priority).
                ],
            ),
            ("ayin", &["sessions", "spans", "conversations"]),
        ];

        for &(sibling, actions) in expected {
            for &action in actions {
                let result = auto_route(action, &cfg);
                assert_eq!(
                    result,
                    Some(sibling),
                    "action '{action}' should route to '{sibling}', got {result:?}"
                );
            }
        }
    }

    #[test]
    fn collision_priority_research() {
        let mut cfg = GatewayConfig::default();
        for (_, sib) in cfg.siblings.iter_mut() {
            sib.enabled = true;
        }
        // "research" → QUANTUM (higher priority than SOUL).
        assert_eq!(auto_route("research", &cfg), Some("quantum"));
    }

    #[test]
    fn collision_priority_search_routes_to_soul() {
        let mut cfg = GatewayConfig::default();
        for (_, sib) in cfg.siblings.iter_mut() {
            sib.enabled = true;
        }
        // "search" is a SOUL action. CORSO has "search_code" but not bare "search".
        assert_eq!(auto_route("search", &cfg), Some("soul"));
    }

    #[test]
    fn total_routable_count_matches_sdk_enums() {
        // 9 + 19 + 6 + 9 + 14 + 3 = 60
        let total = total_routable_action_count();
        assert_eq!(
            total,
            QuantumAction::ALL_ROUTABLE.len()
                + CorsoAction::ALL_ROUTABLE.len()
                + SeraphAction::ALL_ROUTABLE.len()
                + EvaAction::ALL_ROUTABLE.len()
                + SoulAction::ALL_ROUTABLE.len()
                + AyinAction::ALL_ROUTABLE.len(),
        );
    }

    #[test]
    fn routable_actions_for_corso_matches_enum() {
        let actions = routable_actions_for("corso");
        assert_eq!(actions.len(), CorsoAction::ALL_ROUTABLE.len());
        for &expected in CorsoAction::ALL_ROUTABLE {
            assert!(
                actions.contains(&expected.as_str()),
                "missing CORSO action: {}",
                expected.as_str()
            );
        }
    }

    #[test]
    fn routable_actions_for_unknown_is_empty() {
        assert!(routable_actions_for("nonexistent").is_empty());
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
            json!({"action": "triage", "sibling": "quantum", "params": {}}),
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
