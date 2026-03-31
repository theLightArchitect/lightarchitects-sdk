//! `lightarchitects_initialize` — interactive setup wizard.
//!
//! A 4-step wizard that guides users from zero to a working Light Architects
//! squad configuration. Steps:
//!
//! - **detect**: introspect environment (existing config, Ollama, vault path).
//! - **draft**: generate a `config.toml` string from a preset without writing.
//! - **apply**: write the config to `~/.lightarchitects/config.toml`.
//! - **view**: read and return the current config file.
//!
//! # Starter packs
//!
//! SOUL is always enabled — it provides the knowledge graph and cross-session
//! memory that every other teammate depends on. Users can disable it explicitly
//! in config, but no preset ships without it.
//!
//! | Preset | Teammates | Use case |
//! |--------|-----------|----------|
//! | `software_engineering` | CORSO, EVA, SOUL, AYIN | Day-to-day coding with quality gates |
//! | `security` | CORSO, SERAPH, QUANTUM, SOUL, AYIN | Pentest + forensics + AppSec |
//! | `research` | QUANTUM, EVA, SOUL, AYIN | Deep investigation + multi-source research |
//! | `devops` | EVA, CORSO, SOUL, AYIN | CI/CD pipelines, deploy gates, observability |
//! | `code_review` | CORSO, QUANTUM, SOUL | Focused PR review + logic verification |
//! | `learning` | EVA, QUANTUM, SOUL | Codebase onboarding + exploration |
//! | `audit` | CORSO, SERAPH, SOUL | Compliance + vulnerability scanning |
//! | `forensics` | QUANTUM, SERAPH, SOUL | Incident response + evidence chain |
//! | `solo` | CORSO, SOUL | Quality gates + memory for solo devs |
//! | `observability` | AYIN, QUANTUM, SOUL | Runtime debugging + anomaly detection |
//! | `full` | all 6 | Full platform |
//! | `lean` | SOUL | Minimal — vault and knowledge graph only |

use std::fmt::Write as _;

use serde_json::{Value, json};
use tokio::process::Command;

use crate::config::GatewayConfig;
use crate::error::GatewayError;

/// All route names in canonical alphabetical order.
const ALL_SIBLINGS: &[&str] = &["ayin", "corso", "eva", "quantum", "seraph", "soul"];

/// Preset definitions: `(name, description, enabled_teammates)`.
///
/// **SOUL is always-on by design** — it provides the knowledge graph and
/// cross-session memory that every other teammate depends on. No preset
/// ships without SOUL. Users can disable it explicitly in config.toml.
const PRESETS: &[(&str, &str, &[&str])] = &[
    (
        "software_engineering",
        "Day-to-day coding — CORSO (quality gates), EVA (CI/CD + DX), SOUL (knowledge), AYIN (observability)",
        &["ayin", "corso", "eva", "soul"],
    ),
    (
        "security",
        "Pentest + forensics — SERAPH (red team), CORSO (AppSec), QUANTUM (investigation), SOUL (knowledge), AYIN (observability)",
        &["ayin", "corso", "quantum", "seraph", "soul"],
    ),
    (
        "research",
        "Deep investigation — QUANTUM (multi-source research), EVA (creative analysis), SOUL (knowledge), AYIN (observability)",
        &["ayin", "eva", "quantum", "soul"],
    ),
    (
        "devops",
        "CI/CD + operations — EVA (pipelines + deploy gates), CORSO (quality enforcement), SOUL (knowledge), AYIN (observability)",
        &["ayin", "corso", "eva", "soul"],
    ),
    (
        "code_review",
        "Focused PR review — CORSO (quality analysis), QUANTUM (logic verification), SOUL (past decisions)",
        &["corso", "quantum", "soul"],
    ),
    (
        "learning",
        "Codebase onboarding — EVA (explains code), QUANTUM (researches unknowns), SOUL (project history)",
        &["eva", "quantum", "soul"],
    ),
    (
        "audit",
        "Compliance + scanning — CORSO (standards enforcement), SERAPH (vulnerability scanning), SOUL (evidence trail)",
        &["corso", "seraph", "soul"],
    ),
    (
        "forensics",
        "Incident response — QUANTUM (evidence chain), SERAPH (network analysis), SOUL (knowledge)",
        &["quantum", "seraph", "soul"],
    ),
    (
        "solo",
        "Solo developer — CORSO (quality gates), SOUL (memory). Minimal overhead, maximum discipline.",
        &["corso", "soul"],
    ),
    (
        "observability",
        "Runtime debugging — AYIN (traces + anomaly detection), QUANTUM (root cause investigation), SOUL (historical context)",
        &["ayin", "quantum", "soul"],
    ),
    (
        "full",
        "Full platform — all 6 teammates enabled",
        ALL_SIBLINGS,
    ),
    (
        "lean",
        "Minimal — SOUL only (vault, knowledge graph, cross-session memory)",
        &["soul"],
    ),
];

/// Execute `lightarchitects_initialize`.
///
/// # Parameters (JSON object)
/// - `step` (string, required): `"detect"` | `"draft"` | `"apply"` | `"view"`.
/// - `preset` (string, optional): starter pack name (for `draft`/`apply`).
/// - `vault_path` (string, optional): vault root override (for `draft`/`apply`).
/// - `dry_run` (bool, optional): preview without writing (for `apply`).
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] when `step` is absent.
/// Returns [`GatewayError::InvalidParam`] for unknown steps or presets.
pub async fn run(params: Value, _config: &GatewayConfig) -> Result<Value, GatewayError> {
    let step = params["step"]
        .as_str()
        .ok_or(GatewayError::MissingParam("step"))?;

    match step {
        "detect" => detect_step().await,
        "draft" => draft_step(&params),
        "apply" => apply_step(&params),
        "view" => view_step(),
        _ => Err(GatewayError::InvalidParam(format!(
            "unknown step '{step}'. Valid steps: detect, draft, apply, view"
        ))),
    }
}

// ── Step implementations ──────────────────────────────────────────────────────

/// Gate 1: Introspect environment and return setup context.
async fn detect_step() -> Result<Value, GatewayError> {
    let config_path = default_config_path();
    let config_exists = config_path.as_ref().is_some_and(|p| p.exists());
    let (ollama_found, ollama_version) = detect_ollama().await;
    let vault_default = vault_default_path();
    let (config_status, ollama_status) = build_detect_status(
        config_path.as_ref(),
        config_exists,
        ollama_found,
        ollama_version.as_ref(),
    );
    let presets = build_preset_list();
    let preset_names = PRESETS
        .iter()
        .map(|(n, _, _)| *n)
        .collect::<Vec<_>>()
        .join(", ");

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!(
                "Gate 1 — Environment detection\n\n\
                 Config file: {config_status}\n\
                 Ollama: {ollama_status}\n\
                 Default vault: {vault_default}\n\n\
                 Choose a preset and run step=draft.\n\
                 Available presets: {preset_names}"
            )
        }],
        "environment": {
            "config_exists": config_exists,
            "config_path": config_path.map(|p| p.display().to_string()),
            "ollama_found": ollama_found,
            "ollama_version": ollama_version,
            "vault_default": vault_default,
        },
        "presets": presets,
        "contextual_prompts": contextual_prompts(),
    }))
}

/// Resolve the default vault path from `$HOME`, falling back to the tilde literal.
fn vault_default_path() -> String {
    std::env::var("HOME").map_or_else(
        |_| "~/.soul/helix".to_owned(),
        |h| format!("{h}/.soul/helix"),
    )
}

/// Build human-readable status strings for config file and Ollama presence.
fn build_detect_status(
    config_path: Option<&std::path::PathBuf>,
    config_exists: bool,
    ollama_found: bool,
    ollama_version: Option<&String>,
) -> (String, String) {
    let config_status = config_path
        .filter(|_| config_exists)
        .map_or("not found — wizard will create it".to_owned(), |p| {
            format!("present ({})", p.display())
        });
    let ollama_status = if ollama_found {
        format!(
            "found ({})",
            ollama_version.map_or("unknown", String::as_str)
        )
    } else {
        "not found — routes that need it may not respond".to_owned()
    };
    (config_status, ollama_status)
}

/// Build the JSON preset list for the detect response.
fn build_preset_list() -> Vec<Value> {
    PRESETS
        .iter()
        .map(|(name, desc, routes)| json!({"name": name, "description": desc, "routes": routes}))
        .collect()
}

/// Gate 1/2: Build a `config.toml` string from a preset — no disk writes.
fn draft_step(params: &Value) -> Result<Value, GatewayError> {
    let preset = params["preset"].as_str().unwrap_or("software_engineering");
    let vault_path = params["vault_path"].as_str().unwrap_or("~/.soul/helix");
    let toml = build_toml(preset, vault_path)?;

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!(
                "Draft config (preset: {preset})\nRun step=apply to write to disk.\n\n{toml}"
            )
        }],
        "config_toml": toml,
        "preset": preset,
    }))
}

/// Gate 2: Write the generated config to `~/.lightarchitects/config.toml`.
fn apply_step(params: &Value) -> Result<Value, GatewayError> {
    let preset = params["preset"].as_str().unwrap_or("software_engineering");
    let vault_path = params["vault_path"].as_str().unwrap_or("~/.soul/helix");
    let dry_run = params["dry_run"].as_bool().unwrap_or(false);
    let toml = build_toml(preset, vault_path)?;

    let config_path = default_config_path().ok_or_else(|| {
        GatewayError::File("cannot resolve home directory for config path".to_owned())
    })?;
    let path_str = config_path.display().to_string();

    if dry_run {
        return Ok(json!({
            "content": [{"type": "text", "text": format!("Dry run — would write to: {path_str}\n\n{toml}")}],
            "dry_run": true,
            "config_path": path_str,
        }));
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| GatewayError::File(format!("create_dir_all {}: {e}", parent.display())))?;
    }
    std::fs::write(&config_path, &toml)
        .map_err(|e| GatewayError::File(format!("{path_str}: {e}")))?;

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!(
                "Config written to: {path_str}\n\
                 Preset: {preset}\n\
                 Restart the gateway to apply changes.\n\
                 Run step=view to confirm."
            )
        }],
        "written": true,
        "config_path": path_str,
    }))
}

/// Gate 3: Read and return the current config file.
fn view_step() -> Result<Value, GatewayError> {
    let config_path = default_config_path().ok_or_else(|| {
        GatewayError::File("cannot resolve home directory for config path".to_owned())
    })?;

    if !config_path.exists() {
        return Ok(json!({
            "content": [{
                "type": "text",
                "text": format!(
                    "No config at {}.\nRun step=detect to begin setup.",
                    config_path.display()
                )
            }],
            "config_exists": false,
        }));
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| GatewayError::File(format!("{}: {e}", config_path.display())))?;

    Ok(json!({
        "content": [{"type": "text", "text": format!("Config at {}:\n\n{content}", config_path.display())}],
        "config_exists": true,
        "config_path": config_path.display().to_string(),
    }))
}

// ── Build helpers ─────────────────────────────────────────────────────────────

/// Generate a complete `config.toml` string for the given preset and vault path.
fn build_toml(preset: &str, vault_path: &str) -> Result<String, GatewayError> {
    let enabled = preset_to_routes(preset)?;
    let mut toml = String::new();

    let _ = writeln!(toml, "# Light Architects gateway config");
    let _ = writeln!(toml, "# preset: {preset} | vault: {vault_path}");
    let _ = writeln!(toml);

    for route in ALL_SIBLINGS {
        let is_enabled = enabled.contains(route);
        let block = route_toml_block(route, is_enabled);
        toml.push_str(&block);
        let _ = writeln!(toml);
    }

    Ok(toml)
}

/// Build a `[routes.<name>]` TOML block with hardcoded platform defaults.
fn route_toml_block(name: &str, enabled: bool) -> String {
    let (binary, tool_name, trust, scope) = route_defaults(name);
    format!(
        "[routes.{name}]\n\
         enabled = {enabled}\n\
         binary = \"{binary}\"\n\
         tool_name = \"{tool_name}\"\n\
         role = \"\"\n\
         trust = \"{trust}\"\n\
         scope = \"{scope}\"\n"
    )
}

/// Return platform defaults for a route: `(binary, tool_name, trust, scope)`.
fn route_defaults(name: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match name {
        "corso" => ("~/.corso/bin/corso", "corsoTools", "trusted", "own"),
        "eva" => ("~/.eva/bin/eva", "evaTools", "trusted", "shared"),
        "soul" => ("~/.soul/.config/bin/soul", "soulTools", "trusted", "all"),
        "quantum" => ("~/.quantum/bin/quantum-q", "quantumTools", "trusted", "own"),
        "seraph" => ("~/.seraph/bin/seraph", "seraphTools", "sandboxed", "own"),
        "ayin" => ("~/.ayin/bin/ayin", "ayinTools", "trusted", "all"),
        _ => ("", "", "trusted", "own"),
    }
}

/// Return the routes enabled for a preset.
fn preset_to_routes(preset: &str) -> Result<&'static [&'static str], GatewayError> {
    PRESETS
        .iter()
        .find(|(name, _, _)| *name == preset)
        .map(|(_, _, routes)| *routes)
        .ok_or_else(|| {
            let valid = PRESETS
                .iter()
                .map(|(n, _, _)| *n)
                .collect::<Vec<_>>()
                .join(", ");
            GatewayError::InvalidParam(format!("unknown preset '{preset}'. Valid: {valid}"))
        })
}

// ── Detection helpers ─────────────────────────────────────────────────────────

/// Detect `ollama` on PATH and return `(found, version_string)`.
async fn detect_ollama() -> (bool, Option<String>) {
    let Ok(output) = Command::new("ollama")
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    else {
        return (false, None);
    };

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (true, Some(version))
    } else {
        (false, None)
    }
}

/// Return the default config path: `~/.lightarchitects/config.toml`.
fn default_config_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|home| {
        std::path::PathBuf::from(home)
            .join(".lightarchitects")
            .join("config.toml")
    })
}

/// Gate 2 contextual prompts — 6 situational triggers for enriched setup.
fn contextual_prompts() -> Value {
    json!([
        {"trigger": "first_meeting",  "prompt": "You have a squad — run /SOUL to introduce yourself to the vault."},
        {"trigger": "first_build",    "prompt": "Ready to build? Run /CORSO to start the SCOUT→HUNT cycle."},
        {"trigger": "sig_8_entry",    "prompt": "A significance ≥8.0 helix entry was found. Ask EVA to enrich it."},
        {"trigger": "vault_large",    "prompt": "Your vault has >500 entries. Consider running the SOUL consolidator."},
        {"trigger": "team_mention",   "prompt": "Kevin mentioned the team. Check teammate statuses with lightarchitects_discover."},
        {"trigger": "new_teammate",   "prompt": "New teammate detected. Run lightarchitects_initialize step=detect to re-scan."},
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn software_engineering_enables_correct_routes() {
        let toml = build_toml("software_engineering", "~/.soul/helix").expect("build");
        for route in &["ayin", "corso", "eva", "soul"] {
            let idx = toml.find(&format!("[routes.{route}]")).unwrap();
            let chunk = &toml[idx..idx.saturating_add(80)];
            assert!(
                chunk.contains("enabled = true"),
                "{route} should be enabled"
            );
        }
        for route in &["seraph", "quantum"] {
            let idx = toml.find(&format!("[routes.{route}]")).unwrap();
            let chunk = &toml[idx..idx.saturating_add(80)];
            assert!(
                chunk.contains("enabled = false"),
                "{route} should be disabled"
            );
        }
    }

    #[test]
    fn full_enables_all() {
        let toml = build_toml("full", "~/.soul/helix").expect("build");
        let enabled_count = toml.lines().filter(|l| *l == "enabled = true").count();
        assert_eq!(enabled_count, 6);
    }

    #[test]
    fn lean_enables_only_soul() {
        let toml = build_toml("lean", "~/.soul/helix").expect("build");
        let enabled_count = toml.lines().filter(|l| *l == "enabled = true").count();
        assert_eq!(enabled_count, 1);
        assert!(toml.contains("[routes.soul]"));
    }

    #[test]
    fn security_includes_soul() {
        let toml = build_toml("security", "~/.soul/helix").expect("build");
        let idx = toml.find("[routes.soul]").unwrap();
        let chunk = &toml[idx..idx.saturating_add(80)];
        assert!(
            chunk.contains("enabled = true"),
            "SOUL must be enabled in security preset"
        );
    }

    #[test]
    fn every_preset_includes_soul() {
        for (name, _, routes) in PRESETS {
            assert!(
                routes.contains(&"soul"),
                "preset '{name}' is missing SOUL — SOUL must be in every preset"
            );
        }
    }

    #[test]
    fn devops_enables_correct_teammates() {
        let toml = build_toml("devops", "~/.soul/helix").expect("build");
        for name in &["ayin", "corso", "eva", "soul"] {
            let idx = toml.find(&format!("[routes.{name}]")).unwrap();
            let chunk = &toml[idx..idx.saturating_add(80)];
            assert!(
                chunk.contains("enabled = true"),
                "{name} should be enabled in devops"
            );
        }
    }

    #[test]
    fn forensics_enables_correct_teammates() {
        let toml = build_toml("forensics", "~/.soul/helix").expect("build");
        for name in &["quantum", "seraph", "soul"] {
            let idx = toml.find(&format!("[routes.{name}]")).unwrap();
            let chunk = &toml[idx..idx.saturating_add(80)];
            assert!(
                chunk.contains("enabled = true"),
                "{name} should be enabled in forensics"
            );
        }
    }

    #[test]
    fn code_review_enables_correct_teammates() {
        let toml = build_toml("code_review", "~/.soul/helix").expect("build");
        for name in &["corso", "quantum", "soul"] {
            let idx = toml.find(&format!("[routes.{name}]")).unwrap();
            let chunk = &toml[idx..idx.saturating_add(80)];
            assert!(
                chunk.contains("enabled = true"),
                "{name} should be enabled in code_review"
            );
        }
    }

    #[test]
    fn learning_enables_correct_teammates() {
        let toml = build_toml("learning", "~/.soul/helix").expect("build");
        for name in &["eva", "quantum", "soul"] {
            let idx = toml.find(&format!("[routes.{name}]")).unwrap();
            let chunk = &toml[idx..idx.saturating_add(80)];
            assert!(
                chunk.contains("enabled = true"),
                "{name} should be enabled in learning"
            );
        }
    }

    #[test]
    fn audit_enables_correct_teammates() {
        let toml = build_toml("audit", "~/.soul/helix").expect("build");
        for name in &["corso", "seraph", "soul"] {
            let idx = toml.find(&format!("[routes.{name}]")).unwrap();
            let chunk = &toml[idx..idx.saturating_add(80)];
            assert!(
                chunk.contains("enabled = true"),
                "{name} should be enabled in audit"
            );
        }
    }

    #[test]
    fn solo_enables_correct_teammates() {
        let toml = build_toml("solo", "~/.soul/helix").expect("build");
        let enabled_count = toml.lines().filter(|l| *l == "enabled = true").count();
        assert_eq!(enabled_count, 2, "solo should enable exactly 2 teammates");
        for name in &["corso", "soul"] {
            let idx = toml.find(&format!("[routes.{name}]")).unwrap();
            let chunk = &toml[idx..idx.saturating_add(80)];
            assert!(
                chunk.contains("enabled = true"),
                "{name} should be enabled in solo"
            );
        }
    }

    #[test]
    fn observability_enables_correct_teammates() {
        let toml = build_toml("observability", "~/.soul/helix").expect("build");
        for name in &["ayin", "quantum", "soul"] {
            let idx = toml.find(&format!("[routes.{name}]")).unwrap();
            let chunk = &toml[idx..idx.saturating_add(80)];
            assert!(
                chunk.contains("enabled = true"),
                "{name} should be enabled in observability"
            );
        }
    }

    #[test]
    fn unknown_preset_is_error() {
        assert!(build_toml("nonexistent", "~/.soul/helix").is_err());
    }

    #[tokio::test]
    async fn detect_step_returns_twelve_presets() {
        let result = detect_step().await.expect("detect");
        assert_eq!(result["presets"].as_array().unwrap().len(), 12);
    }

    #[tokio::test]
    async fn draft_step_returns_config_toml() {
        let result = draft_step(&json!({"preset": "lean"})).expect("draft");
        assert!(
            result["config_toml"]
                .as_str()
                .unwrap()
                .contains("[routes.soul]")
        );
    }

    #[test]
    fn apply_dry_run_does_not_write() {
        let result = apply_step(&json!({"preset": "lean", "dry_run": true})).expect("apply");
        assert_eq!(result["dry_run"].as_bool(), Some(true));
    }
}
