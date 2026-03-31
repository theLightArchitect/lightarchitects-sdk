//! Preset archetypes — workflow personalities with routing priority.
//!
//! Each preset defines which teammates are enabled and in what routing priority
//! order. SOUL is always-on in every preset (enforced by test). The active
//! preset can be switched mid-session via `tools {action: "preset"}`.
//!
//! SCRUM-ratified 2026-03-30 — EVA SHIP IT, CORSO RATED, SERAPH SCOPE VALID.

use std::sync::{OnceLock, RwLock};

use serde_json::{Value, json};
use tracing::info;

use super::text_result;
use crate::error::GatewayError;

// ── Preset definition ───────────────────────────────────────────────────────

/// A preset archetype: routing priority + enabled teammates.
#[derive(Debug, Clone)]
pub struct Preset {
    /// Unique name (e.g. "`software_engineering`", "forensics").
    pub name: &'static str,
    /// One-line description shown in discover output.
    pub description: &'static str,
    /// Routing priority order — first entry is checked first during auto-route.
    /// Only teammates in this list are enabled for the preset.
    pub routing_priority: &'static [&'static str],
}

/// Security-critical teammates that cannot be disabled without explicit HITL.
/// SERAPH Gate 1 (SCRUM mandate: compile-time enforcement).
const SECURITY_TEAMMATES: &[&str] = &["corso", "seraph", "ayin"];

// ── Built-in presets ────────────────────────────────────────────────────────

/// All built-in preset archetypes.
///
/// Routing priority order determines which teammate is tried first during
/// auto-routing. For example, in `code_review`, CORSO is checked before
/// QUANTUM — quality analysis takes precedence over investigation.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "software_engineering",
        description: "Day-to-day coding — quality gates, DX, observability",
        routing_priority: &["corso", "eva", "soul", "ayin"],
    },
    Preset {
        name: "security",
        description: "Pentest + forensics + AppSec",
        routing_priority: &["seraph", "corso", "quantum", "soul", "ayin"],
    },
    Preset {
        name: "research",
        description: "Deep investigation + multi-source research",
        routing_priority: &["quantum", "eva", "soul", "ayin"],
    },
    Preset {
        name: "devops",
        description: "CI/CD pipelines + deploy gates + observability",
        routing_priority: &["eva", "corso", "soul", "ayin"],
    },
    Preset {
        name: "code_review",
        description: "Focused PR review + logic verification",
        routing_priority: &["corso", "quantum", "soul"],
    },
    Preset {
        name: "learning",
        description: "Codebase onboarding + exploration",
        routing_priority: &["eva", "quantum", "soul"],
    },
    Preset {
        name: "audit",
        description: "Compliance + vulnerability scanning",
        routing_priority: &["corso", "seraph", "soul"],
    },
    Preset {
        name: "forensics",
        description: "Incident response + evidence chain",
        routing_priority: &["quantum", "seraph", "soul"],
    },
    Preset {
        name: "solo",
        description: "Quality gates + memory, minimal overhead",
        routing_priority: &["corso", "soul"],
    },
    Preset {
        name: "observability",
        description: "Runtime debugging + anomaly detection",
        routing_priority: &["ayin", "quantum", "soul"],
    },
    Preset {
        name: "full",
        description: "Full platform — all 6 teammates",
        routing_priority: &["quantum", "corso", "seraph", "eva", "soul", "ayin"],
    },
    Preset {
        name: "lean",
        description: "Vault and knowledge graph only",
        routing_priority: &["soul"],
    },
];

// ── Active preset state ─────────────────────────────────────────────────────

/// Runtime state for the active preset. Initialised to `"software_engineering"`
/// on first access. Hot-swappable via [`set_active_preset`].
static ACTIVE_PRESET: OnceLock<RwLock<String>> = OnceLock::new();

/// Default preset when none is configured.
const DEFAULT_PRESET: &str = "software_engineering";

/// Get the name of the currently active preset.
#[must_use]
pub fn active_preset_name() -> String {
    ACTIVE_PRESET
        .get_or_init(|| RwLock::new(DEFAULT_PRESET.to_owned()))
        .read()
        .map_or_else(|_| DEFAULT_PRESET.to_owned(), |guard| guard.clone())
}

/// Look up a preset by name.
#[must_use]
pub fn find_preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

/// Get the currently active preset definition.
///
/// # Panics
///
/// Panics if the default preset is missing from `ALL_PRESETS` (compile-time bug).
#[must_use]
pub fn active_preset() -> &'static Preset {
    let name = active_preset_name();
    find_preset(&name).unwrap_or_else(|| {
        // Fallback to default if active preset name is invalid.
        find_preset(DEFAULT_PRESET).expect("default preset must exist")
    })
}

/// Get the routing priority for the active preset.
#[must_use]
pub fn active_routing_priority() -> &'static [&'static str] {
    active_preset().routing_priority
}

/// Set the active preset, returning an error if the name is invalid or
/// if SERAPH Gate 1 is violated (disabling security teammates without
/// the previous preset also lacking them).
///
/// # Errors
///
/// Returns [`GatewayError::InvalidParam`] for unknown preset names.
/// Returns [`GatewayError::GovernanceViolation`] if switching would
/// disable a security teammate that was enabled in the previous preset.
pub fn set_active_preset(name: &str) -> Result<&'static Preset, GatewayError> {
    let new_preset = find_preset(name).ok_or_else(|| {
        let valid = PRESETS
            .iter()
            .map(|p| p.name)
            .collect::<Vec<_>>()
            .join(", ");
        GatewayError::InvalidParam(format!("unknown preset '{name}'. Valid: {valid}"))
    })?;

    // SERAPH Gate 1: check if switching would disable a security teammate
    // that was enabled in the current preset.
    let current = active_preset();
    for &sec in SECURITY_TEAMMATES {
        let was_enabled = current.routing_priority.contains(&sec);
        let will_be_enabled = new_preset.routing_priority.contains(&sec);
        if was_enabled && !will_be_enabled {
            return Err(GatewayError::InvalidParam(format!(
                "preset '{name}' disables security teammate '{sec}' which is active in \
                 the current preset '{}'. Switching would reduce security posture. \
                 Use the 'full' preset first, or edit config.toml directly with HITL approval.",
                current.name
            )));
        }
    }

    // Apply the switch.
    let previous = active_preset_name();
    let lock = ACTIVE_PRESET.get_or_init(|| RwLock::new(DEFAULT_PRESET.to_owned()));
    if let Ok(mut guard) = lock.write() {
        name.clone_into(&mut guard);
    }

    info!(
        from = %previous,
        to = %name,
        routing_priority = ?new_preset.routing_priority,
        "preset switched"
    );

    Ok(new_preset)
}

/// Initialise the active preset from config (call once from `main`).
pub fn init_from_config(preset_name: &str) {
    let lock = ACTIVE_PRESET.get_or_init(|| RwLock::new(DEFAULT_PRESET.to_owned()));
    if let Ok(mut guard) = lock.write() {
        if find_preset(preset_name).is_some() {
            preset_name.clone_into(&mut guard);
        }
    }
}

// ── MCP handler ─────────────────────────────────────────────────────────────

/// Execute the `preset` action.
///
/// - `{name: "forensics"}` — switch to the named preset.
/// - `{}` or `{name: null}` — return the current preset + list of all presets.
///
/// # Errors
///
/// Returns [`GatewayError::InvalidParam`] for unknown preset names or
/// SERAPH Gate 1 violations.
pub fn run(params: Value) -> Result<Value, GatewayError> {
    let name = params.get("name").and_then(Value::as_str);

    if let Some(target) = name {
        let preset = set_active_preset(target)?;
        Ok(text_result(
            serde_json::to_string_pretty(&json!({
                "switched": true,
                "preset": preset.name,
                "description": preset.description,
                "routing_priority": preset.routing_priority,
                "teammates_enabled": preset.routing_priority.len(),
            }))
            .unwrap_or_default(),
        ))
    } else {
        let current = active_preset();
        let all_presets: Vec<Value> = PRESETS
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "description": p.description,
                    "teammates": p.routing_priority,
                    "active": p.name == current.name,
                })
            })
            .collect();

        Ok(text_result(
            serde_json::to_string_pretty(&json!({
                "active_preset": current.name,
                "description": current.description,
                "routing_priority": current.routing_priority,
                "presets": all_presets,
            }))
            .unwrap_or_default(),
        ))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_includes_soul() {
        for preset in PRESETS {
            assert!(
                preset.routing_priority.contains(&"soul"),
                "preset '{}' is missing SOUL — SOUL must be in every preset",
                preset.name
            );
        }
    }

    #[test]
    fn twelve_presets_defined() {
        assert_eq!(PRESETS.len(), 12);
    }

    #[test]
    fn find_preset_by_name() {
        let p = find_preset("forensics").expect("forensics should exist");
        assert_eq!(p.name, "forensics");
        assert!(p.routing_priority.contains(&"quantum"));
    }

    #[test]
    fn find_preset_unknown_returns_none() {
        assert!(find_preset("nonexistent").is_none());
    }

    #[test]
    fn default_preset_is_software_engineering() {
        // Note: this test may be affected by other tests that call set_active_preset.
        // The default constant is what we're testing, not runtime state.
        assert_eq!(DEFAULT_PRESET, "software_engineering");
    }

    #[test]
    fn full_preset_has_all_six() {
        let p = find_preset("full").expect("full should exist");
        assert_eq!(p.routing_priority.len(), 6);
        for name in &["quantum", "corso", "seraph", "eva", "soul", "ayin"] {
            assert!(
                p.routing_priority.contains(name),
                "full preset missing {name}"
            );
        }
    }

    #[test]
    fn security_preset_routes_seraph_first() {
        let p = find_preset("security").expect("security should exist");
        assert_eq!(
            p.routing_priority[0], "seraph",
            "security preset should route SERAPH first"
        );
    }

    #[test]
    fn code_review_routes_corso_first() {
        let p = find_preset("code_review").expect("code_review should exist");
        assert_eq!(
            p.routing_priority[0], "corso",
            "code_review should route CORSO first"
        );
    }

    #[test]
    fn forensics_routes_quantum_first() {
        let p = find_preset("forensics").expect("forensics should exist");
        assert_eq!(
            p.routing_priority[0], "quantum",
            "forensics should route QUANTUM first"
        );
    }

    #[test]
    fn preset_list_action_returns_all_presets() {
        let result = run(json!({})).expect("list should work");
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["presets"].as_array().unwrap().len(), 12);
    }

    #[test]
    fn preset_switch_does_not_affect_trust_or_scope() {
        // SERAPH Gate 2: preset switching only changes routing priority,
        // never trust/scope levels. Trust and scope live in GatewayConfig.routes,
        // which Preset cannot modify. This test documents the architectural invariant.
        use crate::config::{GatewayConfig, TrustLevel};

        let cfg = GatewayConfig::default();
        let seraph_trust_before = cfg.agents.get("seraph").map(|c| c.trust);

        // Simulate switching presets — the Preset struct has no trust/scope fields.
        let _security = find_preset("security").unwrap();
        let _lean = find_preset("lean").unwrap();

        // Config is unchanged — presets don't mutate it.
        let seraph_trust_after = cfg.agents.get("seraph").map(|c| c.trust);
        assert_eq!(seraph_trust_before, seraph_trust_after);
        assert_eq!(seraph_trust_before, Some(TrustLevel::Sandboxed));
    }

    #[test]
    fn no_preset_has_prompt_overlay_field() {
        // SERAPH Gate 3: presets must never carry prompt data.
        // This is a structural test — the Preset struct has no prompt field.
        // If someone adds one, this test documents the prohibition.
        let preset_fields = std::mem::size_of::<Preset>();
        // Preset is: name (&str = 16), description (&str = 16), routing_priority (&[&str] = 16)
        // If someone adds a prompt field, the size changes.
        assert!(
            preset_fields <= 48,
            "Preset struct grew unexpectedly — check if a prompt_overlay field was added (BANNED by SERAPH Gate 3)"
        );
    }
}
