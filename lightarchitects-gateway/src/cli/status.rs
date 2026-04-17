//! `lightarchitects status` — show which MCP binaries are present.
//!
//! Checks binary availability for all configured agents using the paths
//! from `GatewayConfig`. This replaces the CLI's `CliConfig::status_lines()`
//! with `GatewayConfig.agents` as the single source of truth.

use crate::cli::output::{OutputMode, print_value};
use crate::config::GatewayConfig;
use crate::error::GatewayError;

/// Show binary availability for all configured agents.
///
/// # Errors
///
/// Returns [`GatewayError`] if serialization fails (should not happen with `serde_json`).
pub fn execute(config: &GatewayConfig, mode: OutputMode) -> Result<(), GatewayError> {
    let mut lines = Vec::new();

    // Check all agents (including disabled ones) for binary availability.
    let mut agents: Vec<_> = config.agents.iter().collect();
    agents.sort_by_key(|(name, _)| *name);

    for (name, agent_cfg) in &agents {
        let path = agent_cfg.binary_path();
        let present = path.exists();
        let marker = if present { "✓" } else { "✗" };
        let enabled = if agent_cfg.enabled {
            "enabled"
        } else {
            "disabled"
        };
        let mode_str = match agent_cfg.mode {
            lightarchitects::core::handler::DispatchMode::Inline => "inline",
            lightarchitects::core::handler::DispatchMode::Spawner => "spawner",
            lightarchitects::core::handler::DispatchMode::Disabled => "disabled",
        };
        lines.push(format!(
            "{marker} {name:<10} {enabled:<9} {mode_str:<8} {}",
            path.display()
        ));
    }

    match mode {
        OutputMode::Human => {
            for line in &lines {
                println!("{line}");
            }
        }
        OutputMode::Json => {
            let status: Vec<serde_json::Value> = agents
                .iter()
                .map(|(name, agent_cfg)| {
                    let path = agent_cfg.binary_path();
                    serde_json::json!({
                        "name": name,
                        "enabled": agent_cfg.enabled,
                        "mode": match agent_cfg.mode {
                            lightarchitects::core::handler::DispatchMode::Inline => "inline",
                            lightarchitects::core::handler::DispatchMode::Spawner => "spawner",
                            lightarchitects::core::handler::DispatchMode::Disabled => "disabled",
                        },
                        "binary": path.display().to_string(),
                        "present": path.exists(),
                    })
                })
                .collect();
            print_value(mode, &serde_json::json!(status));
        }
    }

    Ok(())
}
