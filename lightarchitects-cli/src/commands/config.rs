//! `lightarchitects config` — show resolved binary-path configuration.

use crate::config::CliConfig;
use crate::output::{OutputMode, print_value};

/// Print the resolved CLI configuration as JSON or human-readable text.
pub fn execute(cfg: &CliConfig, mode: OutputMode) {
    let v = serde_json::json!({
        "soul":    cfg.soul.display().to_string(),
        "corso":   cfg.corso.display().to_string(),
        "eva":     cfg.eva.display().to_string(),
        "quantum": cfg.quantum.display().to_string(),
        "seraph":  cfg.seraph.display().to_string(),
    });
    print_value(mode, &v);
}
