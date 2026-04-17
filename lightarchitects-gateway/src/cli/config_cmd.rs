//! `lightarchitects config` — show resolved gateway configuration.
//!
//! Serializes the full `GatewayConfig` as JSON with sensitive fields redacted.
//! This replaces the CLI's `CliConfig` display with `GatewayConfig` as the
//! single source of truth.

use crate::cli::output::{OutputMode, print_value};
use crate::config::GatewayConfig;
use crate::error::GatewayError;

/// Display the resolved gateway configuration.
///
/// # Errors
///
/// Returns [`GatewayError`] if serialization fails.
pub fn execute(config: &GatewayConfig, mode: OutputMode) -> Result<(), GatewayError> {
    let value = serde_json::to_value(config).map_err(GatewayError::Json)?;
    print_value(mode, &value);
    Ok(())
}
