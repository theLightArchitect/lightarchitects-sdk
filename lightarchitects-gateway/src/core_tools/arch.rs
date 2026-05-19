//! Architecture intelligence MCP tools — `lightarchitects_arch_*`.
//!
//! Four tools bridge the `lightarchitects-arch` crate into the MCP sibling
//! protocol. Each tool enforces the M6 per-sibling capability check: the
//! caller must supply `sibling_id`; the gateway derives the path allowlist
//! from `$HOME` (server-side) and never accepts it from the caller.
//!
//! Tools:
//! - `lightarchitects_arch_extract` — extract `ArchModel` from a project root.
//! - `lightarchitects_arch_verify`  — diff planned vs current model.
//! - `lightarchitects_arch_render`  — render model to a diagram format.
//! - `lightarchitects_arch_emit`    — emit full package (all formats).

use lightarchitects_arch::{
    ArchModel, Severity,
    emitter::{emit_d2, emit_html, emit_likec4, emit_markdown, emit_mermaid},
    extractor::{ExtractorConfig, walk_and_extract},
    security::path::canonicalize_and_check,
    verifier,
};
use serde_json::Value;
use std::path::PathBuf;

use crate::config::GatewayConfig;
use crate::core_tools::text_result;
use crate::error::GatewayError;

/// `lightarchitects_arch_extract` — extract an `ArchModel` from a project root.
///
/// # Parameters
/// - `project_root` (string, required): absolute path to analyse.
/// - `sibling_id` (string, optional): calling sibling identity for audit log.
///
/// # Errors
/// Returns [`GatewayError::MissingParam`] when `project_root` is absent.
/// Returns [`GatewayError::Security`] when path fails M6 allowlist check.
pub fn run_extract(params: Value, _config: &GatewayConfig) -> Result<Value, GatewayError> {
    let root_str = params["project_root"]
        .as_str()
        .ok_or(GatewayError::MissingParam("project_root"))?;

    let sibling = params["sibling_id"].as_str().unwrap_or("unknown");
    let root = validate_root(root_str)?;

    tracing::info!(sibling_id = sibling, project_root = %root.display(), "arch_extract");

    let facts = walk_and_extract(&root, &ExtractorConfig::default())
        .map_err(|e| GatewayError::File(format!("extract error: {e}")))?;

    let mut model = ArchModel::new(root.display().to_string());
    model.nodes = facts.nodes;
    model.relations = facts.relations;

    let model_json = serde_json::to_string_pretty(&model)
        .map_err(|e| GatewayError::File(format!("serialize error: {e}")))?;

    Ok(text_result(format!(
        "Extracted {} nodes, {} relations from {}\nWarnings: {}\n\nModel:\n{}",
        model.nodes.len(),
        model.relations.len(),
        root.display(),
        facts.warnings.len(),
        model_json,
    )))
}

/// `lightarchitects_arch_verify` — diff a planned model against current source.
///
/// # Parameters
/// - `planned` (object, required): JSON-serialised `ArchModel` baseline.
/// - `project_root` (string, required): path to project (current model extracted live).
/// - `blocking_threshold` (string, optional): "info"|"low"|"medium"|"high"|"critical".
/// - `sibling_id` (string, optional): caller identity for audit log.
///
/// # Errors
/// Returns [`GatewayError::MissingParam`] when `planned` or `project_root` are absent or invalid.
/// Returns [`GatewayError::File`] when extraction fails.
pub fn run_verify(params: Value, _config: &GatewayConfig) -> Result<Value, GatewayError> {
    let planned: ArchModel = serde_json::from_value(params["planned"].clone())
        .map_err(|_e| GatewayError::MissingParam("planned — invalid ArchModel JSON"))?;

    let root_str = params["project_root"]
        .as_str()
        .ok_or(GatewayError::MissingParam("project_root"))?;

    let sibling = params["sibling_id"].as_str().unwrap_or("unknown");
    let root = validate_root(root_str)?;

    let threshold = parse_threshold(params["blocking_threshold"].as_str().unwrap_or("high"));

    tracing::info!(sibling_id = sibling, project_root = %root.display(), "arch_verify");

    let facts = walk_and_extract(&root, &ExtractorConfig::default())
        .map_err(|e| GatewayError::File(format!("extract error: {e}")))?;
    let mut current = ArchModel::new(root.display().to_string());
    current.nodes = facts.nodes;
    current.relations = facts.relations;

    let result = verifier::run(&planned, &current, threshold);

    let status = if result.has_blocking {
        "BLOCKING"
    } else {
        "CLEAN"
    };

    let findings_text = result
        .findings
        .iter()
        .map(|f| {
            format!(
                "[{:?}/{:?}] {} — {}",
                f.severity, f.class, f.node_id, f.description
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(text_result(format!(
        "arch_verify: {status}\n\
         Findings: {} | Duplicates dropped: {} | Capped: {}\n\
         \n\
         {findings_text}",
        result.findings.len(),
        result.duplicates_dropped,
        result.capped_dropped,
    )))
}

/// `lightarchitects_arch_render` — render an `ArchModel` to a diagram format.
///
/// # Parameters
/// - `model` (object, required): JSON-serialised `ArchModel`.
/// - `format` (string, required): "mermaid"|"d2"|"likec4"|"markdown"|"html".
/// - `sibling_id` (string, optional): caller identity for audit log.
///
/// # Errors
/// Returns [`GatewayError::MissingParam`] when `model` or `format` are absent or invalid.
/// Returns [`GatewayError::File`] when rendering fails.
pub fn run_render(params: Value, _config: &GatewayConfig) -> Result<Value, GatewayError> {
    let model: ArchModel = serde_json::from_value(params["model"].clone())
        .map_err(|_| GatewayError::MissingParam("model — invalid ArchModel JSON"))?;

    let format = params["format"]
        .as_str()
        .ok_or(GatewayError::MissingParam("format"))?;

    let sibling = params["sibling_id"].as_str().unwrap_or("unknown");
    tracing::info!(sibling_id = sibling, format, "arch_render");

    let output = dispatch_render(&model, format)
        .map_err(|e| GatewayError::File(format!("render error: {e}")))?;

    Ok(text_result(output))
}

/// `lightarchitects_arch_emit` — extract and emit full package from a project root.
///
/// # Parameters
/// - `project_root` (string, required): absolute path to analyse.
/// - `sibling_id` (string, optional): caller identity for audit log.
///
/// # Errors
/// Returns [`GatewayError::MissingParam`] when `project_root` is absent.
/// Returns [`GatewayError::File`] when extraction fails.
pub fn run_emit(params: Value, _config: &GatewayConfig) -> Result<Value, GatewayError> {
    let root_str = params["project_root"]
        .as_str()
        .ok_or(GatewayError::MissingParam("project_root"))?;

    let sibling = params["sibling_id"].as_str().unwrap_or("unknown");
    let root = validate_root(root_str)?;

    tracing::info!(sibling_id = sibling, project_root = %root.display(), "arch_emit");

    let facts = walk_and_extract(&root, &ExtractorConfig::default())
        .map_err(|e| GatewayError::File(format!("extract error: {e}")))?;
    let mut model = ArchModel::new(root.display().to_string());
    model.nodes = facts.nodes;
    model.relations = facts.relations;

    let formats = ["mermaid", "d2", "likec4", "markdown", "html"];
    let mut parts = vec![format!(
        "arch_emit: {} nodes, {} relations from {}",
        model.nodes.len(),
        model.relations.len(),
        root.display()
    )];

    for fmt in &formats {
        let result = dispatch_render(&model, fmt);
        let header = format!("\n--- {fmt} ---");
        match result {
            Ok(text) => {
                parts.push(header);
                // Truncate large outputs in MCP response (full output via HTTP route).
                let preview = if text.len() > 2_000 {
                    format!(
                        "{}… [truncated, {} bytes total]",
                        &text[..2_000],
                        text.len()
                    )
                } else {
                    text
                };
                parts.push(preview);
            }
            Err(e) => {
                parts.push(header);
                parts.push(format!("ERROR: {e}"));
            }
        }
    }

    Ok(text_result(parts.join("\n")))
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn validate_root(root_str: &str) -> Result<PathBuf, GatewayError> {
    let home = std::env::var("HOME").map_err(|_| {
        GatewayError::File("$HOME not set — M6 allowlist cannot be constructed".into())
    })?;
    let allowed = [PathBuf::from(&home)];
    canonicalize_and_check(std::path::Path::new(root_str), &allowed)
        .map_err(|e| GatewayError::File(format!("M6 allowlist rejection: {e}")))
}

fn parse_threshold(s: &str) -> Severity {
    match s {
        "info" => Severity::Info,
        "low" => Severity::Low,
        "medium" => Severity::Medium,
        "critical" => Severity::Critical,
        _ => Severity::High,
    }
}

fn dispatch_render(model: &ArchModel, format: &str) -> Result<String, String> {
    match format {
        "mermaid" => emit_mermaid(model).map_err(|e| e.to_string()),
        "d2" => emit_d2(model).map_err(|e| e.to_string()),
        "likec4" => emit_likec4(model).map_err(|e| e.to_string()),
        "markdown" => emit_markdown(model, None).map_err(|e| e.to_string()),
        "html" => emit_html(model, None, false).map_err(|e| e.to_string()),
        other => Err(format!(
            "unknown format '{other}'; valid: mermaid|d2|likec4|markdown|html"
        )),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::config::GatewayConfig;

    fn dummy_config() -> GatewayConfig {
        GatewayConfig::default()
    }

    #[test]
    fn extract_missing_param() {
        let result = run_extract(serde_json::json!({}), &dummy_config());
        assert!(matches!(result, Err(GatewayError::MissingParam(_))));
    }

    #[test]
    fn verify_missing_project_root() {
        let result = run_verify(serde_json::json!({"planned": {}}), &dummy_config());
        // "planned" is an invalid ArchModel, so it will fail on deserialization
        assert!(result.is_err());
    }

    #[test]
    fn render_missing_format() {
        let model = ArchModel::new("test");
        let result = run_render(
            serde_json::json!({"model": serde_json::to_value(&model).unwrap()}),
            &dummy_config(),
        );
        assert!(matches!(result, Err(GatewayError::MissingParam(_))));
    }

    #[test]
    fn render_known_format_succeeds() {
        let model = ArchModel::new("test");
        let result = run_render(
            serde_json::json!({
                "model": serde_json::to_value(&model).unwrap(),
                "format": "mermaid"
            }),
            &dummy_config(),
        );
        assert!(result.is_ok());
    }
}
