//! `l-arc status` — MCP binary availability check.

use crate::config::CliConfig;
use crate::output::{OutputMode, print_list};

/// Print the availability of each MCP binary.
pub fn execute(cfg: &CliConfig, mode: OutputMode) {
    print_list(mode, &cfg.status_lines());
}
