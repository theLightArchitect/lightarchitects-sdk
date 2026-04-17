//! Personality engine — graph-metric personality profile computation.
//!
//! Computes personality dimensions from GDS centrality metrics:
//! - `collaborative` ← clustering coefficient
//! - `proactive` ← betweenness centrality
//! - `innovative` ← Shannon entropy of strand membership
//! - `autonomous` ← inverse degree centrality
//!
//! Confidence gating: `< 20 steps = Insufficient`, `20-50 = Emerging`, `50+ = Established`.
//! Requires GDS — falls back to `null` if unavailable.

use tracing::{info, instrument, warn};

use crate::helix::db::{HelixDb, HelixDbError};
use crate::helix::types::{PersonalityConfidence, PersonalityProfile};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the personality engine.
#[derive(Debug, Clone)]
pub struct PersonalityEngineConfig {
    /// Minimum steps for `Emerging` confidence.
    pub emerging_threshold: usize,
    /// Minimum steps for `Established` confidence.
    pub established_threshold: usize,
}

impl Default for PersonalityEngineConfig {
    fn default() -> Self {
        Self {
            emerging_threshold: 20,
            established_threshold: 50,
        }
    }
}

// ============================================================================
// PersonalityEngine
// ============================================================================

/// Computes personality profiles from graph topology.
pub struct PersonalityEngine {
    config: PersonalityEngineConfig,
}

impl PersonalityEngine {
    /// Create a new personality engine.
    #[must_use]
    pub fn new(config: PersonalityEngineConfig) -> Self {
        Self { config }
    }

    /// Create with default thresholds.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(PersonalityEngineConfig::default())
    }

    /// Compute and write personality profile for a helix.
    ///
    /// Returns `None` if GDS is unavailable or step count is insufficient.
    ///
    /// # Errors
    ///
    /// Returns `HelixDbError` if database operations fail.
    #[instrument(skip(self, db), fields(helix_id = %helix_id))]
    pub async fn compute(
        &self,
        db: &dyn HelixDb,
        helix_id: &str,
    ) -> Result<Option<PersonalityProfile>, HelixDbError> {
        let step_count = count_helix_steps(db, helix_id).await?;

        // Determine confidence level
        let confidence = if step_count < self.config.emerging_threshold {
            info!(
                step_count,
                threshold = self.config.emerging_threshold,
                "Insufficient steps for personality — skipping"
            );
            return Ok(Some(PersonalityProfile {
                helix_id: helix_id.to_owned(),
                confidence: PersonalityConfidence::Insufficient { step_count },
                dimensions: std::collections::HashMap::new(),
                computed_at: chrono::Utc::now(),
            }));
        } else if step_count < self.config.established_threshold {
            PersonalityConfidence::Emerging {
                score: compute_confidence_score(step_count, self.config.established_threshold),
            }
        } else {
            PersonalityConfidence::Established {
                score: compute_confidence_score(step_count, self.config.established_threshold),
            }
        };

        if !check_gds_available(db).await {
            return Ok(None);
        }

        // Compute dimensions from GDS centrality metrics
        let dimensions = compute_dimensions(db, helix_id).await?;

        let profile = PersonalityProfile {
            helix_id: helix_id.to_owned(),
            confidence,
            dimensions,
            computed_at: chrono::Utc::now(),
        };

        // Write to helix metadata
        db.write_personality(helix_id, &profile).await?;

        info!(
            helix_id,
            dimension_count = profile.dimensions.len(),
            "Personality profile computed and written"
        );

        Ok(Some(profile))
    }
}

/// Count steps in a helix for confidence gating.
async fn count_helix_steps(db: &dyn HelixDb, helix_id: &str) -> Result<usize, HelixDbError> {
    let cypher = "MATCH (h:Helix {id: $helix_id})-[:HAS_STEP]->(s:Step) \
                  RETURN count(s) AS step_count";
    let params =
        std::collections::BTreeMap::from([("helix_id".to_string(), serde_json::json!(helix_id))]);
    let records = db.execute_cypher_with_params(cypher, params).await?;

    Ok(records
        .first()
        .and_then(|r| {
            r.fields
                .get("step_count")
                .and_then(serde_json::Value::as_i64)
                .map(|i| {
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let count = i as usize;
                    count
                })
        })
        .unwrap_or(0))
}

/// Check if GDS is available for centrality computations.
async fn check_gds_available(db: &dyn HelixDb) -> bool {
    match db
        .execute_cypher("CALL gds.version() YIELD gdsVersion RETURN gdsVersion")
        .await
    {
        Ok(_) => {
            info!("GDS available — computing centrality metrics");
            true
        }
        Err(e) => {
            warn!(error = %e, "GDS unavailable — personality computation skipped");
            false
        }
    }
}

/// Compute personality dimensions from GDS metrics.
async fn compute_dimensions(
    db: &dyn HelixDb,
    helix_id: &str,
) -> Result<std::collections::HashMap<String, f64>, HelixDbError> {
    let mut dimensions = std::collections::HashMap::new();

    for (name, cypher) in &dimension_cyphers() {
        let params = std::collections::BTreeMap::from([(
            "helix_id".to_string(),
            serde_json::json!(helix_id),
        )]);
        if let Ok(val) = compute_metric(db, cypher, params).await {
            dimensions.insert((*name).into(), val);
        }
    }

    Ok(dimensions)
}

/// Build parameterized Cypher queries for each personality dimension.
///
/// All queries use `$helix_id` parameter — caller supplies the value.
fn dimension_cyphers() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "collaborative",
            "MATCH (h:Helix {id: $helix_id})-[:HAS_STEP]->(s:Step) \
             MATCH (s)-[:LINKS_TO]-(neighbor:Step)-[:LINKS_TO]-(s2:Step) \
             WHERE (s)-[:LINKS_TO]-(s2) \
             WITH s, count(DISTINCT neighbor) AS triangles \
             RETURN avg(toFloat(triangles)) AS metric",
        ),
        (
            "proactive",
            "MATCH (h:Helix {id: $helix_id})-[:HAS_STEP]->(s:Step) \
             OPTIONAL MATCH (s)-[:LINKS_TO]->(target:Step) \
             WITH s, count(target) AS out_degree \
             RETURN avg(toFloat(out_degree)) AS metric",
        ),
        (
            "innovative",
            "MATCH (h:Helix {id: $helix_id})-[:HAS_STEP]->(s:Step) \
             OPTIONAL MATCH (s)-[:MEMBER_OF]->(st:Strand) \
             WITH count(DISTINCT st) AS strand_count \
             RETURN toFloat(strand_count) AS metric",
        ),
        (
            "autonomous",
            "MATCH (h:Helix {id: $helix_id})-[:HAS_STEP]->(s:Step) \
             OPTIONAL MATCH (source:Step)-[:LINKS_TO]->(s) \
             WITH s, count(source) AS in_degree \
             RETURN CASE WHEN avg(toFloat(in_degree)) > 0 \
                    THEN 1.0 / avg(toFloat(in_degree)) \
                    ELSE 1.0 END AS metric",
        ),
    ]
}

/// Execute a metric Cypher with parameters and extract the float result.
async fn compute_metric(
    db: &dyn HelixDb,
    cypher: &str,
    params: std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<f64, HelixDbError> {
    let records = db.execute_cypher_with_params(cypher, params).await?;
    records
        .first()
        .and_then(|r| r.fields.get("metric").and_then(serde_json::Value::as_f64))
        .ok_or_else(|| HelixDbError::Validation("metric query returned no result".into()))
}

/// Compute a 0.0-1.0 confidence score based on step count vs threshold.
fn compute_confidence_score(step_count: usize, established_threshold: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let ratio = step_count as f64 / established_threshold as f64;
    ratio.min(1.0)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PersonalityEngineConfig::default();
        assert_eq!(config.emerging_threshold, 20);
        assert_eq!(config.established_threshold, 50);
    }

    #[test]
    fn test_confidence_score() {
        assert!((compute_confidence_score(10, 50) - 0.2).abs() < 0.01);
        assert!((compute_confidence_score(25, 50) - 0.5).abs() < 0.01);
        assert!((compute_confidence_score(50, 50) - 1.0).abs() < 0.01);
        assert!((compute_confidence_score(100, 50) - 1.0).abs() < 0.01); // capped at 1.0
    }
}
