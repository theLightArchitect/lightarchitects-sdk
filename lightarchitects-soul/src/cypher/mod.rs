//! Cypher query generation traits and types.
//!
//! Provides the [`CypherGenerator`] trait for generating Neo4j Cypher queries
//! from natural-language questions and a [`GraphSchema`] descriptor.
//!
//! # Implementations
//!
//! - [`StaticCypherGenerator`] — template-based, offline, no LLM required.
//! - `LlmCypherGenerator` in `soul-helix` — LLM-backed, requires an online
//!   inference endpoint.
//!
//! # Feature Gate
//!
//! This module is compiled when the `cypher-gen` feature is enabled.
//!
//! # Examples
//!
//! ```rust
//! use lightarchitects_soul::cypher::{CypherGenerator, GraphSchema, NodeType, RelType, StaticCypherGenerator};
//!
//! let schema = GraphSchema {
//!     node_types: vec![NodeType {
//!         label: "Step".into(),
//!         properties: vec!["content".into(), "title".into()],
//!     }],
//!     rel_types: vec![],
//! };
//!
//! let generator = StaticCypherGenerator;
//! let cq = generator.generate(&schema, "consciousness breakthrough").unwrap();
//! assert!(cq.cypher.contains("MATCH"));
//! assert!(cq.params.contains_key("pattern"));
//! ```

#![allow(clippy::items_after_test_module)]

// ── Feature-gated submodules (text2cypher) ────────────────────────────────────

/// Mutation guard for LLM-generated Cypher queries.
///
/// Strips string literals then applies a whole-word blocklist scan for
/// mutation keywords (`CREATE`, `MERGE`, `DELETE`, `SET`, …).
#[cfg(feature = "text2cypher")]
pub mod sanitizer;

/// Neo4j schema descriptions for LLM prompt injection.
///
/// Provides [`BENCH_SCHEMA`](schema::BENCH_SCHEMA) for LongMemEval and
/// [`SOUL_HELIX_SCHEMA`](schema::SOUL_HELIX_SCHEMA) for the production helix.
#[cfg(feature = "text2cypher")]
pub mod schema;

/// LLM-backed Cypher generator — HTTP call → extract → sanitize → inject params.
///
/// Provides [`LlmCypherConfig`](llm::LlmCypherConfig) and
/// [`LlmCypherGenerator`](llm::LlmCypherGenerator), the canonical
/// implementation of [`AsyncCypherGenerator`].
#[cfg(feature = "text2cypher")]
pub mod llm;

// Re-exports from feature-gated submodules.
#[cfg(feature = "text2cypher")]
pub use llm::{LlmCypherConfig, LlmCypherGenerator};
#[cfg(feature = "text2cypher")]
pub use sanitizer::{SanitizeError, sanitize};
#[cfg(feature = "text2cypher")]
pub use schema::{BENCH_SCHEMA, SOUL_HELIX_SCHEMA};

use thiserror::Error;

// ============================================================================
// CypherError
// ============================================================================

/// Error type for Cypher generation failures.
#[derive(Debug, Error)]
pub enum CypherError {
    /// The query cannot be expressed in Cypher with the current implementation.
    #[error("unsupported query: {0}")]
    Unsupported(String),
    /// The provided schema is invalid or missing required types.
    #[error("schema error: {0}")]
    Schema(String),
}

// ============================================================================
// CypherQuery
// ============================================================================

/// A parameterized Cypher query with named `$`-placeholder parameters.
///
/// Rather than interpolating user-supplied values into the Cypher string,
/// [`CypherGenerator`] implementations produce a template with `$param`
/// placeholders and a separate parameter map. The Neo4j Bolt protocol binds
/// these at execution time without parsing them as Cypher — eliminating
/// injection even if the sanitizer were bypassed.
///
/// # Example
///
/// ```rust
/// # use lightarchitects_soul::cypher::{CypherQuery, CypherGenerator, GraphSchema, NodeType, StaticCypherGenerator};
/// let schema = GraphSchema {
///     node_types: vec![NodeType { label: "Step".into(), properties: vec![] }],
///     rel_types: vec![],
/// };
/// let cq = StaticCypherGenerator.generate(&schema, "trust").unwrap();
/// assert!(cq.cypher.contains("$pattern"));
/// assert!(cq.params.contains_key("pattern"));
/// ```
#[derive(Debug, Clone)]
pub struct CypherQuery {
    /// Cypher template with `$`-prefixed parameter placeholders
    /// (e.g., `WHERE s.content =~ $pattern`).
    pub cypher: String,
    /// Bolt parameter map — values bound at execution time.
    /// Keys match the `$`-prefixed names in `cypher`.
    pub params: std::collections::BTreeMap<String, serde_json::Value>,
}

// ============================================================================
// GraphSchema
// ============================================================================

/// Describes the Neo4j graph schema for query generation.
#[derive(Debug, Clone)]
pub struct GraphSchema {
    /// Node labels and their properties present in the graph.
    pub node_types: Vec<NodeType>,
    /// Relationship types connecting nodes.
    pub rel_types: Vec<RelType>,
}

// ============================================================================
// NodeType
// ============================================================================

/// A node label with its associated property names.
#[derive(Debug, Clone)]
pub struct NodeType {
    /// Neo4j node label (e.g., `"Step"`, `"Helix"`).
    pub label: String,
    /// Property names present on nodes with this label.
    pub properties: Vec<String>,
}

// ============================================================================
// RelType
// ============================================================================

/// A directed relationship type between two node labels.
#[derive(Debug, Clone)]
pub struct RelType {
    /// Relationship type name (e.g., `"LINKS_TO"`).
    pub name: String,
    /// Source node label.
    pub from: String,
    /// Target node label.
    pub to: String,
}

// ============================================================================
// CypherGenerator trait
// ============================================================================

/// Generates Neo4j Cypher queries from typed query specifications.
///
/// Implementations include [`StaticCypherGenerator`] (template-based, offline)
/// and `LlmCypherGenerator` in `soul-helix` (LLM-backed, online).
pub trait CypherGenerator: Send + Sync {
    /// Generate a parameterized Cypher query for the given question and schema.
    ///
    /// Returns a [`CypherQuery`] containing a template with `$param` placeholders
    /// and the matching parameter map. Execute via
    /// `HelixDb::execute_cypher_with_params` — the Bolt layer binds parameters
    /// without parsing them as Cypher, preventing injection.
    ///
    /// # Errors
    ///
    /// Returns [`CypherError::Unsupported`] when the question cannot be expressed,
    /// or [`CypherError::Schema`] when required schema elements are missing.
    fn generate(&self, schema: &GraphSchema, question: &str) -> Result<CypherQuery, CypherError>;
}

// ============================================================================
// StaticCypherGenerator
// ============================================================================

/// Template-based Cypher generator — offline, no LLM required.
///
/// Generates simple content-match queries using up to 3 question terms.
/// Suitable for development and testing without a live inference endpoint.
///
/// # RETURN clause
///
/// When the schema's first `NodeType` has properties listed, the generator
/// emits a `RETURN s.prop AS prop, …` clause for each listed property.
/// This matches the aliased-property format expected by `graph_engine::Record`
/// consumers (e.g., soul-helix `record_to_step`).
///
/// When no properties are listed the clause falls back to `RETURN s`
/// (whole-node projection).
pub struct StaticCypherGenerator;

impl CypherGenerator for StaticCypherGenerator {
    fn generate(&self, schema: &GraphSchema, question: &str) -> Result<CypherQuery, CypherError> {
        if question.is_empty() {
            return Err(CypherError::Unsupported(
                "question must not be empty".into(),
            ));
        }

        // Sanitize each term to alphanumeric + hyphen/underscore only.
        // Prevents Cypher regex injection via `'`, `(`, `*`, `\`, and other
        // Java regex metacharacters embedded in the Neo4j =~ pattern.
        let terms: Vec<String> = question
            .split_whitespace()
            .map(|t| {
                t.chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect::<String>()
            })
            .filter(|t| !t.is_empty())
            .take(3)
            .collect();

        if terms.is_empty() {
            return Err(CypherError::Unsupported(
                "no valid terms after sanitization".into(),
            ));
        }

        let pattern = terms.join("|");

        // Build node label and RETURN clause from schema.
        let label = schema
            .node_types
            .first()
            .map_or("Step", |nt| nt.label.as_str());

        let return_clause = match schema.node_types.first() {
            Some(nt) if !nt.properties.is_empty() => {
                let cols = nt
                    .properties
                    .iter()
                    .map(|p| format!("s.{p} AS {p}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("RETURN {cols}")
            }
            _ => "RETURN s".to_owned(),
        };

        // The regex pattern is passed as a Bolt parameter ($pattern), not
        // interpolated into the query string. Even though terms are sanitized,
        // the Bolt protocol provides a second layer: parameters are bound at
        // the driver level and never reach the Cypher parser as text.
        let mut params = std::collections::BTreeMap::new();
        params.insert(
            "pattern".into(),
            serde_json::Value::String(format!("(?i).*({pattern}).*")),
        );

        Ok(CypherQuery {
            cypher: format!(
                "MATCH (s:{label}) WHERE s.content =~ $pattern {return_clause} LIMIT 10"
            ),
            params,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn default_schema() -> GraphSchema {
        GraphSchema {
            node_types: vec![NodeType {
                label: "Step".into(),
                // Mirrors the soul-helix record_to_step aliased-RETURN format.
                properties: vec![
                    "id".into(),
                    "helix_id".into(),
                    "title".into(),
                    "content".into(),
                    "significance".into(),
                ],
            }],
            rel_types: vec![],
        }
    }

    #[test]
    fn test_static_generator_produces_match() {
        let generator = StaticCypherGenerator;
        let schema = default_schema();
        let cq = generator
            .generate(&schema, "consciousness breakthrough")
            .unwrap();
        assert!(
            cq.cypher.contains("MATCH"),
            "should contain MATCH: {}",
            cq.cypher
        );
        assert!(
            cq.cypher.contains("RETURN"),
            "should contain RETURN: {}",
            cq.cypher
        );
        // Query term lives in the $pattern parameter, not interpolated into the template.
        let pattern = cq
            .params
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            pattern.contains("consciousness"),
            "pattern param should contain query term: {pattern}"
        );
    }

    #[test]
    fn test_cypher_query_uses_bolt_parameter() {
        let generator = StaticCypherGenerator;
        let schema = default_schema();
        let cq = generator.generate(&schema, "trust").unwrap();
        // Cypher template uses $pattern placeholder — user input is never in the template.
        assert!(
            cq.cypher.contains("$pattern"),
            "cypher must use $pattern placeholder: {}",
            cq.cypher
        );
        assert!(
            cq.params.contains_key("pattern"),
            "params must contain 'pattern' key"
        );
        let pattern_val = cq.params["pattern"].as_str().unwrap_or("");
        assert!(
            pattern_val.contains("trust"),
            "pattern value must contain the query term: {pattern_val}"
        );
        // Raw query term must NOT be interpolated directly into the cypher string.
        assert!(
            !cq.cypher.contains("trust"),
            "cypher string must not contain raw query term: {}",
            cq.cypher
        );
    }

    #[test]
    fn test_static_generator_empty_question_errors() {
        let generator = StaticCypherGenerator;
        let schema = default_schema();
        let result = generator.generate(&schema, "");
        assert!(result.is_err(), "empty question should produce an error");
    }

    #[test]
    fn test_static_generator_takes_max_three_terms() {
        let generator = StaticCypherGenerator;
        let schema = default_schema();
        let cq = generator
            .generate(&schema, "one two three four five")
            .unwrap();
        // Should only include first 3 terms in the $pattern parameter value.
        let pattern = cq
            .params
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            pattern.contains("one|two|three"),
            "should use first 3 terms: {pattern}"
        );
        assert!(
            !pattern.contains("four") || !pattern.contains("five"),
            "should not include terms beyond first 3: {pattern}"
        );
    }

    #[test]
    fn test_schema_properties_produce_aliased_return() {
        let generator = StaticCypherGenerator;
        let schema = default_schema(); // has id, helix_id, title, content, significance
        let cq = generator.generate(&schema, "consciousness").unwrap();
        // Each schema property should appear as `s.prop AS prop` alias in the template.
        assert!(
            cq.cypher.contains("s.id AS id"),
            "should alias id: {}",
            cq.cypher
        );
        assert!(
            cq.cypher.contains("s.content AS content"),
            "should alias content: {}",
            cq.cypher
        );
        // Whole-node shorthand must NOT appear when schema has properties.
        assert!(
            !cq.cypher.contains("RETURN s ") && !cq.cypher.ends_with("RETURN s"),
            "should not use whole-node RETURN when properties are specified: {}",
            cq.cypher
        );
    }

    #[test]
    fn test_empty_schema_falls_back_to_return_s() {
        let generator = StaticCypherGenerator;
        let schema = GraphSchema {
            node_types: vec![NodeType {
                label: "Step".into(),
                properties: vec![],
            }],
            rel_types: vec![],
        };
        let cq = generator.generate(&schema, "consciousness").unwrap();
        assert!(
            cq.cypher.contains("RETURN s"),
            "empty schema should fall back to RETURN s: {}",
            cq.cypher
        );
    }

    #[test]
    fn test_cypher_error_display() {
        assert_eq!(
            CypherError::Unsupported("not supported".into()).to_string(),
            "unsupported query: not supported"
        );
        assert_eq!(
            CypherError::Schema("missing label".into()).to_string(),
            "schema error: missing label"
        );
    }

    #[test]
    fn test_injection_chars_stripped_from_pattern() {
        let generator = StaticCypherGenerator;
        let schema = default_schema();
        // SERAPH S1 full hardening: sanitized terms go into the $pattern bolt parameter.
        // The cypher template must not contain user-supplied terms at all.
        let cq = generator
            .generate(&schema, "x')OR 1=1 MATCH (n) RETURN n//")
            .unwrap();
        let pattern = cq
            .params
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        // The injection payload "OR 1=1" (with space) must not appear in the param value.
        assert!(
            !pattern.contains("OR 1=1"),
            "injection payload must not appear: {pattern}"
        );
        // No bare `)` from the input `x')OR` — sanitizer strips it.
        assert!(
            !pattern.contains("')"),
            "unescaped input quote/paren must not appear in pattern: {pattern}"
        );
        // The safe alphabetic portion of "x')OR" survives as "xOR".
        assert!(
            pattern.contains('x'),
            "alphanumeric terms must survive: {pattern}"
        );
        // Cypher template must use $pattern, not any user text.
        assert!(
            cq.cypher.contains("$pattern"),
            "cypher must reference $pattern placeholder: {}",
            cq.cypher
        );
    }

    #[test]
    fn test_all_metachar_question_errors() {
        let generator = StaticCypherGenerator;
        let schema = default_schema();
        // A question consisting entirely of metacharacters has no valid terms.
        let result = generator.generate(&schema, "!@#$ %^& *()'");
        assert!(result.is_err(), "question with no valid terms must error");
    }
}

// ============================================================================
// AsyncCypherGenerator trait
// ============================================================================

/// Async variant of [`CypherGenerator`] for LLM-backed implementations.
///
/// Defined here as a trait-only interface. The canonical implementation is
/// [`LlmCypherGenerator`](llm::LlmCypherGenerator) in this crate (behind the
/// `text2cypher` feature gate) which calls an OpenAI-compatible endpoint.
///
/// # MSRV
///
/// Uses AFIT (async fn in trait, stable since Rust 1.75). MSRV for this
/// crate is 1.87 — no compatibility concerns.
///
/// # Examples
///
/// ```rust,no_run
/// # #[cfg(feature = "text2cypher")]
/// # async fn example() -> Result<(), lightarchitects_soul::cypher::CypherError> {
/// use lightarchitects_soul::cypher::{
///     AsyncCypherGenerator, GraphSchema, LlmCypherConfig, LlmCypherGenerator,
/// };
///
/// let config = LlmCypherConfig {
///     helix_id: Some("bench-xyz".into()),
///     ..LlmCypherConfig::default()
/// };
/// let gen = LlmCypherGenerator::new(config).expect("client build");
/// let schema = GraphSchema { node_types: vec![], rel_types: vec![] };
/// let cq = gen.generate_async(&schema, "how many times did I go to the gym?").await?;
/// println!("{}", cq.cypher);
/// # Ok(()) }
/// ```
pub trait AsyncCypherGenerator: Send + Sync {
    /// Generate a parameterized Cypher query asynchronously.
    ///
    /// The `schema` provides a structural hint about the graph (not always
    /// used by LLM-backed implementations that rely on the config
    /// `schema_hint` instead). The `question` is the natural-language input.
    ///
    /// Returns a [`CypherQuery`] with `$param` placeholders and the matching
    /// Bolt parameter map, ready for execution via
    /// `HelixDb::execute_cypher_with_params`.
    ///
    /// # Errors
    ///
    /// Returns [`CypherError::Unsupported`] when the LLM call fails or the
    /// generated Cypher is rejected by the mutation sanitizer.
    fn generate_async(
        &self,
        schema: &GraphSchema,
        question: &str,
    ) -> impl std::future::Future<Output = Result<CypherQuery, CypherError>> + Send;
}
