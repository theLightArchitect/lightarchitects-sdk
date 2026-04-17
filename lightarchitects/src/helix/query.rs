//! Fluent query builder for helix step retrieval.
//!
//! [`HelixQuery`] provides a chainable API for filtering and ordering steps
//! without constructing raw Cypher. The builder is consumed by
//! [`HelixQuery::build_cypher`] which produces a parameterized query.
//!
//! # Usage
//!
//! ```rust,no_run
//! use crate::helix::query::HelixQuery;
//! use crate::helix::types::HelixOrderingMode;
//!
//! let (cypher, params) = HelixQuery::new()
//!     .owner("eva")
//!     .significance_min(7.0)
//!     .strand("emotional")
//!     .ordering_mode(HelixOrderingMode::Temporal)
//!     .limit(20)
//!     .build_cypher();
//! ```

use std::collections::BTreeMap;

use crate::helix::types::HelixOrderingMode;

// ============================================================================
// HelixQuery Builder
// ============================================================================

/// Fluent builder for helix step queries.
///
/// All filters are optional — an empty builder returns all steps (capped by limit).
/// Builder methods consume and return `self` for chaining.
///
/// # Freshness Filtering (RULE 1 Amendment — 2026-03-12)
///
/// Use [`HelixQuery::exclude_expired`] when building queries that feed into a
/// decision chain. When enabled, the generated Cypher adds:
///
/// ```cypher
/// AND (s.expires IS NULL OR s.expires > datetime())
/// ```
///
/// This passes all permanent entries (`expires: null`) and active context
/// entries, while excluding entries whose TTL has passed. Callers that need
/// to audit expired entries (e.g., maintenance, consolidator) should leave
/// this filter off (the default).
#[derive(Debug, Clone)]
pub struct HelixQuery {
    owner: Option<String>,
    epoch: Option<String>,
    significance_min: Option<f64>,
    strand: Option<String>,
    ordering_mode: Option<HelixOrderingMode>,
    limit: u32,
    offset: u32,
    depth: Option<u8>,
    /// When `true`, expired context/decision entries are excluded from results.
    /// Default: `false` — callers opt in for decision-chain queries.
    exclude_expired: bool,
}

impl Default for HelixQuery {
    fn default() -> Self {
        Self {
            owner: None,
            epoch: None,
            significance_min: None,
            strand: None,
            ordering_mode: None,
            limit: 20,
            offset: 0,
            depth: None,
            exclude_expired: false,
        }
    }
}

impl HelixQuery {
    /// Create a new query with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by helix owner (sibling name or app ID).
    #[must_use]
    pub fn owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// Filter by epoch (time period grouping).
    #[must_use]
    pub fn epoch(mut self, epoch: impl Into<String>) -> Self {
        self.epoch = Some(epoch.into());
        self
    }

    /// Minimum significance threshold.
    #[must_use]
    pub fn significance_min(mut self, min: f64) -> Self {
        self.significance_min = Some(min);
        self
    }

    /// Filter by strand membership.
    #[must_use]
    pub fn strand(mut self, strand: impl Into<String>) -> Self {
        self.strand = Some(strand.into());
        self
    }

    /// Override ordering mode (otherwise uses helix default).
    #[must_use]
    pub fn ordering_mode(mut self, mode: HelixOrderingMode) -> Self {
        self.ordering_mode = Some(mode);
        self
    }

    /// Maximum results to return (default: 20).
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// Offset for pagination (default: 0).
    #[must_use]
    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = offset;
        self
    }

    /// Maximum drill-down depth, capped at [`crate::helix::types::MAX_TRAVERSAL_DEPTH`].
    #[must_use]
    pub fn depth(mut self, depth: u8) -> Self {
        self.depth = Some(depth.min(crate::helix::types::MAX_TRAVERSAL_DEPTH));
        self
    }

    /// Build the Cypher query string and parameter map.
    ///
    /// The query uses the IS NULL pattern for optional filters:
    /// `($param IS NULL OR node.prop = $param)` — one plan, one cache entry.
    #[must_use]
    pub fn build_cypher(&self) -> (String, BTreeMap<String, serde_json::Value>) {
        let mut params = BTreeMap::new();
        let mut match_clause = String::from("MATCH (h:Helix)-[:HAS_STEP]->(s:Step)");
        let mut wheres = Vec::new();

        // Owner filter on Helix
        params.insert("owner".into(), opt_str(self.owner.as_deref()));
        wheres.push("($owner IS NULL OR h.owner = $owner)");

        // Significance filter on Step
        params.insert("min_sig".into(), opt_f64(self.significance_min));
        wheres.push("($min_sig IS NULL OR s.significance >= $min_sig)");

        // Epoch filter (stored in step metadata)
        params.insert("epoch".into(), opt_str(self.epoch.as_deref()));
        wheres.push("($epoch IS NULL OR s.metadata CONTAINS $epoch)");

        // Freshness gate: exclude expired context/decision entries when requested.
        // Permanent entries (expires IS NULL) always pass. Active entries pass.
        // Expired entries (expires <= datetime()) are filtered out.
        if self.exclude_expired {
            wheres.push("(s.expires IS NULL OR s.expires > datetime())");
        }

        // Strand filter requires additional MATCH
        if self.strand.is_some() {
            match_clause.push_str(", (s)-[:MEMBER_OF]->(st:Strand)");
            params.insert("strand".into(), opt_str(self.strand.as_deref()));
            wheres.push("st.name = $strand");
        }

        // Build ORDER BY from ordering mode
        let order = match self.ordering_mode {
            Some(HelixOrderingMode::Indexed) => "s.step_index ASC, s.created_at ASC",
            Some(HelixOrderingMode::Custom) => "s.metadata ASC, s.created_at ASC",
            _ => "s.step_date ASC, s.created_at ASC",
        };

        let where_str = if wheres.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", wheres.join(" AND "))
        };

        let cypher = format!(
            "{match_clause}{where_str} \
             RETURN s.id AS id, s.helix_id AS helix_id, s.title AS title, \
                    s.content AS content, s.significance AS significance, \
                    s.step_date AS step_date, s.step_index AS step_index, \
                    s.community_id AS community_id, s.expires AS expires, \
                    s.created_at AS created_at, s.metadata AS metadata \
             ORDER BY {order} \
             SKIP {skip} LIMIT {limit}",
            skip = self.offset,
            limit = self.limit,
        );

        (cypher, params)
    }

    /// Filter out expired context/decision entries (RULE 1 Amendment).
    ///
    /// When `true`, the generated Cypher adds:
    /// `AND (s.expires IS NULL OR s.expires > datetime())`
    ///
    /// Use this for queries that feed results into a decision chain.
    /// Leave `false` (the default) for audit, consolidation, or reporting queries
    /// that need to see all entries including expired ones.
    #[must_use]
    pub fn exclude_expired(mut self, exclude: bool) -> Self {
        self.exclude_expired = exclude;
        self
    }

    /// Get the configured depth (for drill-down operations).
    #[must_use]
    pub fn effective_depth(&self) -> u8 {
        self.depth
            .unwrap_or(crate::helix::types::MAX_TRAVERSAL_DEPTH)
            .min(crate::helix::types::MAX_TRAVERSAL_DEPTH)
    }
}

// ============================================================================
// Parameter Helpers
// ============================================================================

fn opt_str(val: Option<&str>) -> serde_json::Value {
    val.map_or(serde_json::Value::Null, |v| serde_json::json!(v))
}

fn opt_f64(val: Option<f64>) -> serde_json::Value {
    val.map_or(serde_json::Value::Null, |v| serde_json::json!(v))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_query() {
        let q = HelixQuery::new();
        assert_eq!(q.limit, 20);
        assert_eq!(q.offset, 0);
        assert!(q.owner.is_none());
    }

    #[test]
    fn test_builder_chain() {
        let q = HelixQuery::new()
            .owner("eva")
            .significance_min(7.0)
            .strand("emotional")
            .ordering_mode(HelixOrderingMode::Temporal)
            .limit(10)
            .offset(5)
            .depth(3);

        assert_eq!(q.owner.as_deref(), Some("eva"));
        assert_eq!(q.significance_min, Some(7.0));
        assert_eq!(q.strand.as_deref(), Some("emotional"));
        assert_eq!(q.ordering_mode, Some(HelixOrderingMode::Temporal));
        assert_eq!(q.limit, 10);
        assert_eq!(q.offset, 5);
        assert_eq!(q.depth, Some(3));
    }

    #[test]
    fn test_depth_clamped() {
        let q = HelixQuery::new().depth(99);
        assert_eq!(q.effective_depth(), crate::helix::types::MAX_TRAVERSAL_DEPTH);
    }

    #[test]
    fn test_build_cypher_basic() {
        let (cypher, params) = HelixQuery::new().owner("eva").limit(5).build_cypher();
        assert!(cypher.contains("MATCH"));
        assert!(cypher.contains("LIMIT 5"));
        assert!(cypher.contains("$owner"));
        assert_eq!(params.get("owner"), Some(&serde_json::json!("eva")));
    }

    #[test]
    fn test_build_cypher_with_strand() {
        let (cypher, _params) = HelixQuery::new().strand("emotional").build_cypher();
        assert!(cypher.contains("MEMBER_OF"));
        assert!(cypher.contains("st:Strand"));
    }

    #[test]
    fn test_build_cypher_ordering_indexed() {
        let (cypher, _) = HelixQuery::new()
            .ordering_mode(HelixOrderingMode::Indexed)
            .build_cypher();
        assert!(cypher.contains("step_index ASC"));
    }

    #[test]
    fn test_build_cypher_pagination() {
        let (cypher, _) = HelixQuery::new().offset(10).limit(5).build_cypher();
        assert!(cypher.contains("SKIP 10"));
        assert!(cypher.contains("LIMIT 5"));
    }

    #[test]
    fn test_build_cypher_no_filters() {
        let (cypher, params) = HelixQuery::new().build_cypher();
        assert!(cypher.contains("MATCH (h:Helix)-[:HAS_STEP]->(s:Step)"));
        // Owner should be null (no filter)
        assert_eq!(params.get("owner"), Some(&serde_json::Value::Null));
        // expires always in RETURN
        assert!(cypher.contains("s.expires AS expires"));
        // exclude_expired off by default — no freshness WHERE clause
        assert!(!cypher.contains("s.expires IS NULL OR s.expires > datetime()"));
    }

    #[test]
    fn test_build_cypher_exclude_expired() {
        let (cypher, _) = HelixQuery::new().exclude_expired(true).build_cypher();
        assert!(cypher.contains("s.expires IS NULL OR s.expires > datetime()"));
        // expires still in RETURN
        assert!(cypher.contains("s.expires AS expires"));
    }

    #[test]
    fn test_build_cypher_exclude_expired_false() {
        let (cypher, _) = HelixQuery::new().exclude_expired(false).build_cypher();
        // No freshness filter when opt-out
        assert!(!cypher.contains("s.expires IS NULL OR s.expires > datetime()"));
    }

    #[test]
    fn test_builder_include_exclude_expired_field() {
        let q = HelixQuery::new()
            .owner("quantum")
            .exclude_expired(true)
            .limit(10);
        assert_eq!(q.owner.as_deref(), Some("quantum"));
        assert!(q.exclude_expired);
        assert_eq!(q.limit, 10);
    }
}
