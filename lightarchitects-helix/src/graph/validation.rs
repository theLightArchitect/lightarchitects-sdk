//! Cypher label and relationship type validation.
//!
//! # Security Mandate (GUARD)
//!
//! Cypher labels and relationship types CANNOT be parameterized in Neo4j.
//! They must be interpolated into query strings, creating injection risk.
//! This module provides allowlist validation to ensure only known-safe
//! values reach Cypher construction.
//!
//! ALL labels and relationship types MUST pass validation before use
//! in any Cypher query string.

use super::{GraphError, GraphResult};

// ============================================================================
// Default Allowlists (SOUL vault vocabulary)
// ============================================================================

/// Default allowed node labels for the SOUL vault.
pub const DEFAULT_LABELS: &[&str] = &[
    "Note",
    "HelixEntry",
    "Tag",
    "Strand",
    "Emotion",
    "Theme",
    "Journal",
    "SchemaMigration",
];

/// Default allowed relationship types for the SOUL vault.
pub const DEFAULT_REL_TYPES: &[&str] = &[
    "LINKS_TO",
    "HAS_TAG",
    "HAS_STRAND",
    "HAS_EMOTION",
    "HAS_THEME",
    "REFERENCES",
    "NEXT",
    "PREVIOUS",
];

// ============================================================================
// Validation Functions
// ============================================================================

/// Validates that a label is in the allowed set.
///
/// # Errors
///
/// Returns [`GraphError::Validation`] if the label is not allowlisted
/// or contains unsafe characters.
pub fn validate_label(label: &str, allowed: &[&str]) -> GraphResult<()> {
    if !is_safe_identifier(label) {
        return Err(GraphError::Validation(format!(
            "Label '{label}' contains unsafe characters. Only alphanumeric and underscore allowed."
        )));
    }
    if allowed.contains(&label) {
        Ok(())
    } else {
        Err(GraphError::Validation(format!(
            "Label '{label}' is not in the allowlist. Allowed: {allowed:?}"
        )))
    }
}

/// Validates that a relationship type is in the allowed set.
///
/// # Errors
///
/// Returns [`GraphError::Validation`] if the type is not allowlisted
/// or contains unsafe characters.
pub fn validate_rel_type(rel_type: &str, allowed: &[&str]) -> GraphResult<()> {
    if !is_safe_identifier(rel_type) {
        return Err(GraphError::Validation(format!(
            "Relationship type '{rel_type}' contains unsafe characters. Only alphanumeric and underscore allowed."
        )));
    }
    if allowed.contains(&rel_type) {
        Ok(())
    } else {
        Err(GraphError::Validation(format!(
            "Relationship type '{rel_type}' is not in the allowlist. Allowed: {allowed:?}"
        )))
    }
}

/// Validates all labels in a slice against the allowlist.
///
/// # Errors
///
/// Returns [`GraphError::Validation`] on the first invalid label.
pub fn validate_labels(labels: &[String], allowed: &[&str]) -> GraphResult<()> {
    for label in labels {
        validate_label(label, allowed)?;
    }
    Ok(())
}

/// Checks whether a string is a safe identifier (alphanumeric + underscore).
///
/// Used as defense-in-depth even after allowlist validation passes.
#[must_use]
pub fn is_safe_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_label() {
        assert!(validate_label("Note", DEFAULT_LABELS).is_ok());
        assert!(validate_label("HelixEntry", DEFAULT_LABELS).is_ok());
        assert!(validate_label("SchemaMigration", DEFAULT_LABELS).is_ok());
    }

    #[test]
    fn test_invalid_label() {
        let err = validate_label("MaliciousLabel", DEFAULT_LABELS);
        assert!(err.is_err());
        assert!(
            err.unwrap_err()
                .to_string()
                .contains("not in the allowlist")
        );
    }

    #[test]
    fn test_unsafe_label_characters() {
        let err = validate_label("Note; DROP", DEFAULT_LABELS);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("unsafe characters"));
    }

    #[test]
    fn test_empty_label() {
        let err = validate_label("", DEFAULT_LABELS);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("unsafe characters"));
    }

    #[test]
    fn test_valid_rel_type() {
        assert!(validate_rel_type("LINKS_TO", DEFAULT_REL_TYPES).is_ok());
        assert!(validate_rel_type("HAS_TAG", DEFAULT_REL_TYPES).is_ok());
    }

    #[test]
    fn test_invalid_rel_type() {
        let err = validate_rel_type("EVIL_REL", DEFAULT_REL_TYPES);
        assert!(err.is_err());
    }

    #[test]
    fn test_unsafe_rel_type_characters() {
        let err = validate_rel_type("HAS_TAG}]->()", DEFAULT_REL_TYPES);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("unsafe characters"));
    }

    #[test]
    fn test_validate_labels_all_valid() {
        let labels = vec!["Note".into(), "HelixEntry".into()];
        assert!(validate_labels(&labels, DEFAULT_LABELS).is_ok());
    }

    #[test]
    fn test_validate_labels_one_invalid() {
        let labels = vec!["Note".into(), "BadLabel".into()];
        assert!(validate_labels(&labels, DEFAULT_LABELS).is_err());
    }

    #[test]
    fn test_safe_identifier() {
        assert!(is_safe_identifier("Note"));
        assert!(is_safe_identifier("LINKS_TO"));
        assert!(is_safe_identifier("HelixEntry"));
        assert!(is_safe_identifier("SchemaMigration"));
        assert!(!is_safe_identifier(""));
        assert!(!is_safe_identifier("has space"));
        assert!(!is_safe_identifier("semi;colon"));
        assert!(!is_safe_identifier("bracket}"));
        assert!(!is_safe_identifier("quote'"));
    }

    #[test]
    fn test_custom_allowlist() {
        let custom = &["Bible", "Verse", "Chapter"];
        assert!(validate_label("Bible", custom).is_ok());
        assert!(validate_label("Note", custom).is_err());
    }
}
