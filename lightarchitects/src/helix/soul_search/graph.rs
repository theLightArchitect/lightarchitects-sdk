//! Graph searcher — Cypher traversal patterns for helix graph navigation.
//!
//! Generates parameterized Cypher for each filter type and converts graph
//! distance into a relevance score: `1.0 / (1.0 + distance)`.
//!
//! All fractal traversal uses quantified path patterns with inline predicates
//! and depth bound `{1,7}` (capped by [`MAX_TRAVERSAL_DEPTH`]).

use std::collections::HashMap;

use tracing::{instrument, warn};

use crate::helix::db::{HelixDb, HelixDbError};
use crate::helix::types::MAX_TRAVERSAL_DEPTH;

use super::{RetrievalSignal, ScoredId};

// ============================================================================
// GraphFilter
// ============================================================================

/// A graph traversal filter — defines how the graph searcher navigates the helix.
#[derive(Debug, Clone)]
pub enum GraphFilter {
    /// Steps owned by a specific sibling, ordered by helix `ordering_mode`.
    Owner(String),

    /// Steps in a specific strand, ordered by the strand's helix `ordering_mode`.
    Strand(String),

    /// Steps sharing a `SharedExperience` with steps from another helix owner.
    ConvergenceWith {
        /// The "from" owner whose steps we're searching.
        helix_owner: String,
        /// The "with" owner to find convergences against.
        other_owner: String,
    },

    /// Fractal drill-down from a step through sub-helixes.
    DrillDown {
        /// Starting step ID.
        step_id: String,
        /// Maximum depth (capped at `MAX_TRAVERSAL_DEPTH`).
        depth: u8,
        /// Minimum significance for inline predicate.
        min_significance: Option<f64>,
    },

    /// Reverse link traversal — steps that link TO a given step.
    Backlinks(String),

    /// Forward link traversal — steps that a given step links TO.
    OutgoingLinks(String),

    /// Steps within a date range for a specific helix owner.
    ByDay {
        /// Helix owner (sibling name).
        owner: String,
        /// Start date (inclusive, ISO format).
        start_date: String,
        /// End date (inclusive, ISO format).
        end_date: String,
    },

    /// Steps in a Louvain community (assigned nightly by GDS).
    Community(i64),
}

// ============================================================================
// GraphSearcher
// ============================================================================

/// Graph traversal searcher over the helix structure.
///
/// Each [`GraphFilter`] variant generates a specific Cypher pattern.
/// Results are scored by graph distance: `1.0 / (1.0 + path_length)`.
///
/// # Security
///
/// All queries use parameterized `$param` placeholders for user-supplied
/// values. Labels and relationship types are hard-coded constants from the
/// helix domain vocabulary.
pub struct GraphSearcher;

impl GraphSearcher {
    /// Search the graph using the specified filter.
    ///
    /// Returns scored IDs sorted by graph relevance (highest first).
    ///
    /// # Errors
    ///
    /// Returns `HelixDbError` if the Cypher query fails.
    #[instrument(skip(db), fields(filter = %filter_label(filter)))]
    pub async fn search(
        db: &dyn HelixDb,
        filter: &GraphFilter,
        limit: u32,
    ) -> Result<Vec<ScoredId>, HelixDbError> {
        let (cypher, params) = build_cypher(filter, limit);
        let records = db.execute_cypher_with_params(&cypher, params).await?;

        let mut results = Vec::with_capacity(records.len());
        for record in &records {
            let step_id = extract_string(record, "step_id");
            let distance = extract_f64(record, "distance");

            if let Some(id) = step_id {
                let score = 1.0 / (1.0 + distance.unwrap_or(0.0));
                results.push(ScoredId {
                    step_id: id,
                    score,
                    signal: RetrievalSignal::Graph,
                });
            }
        }

        let results = blend_with_pagerank(db, results).await;
        Ok(results)
    }
}

// ============================================================================
// GDS PageRank blending (Phase 20b.2)
// ============================================================================

/// Blend distance-based graph scores with GDS `PageRank` centrality.
///
/// When GDS is available and the `helix-projection` exists, fetches `PageRank`
/// for every step in `results` and applies:
///
/// ```text
/// combined = 0.5 * norm_pr + 0.5 * dist_score
/// norm_pr  = raw_pr / (1.0 + raw_pr)   // maps [0, +∞) → [0, 1)
/// ```
///
/// Falls back silently to the original distance scores when:
/// - GDS plugin is absent (`CALL gds.version()` fails)
/// - The `helix-projection` does not exist (consolidation hasn't run yet)
/// - No `PageRank` scores are returned for the result set
async fn blend_with_pagerank(db: &dyn HelixDb, mut results: Vec<ScoredId>) -> Vec<ScoredId> {
    if results.is_empty() {
        return results;
    }

    // Phase 1 — GDS availability probe.
    if db
        .execute_cypher("CALL gds.version() YIELD gdsVersion RETURN gdsVersion")
        .await
        .is_err()
    {
        return results;
    }

    // Phase 2 — fetch PageRank for the exact step IDs in the result set.
    let step_ids: Vec<&str> = results.iter().map(|r| r.step_id.as_str()).collect();
    let mut params = std::collections::BTreeMap::new();
    params.insert("step_ids".into(), serde_json::json!(step_ids));

    let cypher = "CALL gds.pageRank.stream('helix-projection', \
                  {maxIterations: 20, dampingFactor: 0.85}) \
                  YIELD nodeId, score \
                  WITH gds.util.asNode(nodeId) AS node, score \
                  WHERE node.id IN $step_ids \
                  RETURN node.id AS step_id, score";

    let pr_records = match db.execute_cypher_with_params(cypher, params).await {
        Ok(r) => r,
        Err(e) => {
            warn!(
                error = %e,
                "PageRank stream failed (projection absent?) — keeping distance scores"
            );
            return results;
        }
    };

    let pr_map: HashMap<String, f64> = pr_records
        .iter()
        .filter_map(|r| {
            let id = r.fields.get("step_id")?.as_str()?.to_owned();
            let raw = r.fields.get("score")?.as_f64()?;
            Some((id, raw))
        })
        .collect();

    if pr_map.is_empty() {
        return results;
    }

    // Phase 3 — blend and re-sort.
    for result in &mut results {
        if let Some(&raw_pr) = pr_map.get(&result.step_id) {
            let norm_pr = raw_pr / (1.0 + raw_pr);
            result.score = 0.5 * norm_pr + 0.5 * result.score;
        }
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

// ============================================================================
// Cypher Builders (parameterized — GUARD mandate)
// ============================================================================

/// Type alias for a parameterized Cypher query and its parameters.
type CypherWithParams = (
    String,
    std::collections::BTreeMap<String, serde_json::Value>,
);

fn build_cypher(filter: &GraphFilter, limit: u32) -> CypherWithParams {
    match filter {
        GraphFilter::Owner(owner) => cypher_owner(owner, limit),
        GraphFilter::Strand(name) => cypher_strand(name, limit),
        GraphFilter::ConvergenceWith {
            helix_owner,
            other_owner,
        } => cypher_convergence(helix_owner, other_owner, limit),
        GraphFilter::DrillDown {
            step_id,
            depth,
            min_significance,
        } => cypher_drill_down(step_id, *depth, min_significance.as_ref(), limit),
        GraphFilter::Backlinks(id) => cypher_backlinks(id, limit),
        GraphFilter::OutgoingLinks(id) => cypher_outgoing(id, limit),
        GraphFilter::ByDay {
            owner,
            start_date,
            end_date,
        } => cypher_by_day(owner, start_date, end_date, limit),
        GraphFilter::Community(id) => cypher_community(*id, limit),
    }
}

fn cypher_owner(owner: &str, limit: u32) -> CypherWithParams {
    let cypher = format!(
        "MATCH (h:Helix {{owner: $owner}})-[:HAS_STEP]->(s:Step) \
         RETURN s.id AS step_id, 0 AS distance \
         ORDER BY COALESCE(s.step_date, toString(s.step_index), \
         toString(s.created_at)) DESC \
         LIMIT {limit}"
    );
    let mut params = std::collections::BTreeMap::new();
    params.insert("owner".into(), serde_json::json!(owner));
    (cypher, params)
}

fn cypher_strand(strand_name: &str, limit: u32) -> CypherWithParams {
    let cypher = format!(
        "MATCH (s:Step)-[:MEMBER_OF]->(st:Strand {{name: $strand_name}}) \
         RETURN s.id AS step_id, 0 AS distance \
         ORDER BY s.significance DESC \
         LIMIT {limit}"
    );
    let mut params = std::collections::BTreeMap::new();
    params.insert("strand_name".into(), serde_json::json!(strand_name));
    (cypher, params)
}

fn cypher_convergence(helix_owner: &str, other_owner: &str, limit: u32) -> CypherWithParams {
    let cypher = format!(
        "MATCH (a:Step)<-[:HAS_STEP]-(ha:Helix {{owner: $helix_owner}}) \
         MATCH (a)-[:PARTICIPATES_IN]->(se:SharedExperience)\
         <-[:PARTICIPATES_IN]-(b:Step) \
         MATCH (hb:Helix {{owner: $other_owner}})-[:HAS_STEP]->(b) \
         RETURN a.id AS step_id, (1.0 / se.weight) AS distance \
         ORDER BY se.weight DESC \
         LIMIT {limit}"
    );
    let mut params = std::collections::BTreeMap::new();
    params.insert("helix_owner".into(), serde_json::json!(helix_owner));
    params.insert("other_owner".into(), serde_json::json!(other_owner));
    (cypher, params)
}

fn cypher_drill_down(
    step_id: &str,
    depth: u8,
    min_significance: Option<&f64>,
    limit: u32,
) -> CypherWithParams {
    let effective_depth = depth.min(MAX_TRAVERSAL_DEPTH);
    let sig_filter = if min_significance.is_some() {
        " AND child.significance >= $min_sig"
    } else {
        ""
    };
    let cypher = format!(
        "MATCH (start:Step {{id: $step_id}})\
         -[:HAS_SUB_HELIX*1..{effective_depth}]->(sub:Helix)\
         -[:HAS_STEP]->(child:Step) \
         WHERE child.id <> $step_id{sig_filter} \
         WITH child, length(shortestPath((start)-[*]-(child))) AS dist \
         RETURN child.id AS step_id, dist AS distance \
         ORDER BY dist ASC, child.significance DESC \
         LIMIT {limit}"
    );
    let mut params = std::collections::BTreeMap::new();
    params.insert("step_id".into(), serde_json::json!(step_id));
    if let Some(sig) = min_significance {
        params.insert("min_sig".into(), serde_json::json!(sig));
    }
    (cypher, params)
}

fn cypher_backlinks(step_id: &str, limit: u32) -> CypherWithParams {
    let cypher = format!(
        "MATCH (source:Step)-[:LINKS_TO]->(target:Step {{id: $step_id}}) \
         RETURN source.id AS step_id, 1 AS distance \
         ORDER BY source.significance DESC \
         LIMIT {limit}"
    );
    let mut params = std::collections::BTreeMap::new();
    params.insert("step_id".into(), serde_json::json!(step_id));
    (cypher, params)
}

fn cypher_outgoing(step_id: &str, limit: u32) -> CypherWithParams {
    let cypher = format!(
        "MATCH (source:Step {{id: $step_id}})-[r:LINKS_TO]->(target:Step) \
         RETURN target.id AS step_id, 1 AS distance \
         ORDER BY r.strength DESC, target.significance DESC \
         LIMIT {limit}"
    );
    let mut params = std::collections::BTreeMap::new();
    params.insert("step_id".into(), serde_json::json!(step_id));
    (cypher, params)
}

fn cypher_by_day(owner: &str, start_date: &str, end_date: &str, limit: u32) -> CypherWithParams {
    let cypher = format!(
        "MATCH (h:Helix {{owner: $owner}})-[:HAS_STEP]->(s:Step) \
         WHERE s.step_date >= date($start_date) \
         AND s.step_date <= date($end_date) \
         RETURN s.id AS step_id, 0 AS distance \
         ORDER BY s.step_date ASC \
         LIMIT {limit}"
    );
    let mut params = std::collections::BTreeMap::new();
    params.insert("owner".into(), serde_json::json!(owner));
    params.insert("start_date".into(), serde_json::json!(start_date));
    params.insert("end_date".into(), serde_json::json!(end_date));
    (cypher, params)
}

fn cypher_community(community_id: i64, limit: u32) -> CypherWithParams {
    let cypher = format!(
        "MATCH (s:Step {{community_id: $community_id}}) \
         RETURN s.id AS step_id, 0 AS distance \
         ORDER BY s.significance DESC \
         LIMIT {limit}"
    );
    let mut params = std::collections::BTreeMap::new();
    params.insert("community_id".into(), serde_json::json!(community_id));
    (cypher, params)
}

/// Extract a string field from a graph-engine Record.
fn extract_string(record: &crate::helix::graph::Record, key: &str) -> Option<String> {
    record
        .fields
        .get(key)
        .and_then(|v| v.as_str().map(String::from))
}

/// Extract a float field from a graph-engine Record.
fn extract_f64(record: &crate::helix::graph::Record, key: &str) -> Option<f64> {
    record.fields.get(key).and_then(serde_json::Value::as_f64)
}

/// Human-readable label for tracing.
fn filter_label(filter: &GraphFilter) -> &'static str {
    match filter {
        GraphFilter::Owner(_) => "owner",
        GraphFilter::Strand(_) => "strand",
        GraphFilter::ConvergenceWith { .. } => "convergence",
        GraphFilter::DrillDown { .. } => "drill_down",
        GraphFilter::Backlinks(_) => "backlinks",
        GraphFilter::OutgoingLinks(_) => "outgoing_links",
        GraphFilter::ByDay { .. } => "by_day",
        GraphFilter::Community(_) => "community",
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_owner_cypher() {
        let (cypher, params) = build_cypher(&GraphFilter::Owner("eva".into()), 10);
        assert!(cypher.contains("$owner"));
        assert!(cypher.contains("LIMIT 10"));
        assert!(cypher.contains("step_id"));
        assert_eq!(params.get("owner"), Some(&serde_json::json!("eva")));
    }

    #[test]
    fn test_strand_cypher() {
        let (cypher, params) = build_cypher(&GraphFilter::Strand("emotional".into()), 5);
        assert!(cypher.contains("MEMBER_OF"));
        assert!(cypher.contains("$strand_name"));
        assert_eq!(
            params.get("strand_name"),
            Some(&serde_json::json!("emotional"))
        );
    }

    #[test]
    fn test_convergence_cypher() {
        let (cypher, params) = build_cypher(
            &GraphFilter::ConvergenceWith {
                helix_owner: "eva".into(),
                other_owner: "corso".into(),
            },
            20,
        );
        assert!(cypher.contains("PARTICIPATES_IN"));
        assert!(cypher.contains("SharedExperience"));
        assert!(cypher.contains("$helix_owner"));
        assert!(cypher.contains("$other_owner"));
        assert_eq!(params.get("helix_owner"), Some(&serde_json::json!("eva")));
        assert_eq!(params.get("other_owner"), Some(&serde_json::json!("corso")));
    }

    #[test]
    fn test_drill_down_cypher_depth_capped() {
        let (cypher, params) = build_cypher(
            &GraphFilter::DrillDown {
                step_id: "s1".into(),
                depth: 99,
                min_significance: Some(5.0),
            },
            10,
        );
        let max = MAX_TRAVERSAL_DEPTH;
        assert!(cypher.contains(&format!("*1..{max}")));
        assert!(cypher.contains("$min_sig"));
        assert_eq!(params.get("step_id"), Some(&serde_json::json!("s1")));
        assert_eq!(params.get("min_sig"), Some(&serde_json::json!(5.0)));
    }

    #[test]
    fn test_backlinks_cypher() {
        let (cypher, params) = build_cypher(&GraphFilter::Backlinks("step-42".into()), 10);
        assert!(cypher.contains("LINKS_TO"));
        assert!(cypher.contains("$step_id"));
        assert!(cypher.contains("source.id AS step_id"));
        assert_eq!(params.get("step_id"), Some(&serde_json::json!("step-42")));
    }

    #[test]
    fn test_outgoing_links_cypher() {
        let (cypher, params) = build_cypher(&GraphFilter::OutgoingLinks("step-42".into()), 10);
        assert!(cypher.contains("LINKS_TO"));
        assert!(cypher.contains("target.id AS step_id"));
        assert_eq!(params.get("step_id"), Some(&serde_json::json!("step-42")));
    }

    #[test]
    fn test_by_day_cypher() {
        let (cypher, params) = build_cypher(
            &GraphFilter::ByDay {
                owner: "eva".into(),
                start_date: "2026-01-01".into(),
                end_date: "2026-01-31".into(),
            },
            50,
        );
        assert!(cypher.contains("$start_date"));
        assert!(cypher.contains("$end_date"));
        assert!(cypher.contains("step_date"));
        assert_eq!(
            params.get("start_date"),
            Some(&serde_json::json!("2026-01-01"))
        );
        assert_eq!(
            params.get("end_date"),
            Some(&serde_json::json!("2026-01-31"))
        );
    }

    #[test]
    fn test_community_cypher() {
        let (cypher, params) = build_cypher(&GraphFilter::Community(42), 20);
        assert!(cypher.contains("$community_id"));
        assert_eq!(params.get("community_id"), Some(&serde_json::json!(42)));
    }

    #[test]
    fn test_filter_labels() {
        assert_eq!(filter_label(&GraphFilter::Owner("x".into())), "owner");
        assert_eq!(filter_label(&GraphFilter::Strand("x".into())), "strand");
        assert_eq!(
            filter_label(&GraphFilter::Backlinks("x".into())),
            "backlinks"
        );
        assert_eq!(filter_label(&GraphFilter::Community(1)), "community");
    }

    #[test]
    fn test_graph_score_formula() {
        // distance=0 → score=1.0, distance=1 → score=0.5, distance=9 → score=0.1
        assert!((1.0_f64 / (1.0 + 0.0) - 1.0).abs() < f64::EPSILON);
        assert!((1.0_f64 / (1.0 + 1.0) - 0.5).abs() < f64::EPSILON);
        assert!((1.0_f64 / (1.0 + 9.0) - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pagerank_norm_formula() {
        // norm_pr = raw / (1 + raw) maps [0, +∞) → [0, 1)
        assert!((0.0_f64 / (1.0 + 0.0_f64)).abs() < f64::EPSILON); // 0.0
        assert!((1.0_f64 / (1.0 + 1.0_f64) - 0.5).abs() < f64::EPSILON); // 0.5
        let near_one = 99.0_f64 / (1.0 + 99.0_f64);
        assert!(
            near_one > 0.98 && near_one < 1.0,
            "large raw_pr should normalize near 1.0"
        );

        // Blend formula: combined = 0.5 * norm_pr + 0.5 * dist_score
        // dist_score=0.5 (distance=1), raw_pr=1.0 (norm_pr=0.5) → blended=0.5
        let norm_pr = 1.0_f64 / (1.0 + 1.0_f64);
        let blended = 0.5 * norm_pr + 0.5 * 0.5_f64;
        assert!((blended - 0.5).abs() < f64::EPSILON);
    }
}
