//! HMAC bearer token validation.
//!
//! Mirrors the pattern used by `lÆx0-cli/src/web/mod.rs:109-141`:
//! tokens are compared in constant time via [`constant_time_eq`] to avoid
//! timing side-channel attacks, and the `Bearer` scheme is parsed
//! case-insensitively (per RFC 7235).
//!
//! Two auth surfaces share this validator:
//!
//! - **HTTP SSE** — tokens travel in the `Authorization: Bearer <token>`
//!   header (Phase 5).
//! - **WebSocket terminal** — tokens travel in a
//!   `Sec-WebSocket-Protocol: bearer.<token>` sub-protocol
//!   because browsers cannot set `Authorization` on `new WebSocket()`.
//!   The sub-protocol extractor lives in Phase 2; this module exposes the
//!   underlying constant-time comparator both surfaces call.

use constant_time_eq::constant_time_eq;

/// Validates an `Authorization: Bearer <token>` header value against
/// the expected token in constant time.
///
/// Returns `false` if the header does not begin with `Bearer `
/// (case-insensitive) or the token body does not match exactly.
#[must_use]
pub fn validate_bearer(header: &str, expected_token: &str) -> bool {
    let Some(candidate) = extract_bearer(header) else {
        return false;
    };

    if candidate.is_empty() {
        return false;
    }

    constant_time_eq(candidate.as_bytes(), expected_token.as_bytes())
}

/// Extracts the token body from a `Bearer <token>` header.
///
/// Case-insensitive on the scheme per RFC 7235 §2.1 (auth-scheme token is
/// case-insensitive). Only the scheme prefix is lowercased — the token body
/// is returned verbatim from the original string to preserve its case.
fn extract_bearer(header: &str) -> Option<&str> {
    const SCHEME_LEN: usize = 7; // b e a r e r SPACE
    let trimmed = header.trim();
    if trimmed.len() < SCHEME_LEN {
        return None;
    }
    // Compare only the scheme portion (first 7 bytes) case-insensitively.
    // Slicing by byte offset is safe: "bearer " is pure ASCII.
    if !trimmed[..SCHEME_LEN].eq_ignore_ascii_case("bearer ") {
        return None;
    }
    Some(trimmed[SCHEME_LEN..].trim())
}

/// Validates the bearer sub-protocol form used by WebSocket upgrade.
///
/// Expected value: `bearer.<token>`. Matches the pattern from the nautilus
/// plan's Phase 2 spec and the reasoning in
/// `luminous-weaving-nautilus/plan.md` — browsers can't set
/// `Authorization` on `new WebSocket()`, and query-string tokens leak
/// through access logs. The sub-protocol header is the narrowest channel
/// that works across browsers.
#[must_use]
pub fn validate_ws_subprotocol(subprotocol: &str, expected_token: &str) -> bool {
    const PREFIX_LEN: usize = 7; // b e a r e r .
    if subprotocol.len() < PREFIX_LEN {
        return false;
    }
    // RFC 7235 §2.1: auth-scheme is case-insensitive — same rule as extract_bearer.
    // Slicing by byte offset is safe: "bearer." is pure ASCII.
    if !subprotocol[..PREFIX_LEN].eq_ignore_ascii_case("bearer.") {
        return false;
    }
    let candidate = &subprotocol[PREFIX_LEN..];
    if candidate.is_empty() {
        return false;
    }
    constant_time_eq(candidate.as_bytes(), expected_token.as_bytes())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn bearer_accepts_exact_match() {
        assert!(validate_bearer("Bearer abc123", "abc123"));
    }

    #[test]
    fn bearer_accepts_lowercase_scheme() {
        assert!(validate_bearer("bearer abc123", "abc123"));
    }

    #[test]
    fn bearer_trims_whitespace() {
        assert!(validate_bearer("  Bearer   abc123  ", "abc123"));
    }

    #[test]
    fn bearer_rejects_missing_scheme() {
        assert!(!validate_bearer("abc123", "abc123"));
    }

    #[test]
    fn bearer_rejects_wrong_token() {
        assert!(!validate_bearer("Bearer wrong", "abc123"));
    }

    #[test]
    fn bearer_rejects_empty_body() {
        assert!(!validate_bearer("Bearer ", "abc123"));
        assert!(!validate_bearer("Bearer", "abc123"));
    }

    #[test]
    fn bearer_accepts_uppercase_scheme() {
        // RFC 7235 §2.1: auth-scheme is case-insensitive.
        assert!(validate_bearer("BEARER abc123", "abc123"));
        assert!(validate_bearer("BeArEr abc123", "abc123"));
    }

    #[test]
    fn ws_accepts_exact_match() {
        assert!(validate_ws_subprotocol("bearer.abc123", "abc123"));
    }

    #[test]
    fn ws_rejects_missing_prefix() {
        assert!(!validate_ws_subprotocol("abc123", "abc123"));
    }

    #[test]
    fn ws_rejects_wrong_token() {
        assert!(!validate_ws_subprotocol("bearer.wrong", "abc123"));
    }

    #[test]
    fn ws_rejects_empty_body() {
        assert!(!validate_ws_subprotocol("bearer.", "abc123"));
    }

    #[test]
    fn ws_accepts_uppercase_prefix() {
        // RFC 7235 §2.1: auth-scheme is case-insensitive.
        assert!(validate_ws_subprotocol("Bearer.abc123", "abc123"));
        assert!(validate_ws_subprotocol("BEARER.abc123", "abc123"));
    }
}
