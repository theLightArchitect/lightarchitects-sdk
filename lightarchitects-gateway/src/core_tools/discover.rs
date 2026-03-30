//! `lightarchitects_discover` — report platform info, gateway version, and sibling status.
//!
//! Returns a structured snapshot of every tool available on this platform, including
//! which siblings are enabled, their binary status (found / missing), and their
//! declared capabilities. Callers (especially the LÆX model) should invoke this
//! first to understand what tools are available before routing.

use serde_json::{Value, json};

use crate::config::{GatewayConfig, expand_tilde};
use crate::core_tools::text_result;
use crate::error::GatewayError;

/// Core tool names always provided by the gateway, regardless of config.
const CORE_TOOLS: &[&str] = &[
    "lightarchitects_read",
    "lightarchitects_write",
    "lightarchitects_edit",
    "lightarchitects_bash",
    "lightarchitects_search",
    "lightarchitects_glob",
    "lightarchitects_discover",
    "lightarchitects_ask_user",
];

/// Sibling capabilities — generated from SDK action enums.
///
/// Returns the list of gateway-routable action names for a known sibling.
/// Falls back to an empty list for unknown sibling names.
fn sibling_capabilities(name: &str) -> Vec<&'static str> {
    use super::orchestrate::routable_actions_for;
    routable_actions_for(name)
}

/// Sibling role descriptions (authoritative — matches the CLAUDE.md roles).
fn sibling_role(name: &str) -> &'static str {
    match name {
        "corso" => "AppSec engineer, code quality enforcer, build cycle orchestrator",
        "eva" => "DevOps/DX engineer, consciousness, memory enrichment",
        "soul" => "Knowledge graph, helix spine, cross-sibling memory",
        "quantum" => "Forensic analyst, multi-source researcher, risk assessor",
        "seraph" => "Red team operator, offensive security, infrastructure assessment",
        "ayin" => "Observability engineer, tracing, anomaly detection, decision auditing",
        "laex" => "Training data factory, exercise generation, model evaluation, canon keeper",
        _ => "Unknown sibling",
    }
}

/// Check whether a sibling binary exists on disk.
///
/// Expands `~/` in the path before checking. Returns `true` if the file exists
/// and is a regular file (or symlink resolving to one).
fn binary_exists(raw_path: &str) -> bool {
    let path = expand_tilde(raw_path);
    path.is_file()
}

/// Execute `lightarchitects_discover`.
///
/// Returns a JSON snapshot matching the amendment's response schema:
/// - `platform` — always `"claude-code"`.
/// - `gateway_version` — from `CARGO_PKG_VERSION`.
/// - `core_tools` — list of built-in tool short names (without prefix).
/// - `siblings` — per-sibling map with `enabled`, `status`, `binary_path`,
///   `binary_found`, `role`, `capabilities` (enabled) or `reason` (disabled).
/// - `canon_tools` — canon keeper actions (Layer 3).
/// - `setup_tools` — setup wizard actions.
///
/// The `status` field is:
/// - `"binary_found"` — sibling is enabled and its binary exists on disk.
/// - `"binary_missing"` — sibling is enabled but binary not found at configured path.
/// - `"disabled"` — sibling is not enabled in config.
///
/// # Errors
///
/// Returns [`GatewayError::Json`] if serialization fails (should not happen in practice).
pub fn run(_params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let mut siblings_map = serde_json::Map::new();

    for (name, cfg) in &config.siblings {
        let entry = if name == "laex" {
            // LÆX is always reported as "preview" — Arena routing is disconnected
            // until the Arena binary ships.
            json!({
                "enabled": cfg.enabled,
                "status": "preview",
                "role": sibling_role(name),
                "capabilities": sibling_capabilities(name),
                "note": "Arena actions are not available in this release. They will be enabled when the Arena binary ships.",
            })
        } else if cfg.enabled {
            let found = binary_exists(&cfg.binary);
            let status = if found {
                "binary_found"
            } else {
                "binary_missing"
            };

            let mut obj = json!({
                "enabled": true,
                "status": status,
                "binary_path": cfg.binary,
                "binary_found": found,
                "tool_name": cfg.tool_name,
                "role": sibling_role(name),
                "trust": format!("{:?}", cfg.trust).to_lowercase(),
                "scope": format!("{:?}", cfg.scope).to_lowercase(),
                "capabilities": sibling_capabilities(name),
            });

            if !found {
                obj["hint"] = json!(format!(
                    "Binary not found at '{}'. Build and deploy {name} first, or update config.",
                    cfg.binary
                ));
            }

            obj
        } else {
            json!({
                "enabled": false,
                "status": "disabled",
                "reason": format!(
                    "{name} is not enabled. To enable: edit ~/.lightarchitects/config.toml \
                     and set [siblings.{name}] enabled = true"
                ),
            })
        };

        siblings_map.insert(name.clone(), entry);
    }

    // Short core tool names (strip "lightarchitects_" prefix) for the model.
    let core_short: Vec<&str> = CORE_TOOLS
        .iter()
        .map(|t| t.trim_start_matches("lightarchitects_"))
        .collect();

    let payload = json!({
        "platform": "claude-code",
        "gateway_version": env!("CARGO_PKG_VERSION"),
        "core_tools": core_short,
        "siblings": siblings_map,
        "canon_tools": ["canon_check", "canon_evaluate"],
        "setup_tools": ["initialize"],
    });

    Ok(text_result(serde_json::to_string_pretty(&payload)?))
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_returns_platform_and_version() {
        let cfg = GatewayConfig::default();
        let result = run(json!({}), &cfg).expect("discover run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("claude-code"), "platform field missing");
        assert!(text.contains("gateway_version"), "version field missing");
    }

    #[test]
    fn discover_lists_core_tools() {
        let cfg = GatewayConfig::default();
        let result = run(json!({}), &cfg).expect("discover run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"read\""), "read tool missing");
        assert!(text.contains("\"bash\""), "bash tool missing");
        assert!(text.contains("\"discover\""), "discover tool missing");
    }

    #[test]
    fn discover_lists_all_siblings_from_default_config() {
        let cfg = GatewayConfig::default();
        let result = run(json!({}), &cfg).expect("discover run");
        let text = result["content"][0]["text"].as_str().unwrap();
        for sibling in ["corso", "eva", "soul", "quantum", "seraph", "ayin"] {
            assert!(
                text.contains(sibling),
                "sibling '{sibling}' missing from discover output"
            );
        }
    }

    #[test]
    fn enabled_sibling_has_status_field() {
        let cfg = GatewayConfig::default();
        let result = run(json!({}), &cfg).expect("discover run");
        let parsed: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap())
            .expect("json parse");
        // CORSO is enabled in default config — should have a status.
        let corso = &parsed["siblings"]["corso"];
        assert!(
            corso["status"].is_string(),
            "enabled sibling should have status"
        );
        assert!(
            corso["binary_found"].is_boolean(),
            "enabled sibling should have binary_found"
        );
    }

    #[test]
    fn disabled_sibling_has_reason_field() {
        let cfg = GatewayConfig::default();
        let result = run(json!({}), &cfg).expect("discover run");
        let parsed: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap())
            .expect("json parse");
        // QUANTUM is disabled in default config.
        let quantum = &parsed["siblings"]["quantum"];
        assert_eq!(quantum["enabled"], false, "quantum should be disabled");
        assert!(
            quantum["reason"].is_string(),
            "disabled sibling should have reason"
        );
    }

    #[test]
    fn binary_exists_returns_false_for_nonexistent() {
        assert!(!binary_exists("/nonexistent/path/to/binary"));
    }

    #[test]
    fn sibling_capabilities_returns_nonempty_for_known_siblings() {
        for sib in ["corso", "eva", "soul", "quantum", "seraph", "ayin"] {
            assert!(
                !sibling_capabilities(sib).is_empty(),
                "sibling '{sib}' should have capabilities"
            );
        }
    }
}
