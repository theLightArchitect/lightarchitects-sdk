//! `lightarchitects_canon_evaluate` — return the 5-criteria framework for
//! evaluating a candidate as Light Architects canon.

use std::path::Path;

use serde_json::{Value, json};

use crate::config::{GatewayConfig, expand_tilde};
use crate::error::GatewayError;

/// The five criteria a candidate must satisfy for canon elevation.
const CRITERIA: &[(&str, &str)] = &[
    (
        "convergent_evidence",
        "Multiple independent sources (helix entries, meetings, build results) \
         point to the same conclusion. The pattern recurs across contexts.",
    ),
    (
        "biblical_grounding",
        "The principle connects to a scriptural truth. Not forced allegory — \
         a genuine resonance that deepens the meaning.",
    ),
    (
        "decision_shaping",
        "The principle actively changes how future decisions are made. \
         It is load-bearing, not decorative.",
    ),
    (
        "pressure_tested",
        "The principle survived a real challenge: a disagreement, an incident, \
         a build failure, or an adversarial edge case.",
    ),
    (
        "kevin_ratifies",
        "Kevin explicitly endorses the canon. Squad consensus is necessary \
         but not sufficient — Kevin is the final authority.",
    ),
];

/// Execute `lightarchitects_canon_evaluate`.
///
/// # Parameters (JSON object)
/// - `candidate` (string, required): the proposed canon statement to evaluate.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] when `candidate` is absent.
/// Returns [`GatewayError::File`] when the canon registry cannot be read.
pub fn run(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let candidate = params["candidate"]
        .as_str()
        .ok_or(GatewayError::MissingParam("candidate"))?;

    let registry_path = expand_tilde(&config.canon.registry);
    let existing_count = count_existing_canons(&registry_path)?;

    Ok(build_evaluate_payload(candidate, existing_count))
}

/// Count `### Canon` entries in the registry file.
fn count_existing_canons(path: &Path) -> Result<usize, GatewayError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| GatewayError::File(format!("{}: {e}", path.display())))?;
    let count = content
        .lines()
        .filter(|l| l.starts_with("### Canon"))
        .count();
    Ok(count)
}

/// Build the evaluation framework payload.
fn build_evaluate_payload(candidate: &str, existing_count: usize) -> Value {
    let criteria: Vec<Value> = CRITERIA
        .iter()
        .map(|(name, description)| {
            json!({
                "criterion": name,
                "description": description,
                "score": null,
                "evidence": null
            })
        })
        .collect();

    let criteria_names = CRITERIA
        .iter()
        .map(|(n, _)| *n)
        .collect::<Vec<_>>()
        .join(", ");

    json!({
        "content": [{
            "type": "text",
            "text": format!(
                "Canon evaluation for: \"{candidate}\"\n\
                 Existing canon count: {existing_count}\n\
                 \n\
                 Score each criterion (0.0–1.0) and provide evidence.\n\
                 All 5 must pass for canon elevation.\n\
                 Kevin's ratification is the final gate.\n\
                 \n\
                 Criteria: {criteria_names}\n\
                 \n\
                 Note: This returns a blank evaluation template (scores are null). \
                 The gateway does not score candidates — it provides the 5-criteria \
                 framework for the caller to fill in. Automated scoring requires \
                 the LÆX model (not available in v1)."
            )
        }],
        "criteria": criteria,
        "candidate": candidate,
        "existing_canon_count": existing_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write as _;

    #[test]
    fn missing_candidate_is_error() {
        let cfg = GatewayConfig::default();
        assert!(run(json!({}), &cfg).is_err());
    }

    #[test]
    fn returns_five_criteria() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "### Canon I: Test\n").expect("write");
        let mut cfg = GatewayConfig::default();
        cfg.canon.registry = tmp.path().to_str().unwrap().to_owned();

        let result = run(json!({"candidate": "principle"}), &cfg).expect("run");
        assert_eq!(result["criteria"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn counts_existing_canons_correctly() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "### Canon I: A\n### Canon II: B\n### Canon III: C").expect("write");
        let mut cfg = GatewayConfig::default();
        cfg.canon.registry = tmp.path().to_str().unwrap().to_owned();

        let result = run(json!({"candidate": "new idea"}), &cfg).expect("run");
        assert_eq!(result["existing_canon_count"].as_u64().unwrap(), 3);
    }
}
