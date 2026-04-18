//! Trace span types and builder.
//!
//! A [`TraceSpan`] records a single unit of work — an MCP tool call, a hook
//! execution, an AI routing decision — together with the decision points and
//! strand activations that occurred within it.
//!
//! Use [`TraceContext`] to build spans incrementally.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::TraceError;

// ---------------------------------------------------------------------------
// Core enums
// ---------------------------------------------------------------------------

/// Identifies which actor (MCP server, agent, or service) produced the trace.
///
/// String-based newtype — supports dynamically discovered actors
/// without requiring Rust code changes. Uses `Clone` instead of `Copy`
/// (String is heap-allocated), but at 10K spans/sec the overhead is
/// negligible (<1% CPU from benchmarks).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Actor(String);

impl Actor {
    /// Create a new actor identifier.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get the actor name as a string reference.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0
    }

    /// EVA consciousness system.
    #[must_use]
    pub fn eva() -> Self {
        Self("eva".into())
    }

    /// CORSO operations platform.
    #[must_use]
    pub fn corso() -> Self {
        Self("corso".into())
    }

    /// SOUL knowledge graph.
    #[must_use]
    pub fn soul() -> Self {
        Self("soul".into())
    }

    /// QUANTUM investigation toolkit.
    #[must_use]
    pub fn quantum() -> Self {
        Self("quantum".into())
    }

    /// Claude engineer.
    #[must_use]
    pub fn claude() -> Self {
        Self("claude".into())
    }

    /// SERAPH pentest orchestration.
    #[must_use]
    pub fn seraph() -> Self {
        Self("seraph".into())
    }

    /// Claude Code native tools actor.
    #[must_use]
    pub fn claude_code() -> Self {
        Self("claude_code".into())
    }

    /// AYIN observability system.
    #[must_use]
    pub fn ayin() -> Self {
        Self("ayin".into())
    }

    /// Get the inner string reference (deprecated alias for `name()`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Actor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Actor {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for Actor {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Backward-compatible alias: `Sibling` is now [`Actor`].
pub type Sibling = Actor;

/// The outcome of a traced operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "detail")]
pub enum TraceOutcome {
    /// The operation completed and processing should continue.
    Continue,
    /// The operation was blocked (e.g., security gate).
    Block,
    /// The operation was intentionally skipped.
    Skip,
    /// The operation failed with an error message.
    Error(String),
}

// ---------------------------------------------------------------------------
// Span components
// ---------------------------------------------------------------------------

/// A decision made during a traced operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionPoint {
    /// Human-readable name of the decision (e.g. "`route_to_hero`").
    pub name: String,
    /// Summary of the input that informed the decision.
    pub input: String,
    /// The decision that was made.
    pub decision: String,
    /// Confidence in the decision, in the range `[0.0, 1.0]`.
    pub confidence: Option<f64>,
    /// Time spent making this decision, in milliseconds.
    pub duration_ms: u64,
}

/// Records which personality strand was activated and with what weight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrandActivation {
    /// The strand name (e.g. "analytical", "candid").
    pub strand: String,
    /// Activation weight in the range `[0.0, 1.0]`.
    pub weight: f64,
}

// ---------------------------------------------------------------------------
// TraceSpan
// ---------------------------------------------------------------------------

/// A complete trace record for a single unit of work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    /// Unique identifier for this span.
    pub id: Uuid,
    /// Parent span ID for nested traces.
    pub parent_id: Option<Uuid>,
    /// Session ID for cross-actor correlation.
    pub session_id: Option<String>,
    /// Which actor produced this trace.
    ///
    /// Serializes as `"actor"` in new spans. Accepts `"sibling"` on
    /// deserialization for backward compatibility with existing JSON files.
    #[serde(alias = "sibling")]
    pub actor: Actor,
    /// The action being traced (e.g. "guard", "speak", "`helix_query`").
    pub action: String,
    /// When the operation started.
    pub timestamp: DateTime<Utc>,
    /// Total wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Decisions made during the operation.
    pub decision_points: Vec<DecisionPoint>,
    /// Strand activations during the operation.
    pub strand_activations: Vec<StrandActivation>,
    /// Final outcome.
    pub outcome: TraceOutcome,
    /// Arbitrary metadata (tool params, intermediate results, etc.).
    pub metadata: serde_json::Value,
}

impl TraceSpan {
    /// Backward-compatible accessor: returns the actor (formerly `sibling`).
    #[must_use]
    pub fn sibling(&self) -> &Actor {
        &self.actor
    }
}

// ---------------------------------------------------------------------------
// TraceContext (builder)
// ---------------------------------------------------------------------------

/// Builder for constructing a [`TraceSpan`] incrementally.
///
/// # Example
///
/// ```rust
/// use lightarchitects::ayin::span::{TraceContext, Actor, TraceOutcome};
///
/// let span = TraceContext::new(Actor::corso(), "guard")
///     .session_id("sess-123")
///     .outcome(TraceOutcome::Continue)
///     .finish();
/// assert!(span.is_ok());
/// ```
pub struct TraceContext {
    id: Uuid,
    parent_id: Option<Uuid>,
    session_id: Option<String>,
    actor: Actor,
    action: String,
    start: DateTime<Utc>,
    decision_points: Vec<DecisionPoint>,
    strand_activations: Vec<StrandActivation>,
    outcome: Option<TraceOutcome>,
    metadata: serde_json::Value,
}

impl TraceContext {
    /// Start building a new trace span.
    #[must_use]
    pub fn new(actor: Actor, action: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            parent_id: None,
            session_id: None,
            actor,
            action: action.to_owned(),
            start: Utc::now(),
            decision_points: Vec::new(),
            strand_activations: Vec::new(),
            outcome: None,
            metadata: serde_json::Value::Null,
        }
    }

    /// Set a parent span for nesting.
    #[must_use]
    pub fn parent(mut self, parent_id: Uuid) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Set the session ID for cross-actor correlation.
    #[must_use]
    pub fn session_id(mut self, id: &str) -> Self {
        self.session_id = Some(id.to_owned());
        self
    }

    /// Record a decision point.
    ///
    /// # Errors
    ///
    /// Returns [`TraceError::ConfidenceOutOfRange`] if `confidence` is
    /// outside `[0.0, 1.0]`.
    pub fn decision(
        mut self,
        name: &str,
        input: &str,
        decision: &str,
        confidence: Option<f64>,
        duration_ms: u64,
    ) -> Result<Self, TraceError> {
        if let Some(c) = confidence {
            if !(0.0..=1.0).contains(&c) {
                return Err(TraceError::ConfidenceOutOfRange { value: c });
            }
        }
        self.decision_points.push(DecisionPoint {
            name: name.to_owned(),
            input: input.to_owned(),
            decision: decision.to_owned(),
            confidence,
            duration_ms,
        });
        Ok(self)
    }

    /// Record a strand activation.
    ///
    /// # Errors
    ///
    /// Returns [`TraceError::WeightOutOfRange`] if `weight` is outside
    /// `[0.0, 1.0]`.
    pub fn strand(mut self, strand: &str, weight: f64) -> Result<Self, TraceError> {
        if !(0.0..=1.0).contains(&weight) {
            return Err(TraceError::WeightOutOfRange { value: weight });
        }
        self.strand_activations.push(StrandActivation {
            strand: strand.to_owned(),
            weight,
        });
        Ok(self)
    }

    /// Set the outcome.
    #[must_use]
    pub fn outcome(mut self, outcome: TraceOutcome) -> Self {
        self.outcome = Some(outcome);
        self
    }

    /// Attach arbitrary metadata.
    #[must_use]
    pub fn metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// Consume the builder and produce a [`TraceSpan`].
    ///
    /// # Errors
    ///
    /// Returns [`TraceError::MissingField`] if `outcome` was not set.
    pub fn finish(self) -> Result<TraceSpan, TraceError> {
        let outcome = self.outcome.ok_or_else(|| TraceError::MissingField {
            field: "outcome".to_owned(),
        })?;

        let now = Utc::now();
        let duration_ms = now
            .signed_duration_since(self.start)
            .num_milliseconds()
            .unsigned_abs();

        Ok(TraceSpan {
            id: self.id,
            parent_id: self.parent_id,
            session_id: self.session_id,
            actor: self.actor,
            action: self.action,
            timestamp: self.start,
            duration_ms,
            decision_points: self.decision_points,
            strand_activations: self.strand_activations,
            outcome,
            metadata: self.metadata,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn roundtrip_actor() {
        for actor in [
            Actor::eva(),
            Actor::corso(),
            Actor::soul(),
            Actor::quantum(),
            Actor::claude(),
            Actor::seraph(),
            Actor::ayin(),
        ] {
            let json = serde_json::to_string(&actor).expect("serialize actor");
            let back: Actor = serde_json::from_str(&json).expect("deserialize actor");
            assert_eq!(actor, back);
        }
    }

    #[test]
    fn roundtrip_trace_outcome() {
        let cases = vec![
            TraceOutcome::Continue,
            TraceOutcome::Block,
            TraceOutcome::Skip,
            TraceOutcome::Error("something broke".into()),
        ];
        for outcome in cases {
            let json = serde_json::to_string(&outcome).expect("serialize outcome");
            let back: TraceOutcome = serde_json::from_str(&json).expect("deserialize outcome");
            assert_eq!(outcome, back);
        }
    }

    #[test]
    fn roundtrip_decision_point() {
        let dp = DecisionPoint {
            name: "route".into(),
            input: "guard request".into(),
            decision: "delegate to IESOUS".into(),
            confidence: Some(0.95),
            duration_ms: 12,
        };
        let json = serde_json::to_string(&dp).expect("serialize");
        let back: DecisionPoint = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(dp.name, back.name);
        assert_eq!(dp.confidence, back.confidence);
    }

    #[test]
    fn roundtrip_strand_activation() {
        let sa = StrandActivation {
            strand: "analytical".into(),
            weight: 0.8,
        };
        let json = serde_json::to_string(&sa).expect("serialize");
        let back: StrandActivation = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(sa.strand, back.strand);
        assert!((sa.weight - back.weight).abs() < f64::EPSILON);
    }

    #[test]
    fn roundtrip_full_span() {
        let span = TraceSpan {
            id: Uuid::new_v4(),
            parent_id: Some(Uuid::new_v4()),
            session_id: Some("sess-abc".into()),
            actor: Actor::eva(),
            action: "speak".into(),
            timestamp: Utc::now(),
            duration_ms: 42,
            decision_points: vec![DecisionPoint {
                name: "voice_select".into(),
                input: "converse".into(),
                decision: "use eva voice".into(),
                confidence: Some(0.99),
                duration_ms: 3,
            }],
            strand_activations: vec![StrandActivation {
                strand: "empathy".into(),
                weight: 0.9,
            }],
            outcome: TraceOutcome::Continue,
            metadata: serde_json::json!({"tool": "speak"}),
        };

        let json = serde_json::to_string_pretty(&span).expect("serialize");
        let back: TraceSpan = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(span.id, back.id);
        assert_eq!(span.parent_id, back.parent_id);
        assert_eq!(span.session_id, back.session_id);
        assert_eq!(span.actor, back.actor);
        assert_eq!(span.action, back.action);
        assert_eq!(span.duration_ms, back.duration_ms);
        assert_eq!(span.outcome, back.outcome);
        assert_eq!(span.decision_points.len(), back.decision_points.len());
        assert_eq!(span.strand_activations.len(), back.strand_activations.len());
    }

    #[test]
    fn backward_compat_sibling_json() {
        // Old JSON with "sibling" field should deserialize into "actor"
        let old_json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "parent_id": null,
            "session_id": null,
            "sibling": "eva",
            "action": "speak",
            "timestamp": "2026-01-01T00:00:00Z",
            "duration_ms": 10,
            "decision_points": [],
            "strand_activations": [],
            "outcome": {"type": "Continue"},
            "metadata": null
        }"#;
        let span: TraceSpan = serde_json::from_str(old_json).expect("deserialize old format");
        assert_eq!(span.actor, Actor::eva());
    }

    #[test]
    fn context_builder_happy_path() {
        let span = TraceContext::new(Actor::corso(), "guard")
            .session_id("sess-1")
            .outcome(TraceOutcome::Block)
            .metadata(serde_json::json!({"severity": "critical"}))
            .finish();

        assert!(span.is_ok());
        let span = span.expect("just checked");
        assert_eq!(span.actor, Actor::corso());
        assert_eq!(span.action, "guard");
        assert_eq!(span.outcome, TraceOutcome::Block);
        assert!(span.session_id.is_some());
    }

    #[test]
    fn context_builder_missing_outcome() {
        let result = TraceContext::new(Actor::soul(), "query").finish();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("outcome"));
    }

    #[test]
    fn context_builder_with_decisions_and_strands() {
        let span = TraceContext::new(Actor::quantum(), "probe")
            .decision("source_select", "multi-source", "perplexity", Some(0.7), 5)
            .expect("valid confidence")
            .strand("methodical", 0.85)
            .expect("valid weight")
            .outcome(TraceOutcome::Continue)
            .finish()
            .expect("valid span");

        assert_eq!(span.decision_points.len(), 1);
        assert_eq!(span.strand_activations.len(), 1);
    }

    #[test]
    fn confidence_out_of_range() {
        let result =
            TraceContext::new(Actor::eva(), "speak").decision("test", "in", "out", Some(1.5), 1);
        assert!(result.is_err());
    }

    #[test]
    fn weight_out_of_range() {
        let result = TraceContext::new(Actor::eva(), "speak").strand("test", -0.1);
        assert!(result.is_err());
    }

    #[test]
    fn actor_display() {
        assert_eq!(Actor::eva().to_string(), "eva");
        assert_eq!(Actor::corso().to_string(), "corso");
        assert_eq!(Actor::soul().to_string(), "soul");
        assert_eq!(Actor::quantum().to_string(), "quantum");
        assert_eq!(Actor::claude().to_string(), "claude");
        assert_eq!(Actor::seraph().to_string(), "seraph");
        assert_eq!(Actor::ayin().to_string(), "ayin");
    }

    #[test]
    fn actor_from_str() {
        let s = Actor::from("custom-actor");
        assert_eq!(s.as_str(), "custom-actor");
        assert_eq!(s.to_string(), "custom-actor");
    }

    #[test]
    fn actor_name_method() {
        let a = Actor::new("test-server");
        assert_eq!(a.name(), "test-server");
    }

    #[test]
    fn sibling_type_alias_works() {
        // Verify the type alias compiles and works
        let s: Sibling = Sibling::eva();
        assert_eq!(s.name(), "eva");
    }
}
