//! `lightarchitects_orchestrate` — route a request to an enabled route.
//!
//! Routing is driven by the canonical action enums in the SDK route crates.
//! When `route` is omitted, the action string is parsed against each route's
//! enum in priority order. When both match, the first enabled route wins; the
//! caller can always override by specifying `route` explicitly.
//!
//! Priority order: QUANTUM > CORSO > SERAPH > EVA > SOUL > AYIN.
//! This ensures QUANTUM's `research` wins over SOUL's, and CORSO's domain-heavy
//! actions come before SOUL's generic names (search, query, stats).

use serde_json::{Value, json};

use lightarchitects::ayin::AyinAction;
use lightarchitects::corso::CorsoAction;
use lightarchitects::eva::EvaAction;
use lightarchitects::quantum::QuantumAction;
use lightarchitects::seraph::SeraphAction;
use lightarchitects::soul::SoulAction;

use crate::config::GatewayConfig;
use crate::error::GatewayError;

#[cfg(feature = "spawner")]
use crate::spawner::call_agent;

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

/// Priority-ordered route routing table.
///
/// Order: QUANTUM > CORSO > SERAPH > EVA > SOUL > AYIN.
///
/// Rationale:
/// - QUANTUM first: its `research` must win over SOUL's `research`.
/// - CORSO second: domain-heavy security/ops actions.
/// - SERAPH third: pentest investigation actions.
/// - EVA fourth: creative/consciousness actions.
/// - SOUL fifth: generic vault names (search, query, stats) only match if
///   no other route claims them.
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

/// Resolve the best teammate for `action` given the current config.
///
/// Uses the active preset's routing priority order. Falls back to the
/// static `SIBLING_ROUTES` order for teammates not in the preset's list.
///
/// Returns `Some(name)` for the first enabled teammate whose enum
/// recognises `action` as routable, or `None` if no match is found.
fn auto_route<'a>(action: &str, config: &'a GatewayConfig) -> Option<&'a str> {
    auto_route_with_priority(action, config, super::preset::active_routing_priority())
}

/// Inner routing function that accepts an explicit priority order.
///
/// Tries each name in `priority` first, then falls back to any remaining
/// teammates in the static `SIBLING_ROUTES` order.
fn auto_route_with_priority<'a>(
    action: &str,
    config: &'a GatewayConfig,
    priority: &[&str],
) -> Option<&'a str> {
    // Phase 1: check teammates in preset priority order.
    for &name in priority {
        if let Some(route) = SIBLING_ROUTES.iter().find(|r| r.name == name) {
            if (route.matches)(action) {
                if let Some(cfg) = config.agents.get(route.name) {
                    if cfg.enabled {
                        return Some(route.name);
                    }
                }
            }
        }
    }

    // Phase 2: fallback to static order for teammates NOT in the preset.
    for route in SIBLING_ROUTES {
        if priority.contains(&route.name) {
            continue; // Already checked in phase 1.
        }
        if (route.matches)(action) {
            if let Some(cfg) = config.agents.get(route.name) {
                if cfg.enabled {
                    return Some(route.name);
                }
            }
        }
    }

    None
}

/// Return the total number of gateway-routable actions across all agents.
#[must_use]
pub fn total_routable_action_count() -> usize {
    QuantumAction::ALL_ROUTABLE.len()
        + CorsoAction::ALL_ROUTABLE.len()
        + SeraphAction::ALL_ROUTABLE.len()
        + EvaAction::ALL_ROUTABLE.len()
        + SoulAction::ALL_ROUTABLE.len()
        + AyinAction::ALL_ROUTABLE.len()
}

/// Collect all routable action names for a given route.
///
/// Returns an empty slice for unknown route names.
#[must_use]
pub fn routable_actions_for(agent: &str) -> Vec<&'static str> {
    match agent {
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

// ── Disabled-route response ────────────────────────────────────────────────

/// Build the structured "agent not enabled" error payload.
///
/// This is returned as a successful MCP tool result (not a JSON-RPC error)
/// so the model can inspect and handle it gracefully.
fn disabled_response(agent: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&json!({
                "error": "agent_not_enabled",
                "agent": agent,
                "message": format!(
                    "{agent} is not enabled. To enable: edit ~/.lightarchitects/config.toml \
                     and set [agents.{agent}] enabled = true"
                ),
                "alternative": "Use lightarchitects_bash to run tools directly, or enable the agent."
            })).unwrap_or_default()
        }]
    })
}

/// Build a "no agent found" payload when auto-routing fails.
fn no_agent_response(action: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&json!({
                "error": "no_agent",
                "action": action,
                "message": format!(
                    "No enabled target matched action '{action}'. \
                     Specify 'agent' explicitly or enable a target that handles this action."
                ),
                "hint": "Use lightarchitects_discover to see which routes are available."
            })).unwrap_or_default()
        }]
    })
}

// ── Main handler ─────────────────────────────────────────────────────────────

/// Execute `lightarchitects_orchestrate`.
///
/// Routes the `action` + `params` to the appropriate route, either by explicit
/// `route` parameter or via the auto-routing table.
///
/// Returns the agent's raw MCP tool result, or a structured error payload if
/// the route is disabled or no agent matches.
///
/// # Errors
///
/// Returns [`GatewayError`] only for protocol-level failures (spawn, I/O, JSON).
/// Logical errors (disabled agent, no agent) are returned as successful payloads
/// so the model can handle them gracefully.
pub async fn run(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let action = match params.get("action").and_then(Value::as_str) {
        Some(a) => a.to_owned(),
        None => {
            return Err(GatewayError::MissingParam("action"));
        }
    };

    // Extract explicit route override (optional).
    let explicit_route = params
        .get("agent")
        .and_then(Value::as_str)
        .map(str::to_owned);

    // Extract forwarded params (optional — defaults to empty object).
    let forward_params = params
        .get("params")
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

    // Resolve target route.
    let target_route = match explicit_route.as_deref() {
        Some(name) => {
            // Explicit route — validate it exists and is enabled.
            match config.agents.get(name) {
                Some(cfg) if cfg.enabled => name.to_owned(),
                Some(_) | None => return Ok(disabled_response(name)),
            }
        }
        None => {
            // Auto-route by action keyword via SDK enums.
            match auto_route(&action, config) {
                Some(name) => name.to_owned(),
                None => return Ok(no_agent_response(&action)),
            }
        }
    };

    // ── Dual-path dispatch ──────────────────────────────────────────────────
    // 1. Try in-process handler (compile-time feature gate + runtime config).
    // 2. Fall back to subprocess spawn (current behaviour).

    #[cfg(any(
        feature = "inline-ayin",
        feature = "inline-corso",
        feature = "inline-eva",
        feature = "inline-soul",
        feature = "inline-quantum",
    ))]
    if let Some(registry) = crate::handlers::registry() {
        if let Some(handler) = registry.get(&target_route) {
            return handler
                .call(&action, forward_params)
                .await
                .map_err(GatewayError::from);
        }
    }

    #[cfg(feature = "spawner")]
    {
        return call_agent(&target_route, &action, forward_params, config).await;
    }

    // No inline handler matched and spawner is not compiled in.
    #[cfg(not(feature = "spawner"))]
    {
        Err(GatewayError::AgentNotEnabled(target_route))
    }
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
    fn auto_route_returns_none_for_disabled_agent() {
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
        if let Some(l) = cfg.agents.get_mut("laex") {
            l.enabled = true;
        }
        assert_eq!(auto_route("forge", &cfg), None);
        assert_eq!(auto_route("summon", &cfg), None);
        assert_eq!(auto_route("canon_check", &cfg), None);
    }

    #[test]
    fn auto_route_research_routes_to_quantum() {
        let mut cfg = GatewayConfig::default();
        if let Some(q) = cfg.agents.get_mut("quantum") {
            q.enabled = true;
        }
        // "research" belongs exclusively to QUANTUM since soul_search rename.
        let full = super::super::preset::find_preset("full").unwrap();
        assert_eq!(
            auto_route_with_priority("research", &cfg, full.routing_priority),
            Some("quantum")
        );
    }

    #[test]
    fn auto_route_soul_search_routes_to_soul() {
        let cfg = GatewayConfig::default();
        // "soul_search" is SOUL's renamed research action — no collision.
        assert_eq!(auto_route("soul_search", &cfg), Some("soul"));
    }

    #[test]
    fn auto_route_prefers_quantum_for_trace_in_full_preset() {
        let mut cfg = GatewayConfig::default();
        if let Some(q) = cfg.agents.get_mut("quantum") {
            q.enabled = true;
        }
        let full = super::super::preset::find_preset("full").unwrap();
        // "trace" is a QUANTUM workflow action — routes correctly in full preset.
        assert_eq!(
            auto_route_with_priority("trace", &cfg, full.routing_priority),
            Some("quantum")
        );
    }

    #[test]
    fn all_sdk_routable_actions_route_correctly_with_full_preset() {
        // Enable all teammates and use the full preset priority.
        let mut cfg = GatewayConfig::default();
        for sib in cfg.agents.values_mut() {
            sib.enabled = true;
        }

        let full = super::super::preset::find_preset("full").unwrap();

        // Verify every routable action for each teammate resolves correctly
        // under the full preset's priority (QUANTUM > CORSO > SERAPH > EVA > SOUL > AYIN).
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
                    "soul_search",
                ],
            ),
            ("ayin", &["sessions", "spans", "conversations"]),
        ];

        for &(teammate, actions) in expected {
            for &action in actions {
                let result = auto_route_with_priority(action, &cfg, full.routing_priority);
                assert_eq!(
                    result,
                    Some(teammate),
                    "action '{action}' should route to '{teammate}', got {result:?}"
                );
            }
        }
    }

    #[test]
    fn research_routes_to_quantum_full_preset() {
        let mut cfg = GatewayConfig::default();
        for sib in cfg.agents.values_mut() {
            sib.enabled = true;
        }
        let full = super::super::preset::find_preset("full").unwrap();
        // "research" belongs to QUANTUM only — no collision since soul_search rename.
        assert_eq!(
            auto_route_with_priority("research", &cfg, full.routing_priority),
            Some("quantum")
        );
    }

    #[test]
    fn soul_search_routes_to_soul_default_preset() {
        // Default preset (software_engineering) includes SOUL.
        // "soul_search" is SOUL's renamed research action.
        let cfg = GatewayConfig::default();
        assert_eq!(auto_route("soul_search", &cfg), Some("soul"));
    }

    #[test]
    fn collision_priority_search_routes_to_soul() {
        let mut cfg = GatewayConfig::default();
        for sib in cfg.agents.values_mut() {
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
    async fn orchestrate_disabled_agent_returns_structured_payload() {
        let cfg = GatewayConfig::default();
        // QUANTUM is disabled in default config.
        let result = run(
            json!({"action": "triage", "agent": "quantum", "params": {}}),
            &cfg,
        )
        .await
        .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("agent_not_enabled"), "expected error payload");
        assert!(text.contains("quantum"), "expected agent name in payload");
    }

    #[tokio::test]
    async fn orchestrate_no_agent_returns_structured_payload() {
        let cfg = GatewayConfig::default();
        let result = run(json!({"action": "frobnicate"}), &cfg).await.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("no_agent"), "expected no_agent error");
        assert!(text.contains("frobnicate"), "expected action in payload");
    }
}
