//! Convergence queries — N-way `SharedExperience` retrieval.
//!
//! Dedicated API for querying convergence points between helixes.
//! This is the data contract for visualization layers (e.g., Three.js arc rendering).

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::db::{HelixDb, HelixDbError};
use crate::types::DiscoveryMethod;

// ============================================================================
// Types
// ============================================================================

/// Parameters for a convergence query.
#[derive(Debug, Clone)]
pub struct ConvergenceParams {
    /// Filter to specific helix owners (empty = all).
    pub helix_ids: Vec<String>,
    /// Minimum weight threshold.
    pub min_weight: f64,
    /// Minimum participant count.
    pub min_participants: usize,
    /// Maximum results.
    pub limit: u32,
}

impl Default for ConvergenceParams {
    fn default() -> Self {
        Self {
            helix_ids: Vec::new(),
            min_weight: 0.0,
            min_participants: 2,
            limit: 50,
        }
    }
}

/// A convergence result — a `SharedExperience` with its participants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergenceResult {
    /// `SharedExperience` node ID.
    pub shared_experience_id: String,
    /// Convergence weight.
    pub weight: f64,
    /// How this convergence was discovered.
    pub discovered_by: DiscoveryMethod,
    /// Flat participant list (compatible with graph layout engines).
    pub participants: Vec<ConvergenceParticipant>,
}

/// A participant in a convergence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergenceParticipant {
    /// Step ID.
    pub step_id: String,
    /// Helix ID the step belongs to.
    pub helix_id: String,
    /// Helix owner (sibling name).
    pub helix_owner: String,
    /// Step title.
    pub title: String,
}

// ============================================================================
// Query
// ============================================================================

/// Query convergences across helixes.
///
/// # Errors
///
/// Returns `HelixDbError` if the Cypher query fails.
#[instrument(skip(db), fields(
    helix_count = params.helix_ids.len(),
    min_weight = params.min_weight,
    limit = params.limit
))]
pub async fn query_convergences(
    db: &dyn HelixDb,
    params: &ConvergenceParams,
) -> Result<Vec<ConvergenceResult>, HelixDbError> {
    let helix_filter = if params.helix_ids.is_empty() {
        String::new()
    } else {
        "AND s.helix_id IN $helix_ids ".to_owned()
    };

    let limit = params.limit;
    let cypher = format!(
        "MATCH (se:SharedExperience) \
         WHERE se.weight >= $min_weight \
         AND se.participant_count >= $min_p \
         MATCH (s:Step)-[:PARTICIPATES_IN]->(se) \
         {helix_filter}\
         WITH se, collect({{ \
           step_id: s.id, helix_id: s.helix_id, \
           title: s.title \
         }}) AS parts \
         MATCH (h:Helix)-[:HAS_STEP]->(ps:Step) \
         WHERE ps.id IN [p IN parts | p.step_id] \
         WITH se, parts, collect(DISTINCT {{ step_id: ps.id, owner: h.owner }}) AS owners \
         RETURN se.id AS se_id, se.weight AS weight, \
                se.discovered_by AS discovered_by, \
                parts, owners \
         ORDER BY se.weight DESC \
         LIMIT {limit}"
    );

    let min_p_i64 = i64::try_from(params.min_participants).unwrap_or(i64::MAX);
    let mut cypher_params = std::collections::BTreeMap::new();
    cypher_params.insert("min_weight".into(), serde_json::json!(params.min_weight));
    cypher_params.insert("min_p".into(), serde_json::json!(min_p_i64));
    if !params.helix_ids.is_empty() {
        cypher_params.insert("helix_ids".into(), serde_json::json!(&params.helix_ids));
    }

    let records = db
        .execute_cypher_with_params(&cypher, cypher_params)
        .await?;

    Ok(parse_convergence_records(&records))
}

/// Parse graph-engine records into convergence results.
fn parse_convergence_records(records: &[crate::graph::Record]) -> Vec<ConvergenceResult> {
    records
        .iter()
        .map(|record| {
            let discovered_by_str = extract_string(record, "discovered_by").unwrap_or_default();
            ConvergenceResult {
                shared_experience_id: extract_string(record, "se_id").unwrap_or_default(),
                weight: extract_f64(record, "weight").unwrap_or(0.0),
                discovered_by: parse_discovery_method(&discovered_by_str),
                participants: Vec::new(), // Populated via separate per-SE query if needed
            }
        })
        .collect()
}

fn parse_discovery_method(s: &str) -> DiscoveryMethod {
    match s {
        "Louvain" => DiscoveryMethod::Louvain,
        "EmbeddingSimilarity" => DiscoveryMethod::EmbeddingSimilarity,
        _ => DiscoveryMethod::Explicit,
    }
}

/// Extract a string field from a graph-engine Record.
fn extract_string(record: &crate::graph::Record, key: &str) -> Option<String> {
    record
        .fields
        .get(key)
        .and_then(|v| v.as_str().map(String::from))
}

/// Extract a float field from a graph-engine Record.
fn extract_f64(record: &crate::graph::Record, key: &str) -> Option<f64> {
    record.fields.get(key).and_then(serde_json::Value::as_f64)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_default_params() {
        let params = ConvergenceParams::default();
        assert!(params.helix_ids.is_empty());
        assert!((params.min_weight - 0.0).abs() < f64::EPSILON);
        assert_eq!(params.min_participants, 2);
        assert_eq!(params.limit, 50);
    }

    #[test]
    fn test_parse_discovery_method() {
        assert_eq!(
            parse_discovery_method("Explicit"),
            DiscoveryMethod::Explicit
        );
        assert_eq!(parse_discovery_method("Louvain"), DiscoveryMethod::Louvain);
        assert_eq!(
            parse_discovery_method("EmbeddingSimilarity"),
            DiscoveryMethod::EmbeddingSimilarity
        );
        assert_eq!(parse_discovery_method("unknown"), DiscoveryMethod::Explicit);
    }

    #[test]
    fn test_convergence_result_serde() {
        let result = ConvergenceResult {
            shared_experience_id: "se-1".into(),
            weight: 0.85,
            discovered_by: DiscoveryMethod::Louvain,
            participants: vec![ConvergenceParticipant {
                step_id: "s1".into(),
                helix_id: "h1".into(),
                helix_owner: "eva".into(),
                title: "Test Step".into(),
            }],
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(json.contains("se-1"));
        assert!(json.contains("0.85"));
    }
}
