//! LASDLC OpenTelemetry semantic-convention attribute constants.
//!
//! SDK-side mirror of `AYIN-DEV/ayin/src/semconv/lasdlc.rs`. Per LASDLC v2.4.2
//! §7.7 D8 sub-component coverage + `manifest.yaml#observability_contract`. 5 spans
//! declared, each with required attribute keys following OpenTelemetry semantic
//! conventions (W3C-compliant via Trace Context).
//!
//! Attribute keys follow the `lasdlc.<domain>.<action>` namespace convention. They
//! are emitted as nested JSON keys within [`crate::ayin::span::TraceContext::metadata`]
//! to avoid an opentelemetry-semantic-conventions crate dependency.
//!
//! References:
//! - W3C Trace Context: <https://www.w3.org/TR/trace-context/>
//! - OpenTelemetry Semantic Conventions: <https://opentelemetry.io/docs/specs/semconv/>

// =============================================================================
// Span name constants (5 spans per manifest.yaml#observability_contract)
// =============================================================================

/// Per-assertion gate evaluation span.
///
/// Required attrs: [`ATTR_ASSERTION_ID`], [`ATTR_VALIDATION_STATUS`],
/// [`ATTR_CONFIDENCE_VALUE`], [`ATTR_CITATIONS_COUNT`], [`ATTR_CACHE_PATH_RESOLVED`].
///
/// Signal latency budget: 100ms p95.
pub const SPAN_ASSERTION_EVALUATE: &str = "lasdlc.assertion.evaluate";

/// Operator resolve action span (resolve-blocked-flow).
///
/// Required attrs: [`ATTR_ASSERTION_ID`], [`ATTR_ACTION_TYPE`], [`ATTR_OPERATOR_ID`],
/// [`ATTR_DURATION_MS`], [`ATTR_TRACE_CONTEXT_HMAC`].
///
/// Signal latency budget: 200ms p95.
pub const SPAN_ASSERTION_RESOLVE: &str = "lasdlc.assertion.resolve";

/// UI dashboard render span.
///
/// Required attrs: [`ATTR_ROUTE`], [`ATTR_BUILD_ID`], [`ATTR_RENDER_TIME_MS`],
/// [`ATTR_ASSERTION_COUNT`].
///
/// Signal latency budget: 500ms p95.
pub const SPAN_DASHBOARD_RENDER: &str = "lasdlc.dashboard.render";

/// Operator click → action interaction span.
///
/// Required attrs: [`ATTR_INTERACTION_TYPE`], [`ATTR_TARGET_ASSERTION_ID`], [`ATTR_LATENCY_MS`].
///
/// Signal latency budget: 200ms p95.
pub const SPAN_DASHBOARD_INTERACTION: &str = "lasdlc.dashboard.interaction";

/// Hook execution span (`PreToolUse` / `PostToolUse` from gateway governance).
///
/// Required attrs: [`ATTR_HOOK_NAME`], [`ATTR_DECISION_CLASS`], [`ATTR_BLOCKED`],
/// [`ATTR_VALIDATION_STATUS_EMITTED`].
///
/// Signal latency budget: 50ms p95.
pub const SPAN_HOOK_FIRE: &str = "lasdlc.hook.fire";

// =============================================================================
// Attribute key constants (shared across spans)
// =============================================================================

/// Identifier of the assertion being evaluated, resolved, or referenced.
pub const ATTR_ASSERTION_ID: &str = "lasdlc.assertion.id";

/// Validation status enum value: VALIDATED | `INSUFFICIENT_EVIDENCE` | UNVALIDATED |
/// DISPUTED | EXEMPT | `N_A`. Per LASDLC §0.6 `confidence_threshold_gate`.
pub const ATTR_VALIDATION_STATUS: &str = "lasdlc.validation_status";

/// Confidence value (numeric 0-100 OR interval `<low>-<high> (point ~<X>)` string).
/// Per Canon XXXIV interval reporting discipline.
pub const ATTR_CONFIDENCE_VALUE: &str = "lasdlc.confidence_value";

/// Count of `primary_source_citations`[] entries on the assertion.
pub const ATTR_CITATIONS_COUNT: &str = "lasdlc.citations.count";

/// Whether the `cache_path` on each citation actually resolves on disk (boolean).
pub const ATTR_CACHE_PATH_RESOLVED: &str = "lasdlc.cache_path.resolved";

/// Operator action type for resolve span:
/// `provide_citation | escalate_to_research_tier | accept_unvalidated_with_rationale | dispute`.
pub const ATTR_ACTION_TYPE: &str = "lasdlc.action_type";

/// Operator identifier (session-bound).
pub const ATTR_OPERATOR_ID: &str = "lasdlc.operator_id";

/// Span duration in milliseconds.
pub const ATTR_DURATION_MS: &str = "lasdlc.duration_ms";

/// HMAC signature on operator-initiated trace context (SERAPH M4 + cross-repo IPC trust).
///
/// **WAVE-3.1 DEFERRAL**: this constant is declared so callers + downstream consumers
/// can reference it as a key, but AYIN does NOT generate, verify, or enforce the
/// presence of this attribute in Wave 3.1. Values supplied here are stored verbatim
/// with no integrity validation until Wave 3.2 gateway ratification.
///
/// HMAC computation/verification is handled by Wave 3.2 (gateway
/// `src/security/hmac.rs`). Consumers MUST treat this as untrusted-passthrough
/// until the loopback-only acceptance window clears.
pub const ATTR_TRACE_CONTEXT_HMAC: &str = "lasdlc.trace_context.hmac";

/// Webshell route path.
pub const ATTR_ROUTE: &str = "lasdlc.route";

/// Build codename / identifier.
pub const ATTR_BUILD_ID: &str = "lasdlc.build_id";

/// Dashboard render time in milliseconds.
pub const ATTR_RENDER_TIME_MS: &str = "lasdlc.render_time_ms";

/// Count of assertions rendered in dashboard.
pub const ATTR_ASSERTION_COUNT: &str = "lasdlc.assertion.count";

/// UI interaction type: click | hover | focus | `resolve_open` | `drawer_open`.
pub const ATTR_INTERACTION_TYPE: &str = "lasdlc.interaction.type";

/// Assertion ID targeted by interaction.
pub const ATTR_TARGET_ASSERTION_ID: &str = "lasdlc.target_assertion.id";

/// Interaction latency in milliseconds.
pub const ATTR_LATENCY_MS: &str = "lasdlc.latency_ms";

/// Hook name (e.g. `PreToolUse:Assertion_ConfidenceThresholdGate`).
pub const ATTR_HOOK_NAME: &str = "lasdlc.hook.name";

/// Decision class: architecture | design | algorithm | empirical | security |
/// performance | `standards_compliance` | `data_modeling`.
pub const ATTR_DECISION_CLASS: &str = "lasdlc.decision_class";

/// Whether the hook BLOCKED the action (boolean).
pub const ATTR_BLOCKED: &str = "lasdlc.blocked";

/// Validation status emitted by the hook (same enum as [`ATTR_VALIDATION_STATUS`]).
pub const ATTR_VALIDATION_STATUS_EMITTED: &str = "lasdlc.validation_status.emitted";

// =============================================================================
// Helpers
// =============================================================================

/// Returns `true` if `key` is a valid LASDLC semconv attribute key (must start with
/// `lasdlc.`).
#[must_use]
pub fn is_lasdlc_key(key: &str) -> bool {
    key.starts_with("lasdlc.")
}

/// Returns the canonical list of recognized LASDLC span names.
#[must_use]
pub fn known_spans() -> &'static [&'static str] {
    &[
        SPAN_ASSERTION_EVALUATE,
        SPAN_ASSERTION_RESOLVE,
        SPAN_DASHBOARD_RENDER,
        SPAN_DASHBOARD_INTERACTION,
        SPAN_HOOK_FIRE,
    ]
}

/// Returns `true` if `span_name` is a recognized LASDLC span.
#[must_use]
pub fn is_known_span(span_name: &str) -> bool {
    known_spans().contains(&span_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_names_have_lasdlc_prefix() {
        for span in known_spans() {
            assert!(
                span.starts_with("lasdlc."),
                "span {span} missing lasdlc. prefix"
            );
        }
    }

    #[test]
    fn known_spans_contains_five_entries() {
        assert_eq!(
            known_spans().len(),
            5,
            "manifest.yaml#observability_contract.spans declares exactly 5"
        );
    }

    #[test]
    fn is_lasdlc_key_validates_prefix() {
        assert!(is_lasdlc_key("lasdlc.assertion.id"));
        assert!(is_lasdlc_key("lasdlc.validation_status"));
        assert!(!is_lasdlc_key("foo.bar"));
        assert!(!is_lasdlc_key("assertion.id"));
        assert!(!is_lasdlc_key(""));
    }

    #[test]
    fn is_known_span_recognizes_all_five() {
        assert!(is_known_span(SPAN_ASSERTION_EVALUATE));
        assert!(is_known_span(SPAN_ASSERTION_RESOLVE));
        assert!(is_known_span(SPAN_DASHBOARD_RENDER));
        assert!(is_known_span(SPAN_DASHBOARD_INTERACTION));
        assert!(is_known_span(SPAN_HOOK_FIRE));
        assert!(!is_known_span("lasdlc.unknown.span"));
        assert!(!is_known_span("ayin.guard"));
    }
}
