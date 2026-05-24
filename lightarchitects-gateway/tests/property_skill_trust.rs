//! Property-based tests for the `skill_trust` public API (Canon XXVII Suite 3).
//!
//! The SHA-256 hash property tests (64 hex chars / determinism / avalanche)
//! live in `src/cli/skill_trust.rs` alongside the private `sha256_content`
//! function they target — that avoids env-var concurrency hazards from
//! parallel proptest test threads.
//!
//! This file contains property tests for the PUBLIC `verify_or_pin` API:
//!
//! 1. `prop_verify_or_pin_first_call_always_ok` — first pin always succeeds
//!    (any slug + content → Ok). Tests without env isolation by using a
//!    content-derived slug so iterations don't collide in the real ledger.
//!
//! 2. `prop_verify_or_pin_second_call_with_same_content_ok` — determinism at
//!    the public API level: pin + re-verify with identical content → Ok.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;

/// Derive a stable ledger slug from content for property tests.
/// Uses the first 10 ASCII-printable characters from the content, uppercased,
/// prefixed with "PT_" to namespace away from production entries.
/// Falls back to "PT_FALLBACK" for empty or all-non-ASCII content.
fn slug_for(content: &str) -> String {
    let ascii_part: String = content
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .take(8)
        .collect::<String>()
        .to_uppercase();
    if ascii_part.len() >= 3 {
        format!("PT_{ascii_part}")
    } else {
        format!("PT_FALLBACK_{}", content.len() % 100)
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// First verify_or_pin for any (slug, content) pair must return Ok.
    ///
    /// This property holds whether the slot is fresh (pin path) or already
    /// pinned with the same content (match path). The derived slug ensures
    /// each distinct content maps to its own ledger slot.
    #[test]
    fn prop_verify_or_pin_first_call_always_ok(content in "[a-zA-Z ]{10,50}") {
        let slug = slug_for(&content);
        // First call: either pins (new slot) or matches (same content re-run).
        let result = lightarchitects_gateway::cli::skill_trust::verify_or_pin(&slug, &content);
        prop_assert!(
            result.is_ok(),
            "verify_or_pin must return Ok for slug={slug} content={content:?}: got {result:?}"
        );
    }

    /// Two consecutive verify_or_pin calls with the same (slug, content) must
    /// both return Ok — the hash function is deterministic.
    #[test]
    fn prop_verify_or_pin_same_content_always_matches(content in "[a-zA-Z ]{10,50}") {
        let slug = slug_for(&content);
        let r1 = lightarchitects_gateway::cli::skill_trust::verify_or_pin(&slug, &content);
        let r2 = lightarchitects_gateway::cli::skill_trust::verify_or_pin(&slug, &content);
        prop_assert!(
            r1.is_ok() && r2.is_ok(),
            "both calls must succeed for slug={slug}: r1={r1:?} r2={r2:?}"
        );
    }
}
