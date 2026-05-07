//! Integration tests for Wave 3.2 `ScopeGovernor` 5-gate + `validate_citations`.
//!
//! Covers all gate combinations (TTL / target / tool / concurrent / domain)
//! and citation staleness check per LASDLC v2.4.2 `gates.yaml#phase_3_implement`.
//!
//! Canon XXVII test pyramid: integration tier.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lightarchitects_gateway::governance::{
    CitationCheckResult, ScopeGovernorContext, enforce_operator_action, validate_citations,
};

fn valid_ctx() -> ScopeGovernorContext {
    ScopeGovernorContext {
        operator_id: "op-kft".into(),
        build_id: "luminous-confidence-portal".into(),
        tool: "resolve-assertion".into(),
        timestamp_iso8601: chrono::Utc::now().to_rfc3339(),
        authorized_builds: vec!["luminous-confidence-portal".into()],
        allowed_tools: Vec::new(), // empty = unrestricted
        concurrent_count: 0,
        concurrent_limit: 5,
    }
}

// ── Gate 1: TTL ───────────────────────────────────────────────────────────────

#[test]
fn gate1_recent_timestamp_passes() {
    let ctx = valid_ctx();
    assert!(enforce_operator_action(&ctx).is_ok());
}

#[test]
fn gate1_expired_timestamp_blocked() {
    let old = (chrono::Utc::now() - chrono::Duration::seconds(400)).to_rfc3339();
    let ctx = ScopeGovernorContext {
        timestamp_iso8601: old,
        ..valid_ctx()
    };
    let err = enforce_operator_action(&ctx).unwrap_err();
    assert!(err.to_string().contains("TTL"));
}

#[test]
fn gate1_invalid_timestamp_returns_governance_error() {
    let ctx = ScopeGovernorContext {
        timestamp_iso8601: "not-a-date".into(),
        ..valid_ctx()
    };
    assert!(enforce_operator_action(&ctx).is_err());
}

// ── Gate 2: Target ────────────────────────────────────────────────────────────

#[test]
fn gate2_authorized_build_passes() {
    let ctx = valid_ctx();
    assert!(enforce_operator_action(&ctx).is_ok());
}

#[test]
fn gate2_unauthorized_build_blocked() {
    let ctx = ScopeGovernorContext {
        build_id: "other-build".into(),
        authorized_builds: vec!["luminous-confidence-portal".into()],
        ..valid_ctx()
    };
    let err = enforce_operator_action(&ctx).unwrap_err();
    assert!(err.to_string().contains("Target"));
}

#[test]
fn gate2_empty_authorized_builds_allows_any() {
    let ctx = ScopeGovernorContext {
        authorized_builds: Vec::new(),
        build_id: "any-build-id".into(),
        ..valid_ctx()
    };
    assert!(enforce_operator_action(&ctx).is_ok());
}

// ── Gate 3: Tool ──────────────────────────────────────────────────────────────

#[test]
fn gate3_allowed_tool_passes() {
    let ctx = ScopeGovernorContext {
        allowed_tools: vec!["resolve-assertion".into()],
        ..valid_ctx()
    };
    assert!(enforce_operator_action(&ctx).is_ok());
}

#[test]
fn gate3_disallowed_tool_blocked() {
    let ctx = ScopeGovernorContext {
        tool: "bash".into(),
        allowed_tools: vec!["resolve-assertion".into()],
        ..valid_ctx()
    };
    let err = enforce_operator_action(&ctx).unwrap_err();
    assert!(err.to_string().contains("Tool"));
}

#[test]
fn gate3_empty_allowed_tools_unrestricted() {
    let ctx = ScopeGovernorContext {
        allowed_tools: Vec::new(),
        ..valid_ctx()
    };
    assert!(enforce_operator_action(&ctx).is_ok());
}

// ── Gate 4: Concurrent ────────────────────────────────────────────────────────

#[test]
fn gate4_under_limit_passes() {
    let ctx = ScopeGovernorContext {
        concurrent_count: 4,
        concurrent_limit: 5,
        ..valid_ctx()
    };
    assert!(enforce_operator_action(&ctx).is_ok());
}

#[test]
fn gate4_at_limit_blocked() {
    let ctx = ScopeGovernorContext {
        concurrent_count: 5,
        concurrent_limit: 5,
        ..valid_ctx()
    };
    let err = enforce_operator_action(&ctx).unwrap_err();
    assert!(err.to_string().contains("Concurrent"));
}

#[test]
fn gate4_over_limit_blocked() {
    let ctx = ScopeGovernorContext {
        concurrent_count: 10,
        concurrent_limit: 5,
        ..valid_ctx()
    };
    assert!(enforce_operator_action(&ctx).is_err());
}

// ── Gate 5: Domain ────────────────────────────────────────────────────────────

#[test]
fn gate5_resolve_assertion_in_domain() {
    let ctx = ScopeGovernorContext {
        tool: "resolve-assertion".into(),
        ..valid_ctx()
    };
    assert!(enforce_operator_action(&ctx).is_ok());
}

#[test]
fn gate5_query_blocked_flow_in_domain() {
    let ctx = ScopeGovernorContext {
        tool: "query-blocked-flow".into(),
        ..valid_ctx()
    };
    assert!(enforce_operator_action(&ctx).is_ok());
}

#[test]
fn gate5_deploy_outside_operator_domain() {
    let ctx = ScopeGovernorContext {
        tool: "deploy".into(),
        ..valid_ctx()
    };
    let err = enforce_operator_action(&ctx).unwrap_err();
    assert!(err.to_string().contains("Domain"));
}

// ── validate_citations ────────────────────────────────────────────────────────

#[test]
fn citations_existing_file_resolves() {
    // Use a known path that exists on any system.
    let result: CitationCheckResult = validate_citations(&["/etc/hosts"]);
    assert!(result.resolved.contains(&"/etc/hosts".to_owned()));
    assert!(result.unresolved.is_empty());
    assert!(result.all_resolved());
}

#[test]
fn citations_missing_file_unresolved() {
    let result = validate_citations(&["/nonexistent/path/citation.md"]);
    assert!(
        result
            .unresolved
            .contains(&"/nonexistent/path/citation.md".to_owned())
    );
    assert!(!result.all_resolved());
}

#[test]
fn citations_mixed_resolved_and_unresolved() {
    let result = validate_citations(&["/etc/hosts", "/nonexistent/path/missing.md"]);
    assert_eq!(result.resolved.len(), 1);
    assert_eq!(result.unresolved.len(), 1);
    assert!(!result.all_resolved());
}

#[test]
fn citations_empty_input_all_resolved() {
    let result = validate_citations(&[]);
    assert!(result.all_resolved());
    assert!(result.resolved.is_empty());
}
