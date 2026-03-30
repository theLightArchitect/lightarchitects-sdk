//! `lightarchitects_canon_check` — check a decision against the Light Architects canon.
//!
//! Reads the canon registry file, extracts all ratified canon headers, and returns
//! a structured payload so that models can evaluate a decision against
//! existing precedents.

use std::path::Path;

use serde_json::Value;

use crate::config::{GatewayConfig, expand_tilde};
use crate::core_tools::text_result;
use crate::error::GatewayError;

/// Execute `lightarchitects_canon_check`.
///
/// # Parameters (JSON object)
/// - `decision` (string, required): the decision or proposed action to evaluate.
/// - `verbose` (bool, optional, default `false`): include raw registry content alongside headers.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] when `decision` is absent.
/// Returns [`GatewayError::File`] when the canon registry cannot be read.
pub fn run(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let decision = params["decision"]
        .as_str()
        .ok_or(GatewayError::MissingParam("decision"))?;

    let verbose = params["verbose"].as_bool().unwrap_or(false);
    let registry_path = expand_tilde(&config.canon.registry);
    let (headers, content) = read_canon(&registry_path)?;

    Ok(build_check_payload(decision, verbose, headers, content))
}

/// Read the canon registry and extract all `### Canon` headers.
fn read_canon(path: &Path) -> Result<(Vec<String>, String), GatewayError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| GatewayError::File(format!("{}: {e}", path.display())))?;

    let headers: Vec<String> = content
        .lines()
        .filter(|l| l.starts_with("### Canon"))
        .map(|l| l.trim_start_matches('#').trim().to_owned())
        .collect();

    Ok((headers, content))
}

/// Build the structured payload for a canon check.
fn build_check_payload(
    decision: &str,
    verbose: bool,
    headers: Vec<String>,
    content: String,
) -> Value {
    let canon_count = headers.len();
    let header_list = headers.join("\n");

    let mut summary = format!(
        "Canon check for: \"{decision}\"\n\
         \n\
         {canon_count} ratified canons:\n\
         {header_list}\n\
         \n\
         Evaluate the decision against each canon. \
         Flag conflicts, alignments, or gaps."
    );

    if verbose {
        summary.push_str("\n\n--- Canon Registry ---\n");
        summary.push_str(&content);
    }

    text_result(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write as _;

    #[test]
    fn missing_decision_is_error() {
        let cfg = GatewayConfig::default();
        assert!(run(json!({}), &cfg).is_err());
    }

    #[test]
    fn missing_registry_file_is_error() {
        let mut cfg = GatewayConfig::default();
        cfg.canon.registry = "/nonexistent/canon.md".to_owned();
        assert!(run(json!({"decision": "test"}), &cfg).is_err());
    }

    #[test]
    fn returns_headers_from_registry() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "### Canon I: Builders Cookbook\n### Canon II: Comms\n").expect("write");
        let mut cfg = GatewayConfig::default();
        cfg.canon.registry = tmp.path().to_str().unwrap().to_owned();

        let result = run(json!({"decision": "add new tool"}), &cfg).expect("run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Canon I: Builders Cookbook"));
        assert!(text.contains("Canon II: Comms"));
    }

    #[test]
    fn verbose_includes_raw_content() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "### Canon I: Test\n\nsome raw content").expect("write");
        let mut cfg = GatewayConfig::default();
        cfg.canon.registry = tmp.path().to_str().unwrap().to_owned();

        let result = run(json!({"decision": "x", "verbose": true}), &cfg).expect("run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("some raw content"));
    }
}
