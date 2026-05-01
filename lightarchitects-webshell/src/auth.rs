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

// ── Cookie-based session auth (v0.4.0) ─────────────────────────────────────

/// Name of the `HttpOnly` session cookie.
const SESSION_COOKIE_NAME: &str = "la_session";

/// Validates a raw token (no scheme prefix) against the expected token in
/// constant time.  Used by the cookie-exchange endpoint where the client
/// sends the bare token in a JSON body.
#[must_use]
pub fn validate_raw_token(provided: &str, expected: &str) -> bool {
    if provided.is_empty() {
        return false;
    }
    constant_time_eq(provided.as_bytes(), expected.as_bytes())
}

/// Extracts the `la_session` value from a `Cookie` request header.
///
/// Parses the standard `name1=val1; name2=val2` format.  The token body is
/// returned verbatim — per RFC 6265 §4.2.1, cookie-value is an opaque byte
/// sequence and must not be trimmed.
#[must_use]
pub fn extract_session_cookie(cookie_header: &str) -> Option<&str> {
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(rest) = pair.strip_prefix(SESSION_COOKIE_NAME) {
            let rest = rest.trim_start();
            if let Some(val) = rest.strip_prefix('=') {
                return Some(val);
            }
        }
    }
    None
}

/// Returns `true` when the `Cookie` header contains a valid `la_session` token.
#[must_use]
pub fn validate_session_cookie(cookie_header: &str, expected_token: &str) -> bool {
    let Some(candidate) = extract_session_cookie(cookie_header) else {
        return false;
    };
    if candidate.is_empty() {
        return false;
    }
    constant_time_eq(candidate.as_bytes(), expected_token.as_bytes())
}

/// Builds a `Set-Cookie` header value for the session cookie.
///
/// Attributes: `HttpOnly` (blocks JS access), `SameSite=Strict` (blocks CSRF),
/// `Path=/`, `Max-Age=28800` (8-hour TTL).
#[must_use]
pub fn session_cookie_header(token: &str) -> String {
    format!("{SESSION_COOKIE_NAME}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=28800")
}

/// Builds a `Set-Cookie` header value that immediately expires the session cookie.
#[must_use]
pub fn clear_session_cookie_header() -> &'static str {
    "la_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"
}

// ── Notify-token validation ─────────────────────────────────────────────────

/// Validates a hex-encoded notify token (from the `X-LA-Notify-Token` header)
/// against the expected 32-byte token stored in a `BuildSession`.
///
/// Accepts only exactly 64 lowercase/uppercase hex characters. Decodes into
/// 32 bytes and compares in constant time. Returns `false` for any length
/// mismatch, invalid hex, or byte-level mismatch.
#[must_use]
pub fn validate_notify_token(provided_hex: &str, expected: &[u8; 32]) -> bool {
    if provided_hex.len() != 64 {
        return false;
    }
    let Some(provided) = decode_hex_32(provided_hex) else {
        return false;
    };
    constant_time_eq(&provided, expected)
}

/// Decode a 64-char hex string into a 32-byte array. Returns `None` if any
/// character is not a valid hex digit.
fn decode_hex_32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = [0u8; 32];
    for i in 0..32 {
        let hi = hex_nibble(bytes[i * 2])?;
        let lo = hex_nibble(bytes[i * 2 + 1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

/// Decode a single hex ASCII byte to its 0-15 numeric value.
fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
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

    // ── cookie session auth ─────────────────────────────────────────────────

    #[test]
    fn raw_token_accepts_match() {
        assert!(validate_raw_token("secret", "secret"));
    }

    #[test]
    fn raw_token_rejects_empty() {
        assert!(!validate_raw_token("", "secret"));
    }

    #[test]
    fn raw_token_rejects_wrong() {
        assert!(!validate_raw_token("wrong", "secret"));
    }

    #[test]
    fn extract_cookie_single() {
        assert_eq!(extract_session_cookie("la_session=abc"), Some("abc"));
    }

    #[test]
    fn extract_cookie_multiple_first() {
        assert_eq!(
            extract_session_cookie("la_session=abc; other=xyz"),
            Some("abc")
        );
    }

    #[test]
    fn extract_cookie_multiple_middle() {
        assert_eq!(
            extract_session_cookie("foo=bar; la_session=abc; baz=qux"),
            Some("abc")
        );
    }

    #[test]
    fn extract_cookie_missing() {
        assert_eq!(extract_session_cookie("other=val"), None);
    }

    #[test]
    fn extract_cookie_prefix_collision() {
        // `la_session_extra` must not match `la_session`
        assert_eq!(extract_session_cookie("la_session_extra=val"), None);
    }

    #[test]
    fn validate_session_cookie_accepts_correct() {
        assert!(validate_session_cookie("la_session=tok123", "tok123"));
    }

    #[test]
    fn validate_session_cookie_rejects_wrong() {
        assert!(!validate_session_cookie("la_session=wrong", "tok123"));
    }

    #[test]
    fn validate_session_cookie_rejects_missing() {
        assert!(!validate_session_cookie("other=val", "tok123"));
    }

    #[test]
    fn session_cookie_header_format() {
        let h = session_cookie_header("abc");
        assert!(h.starts_with("la_session=abc"));
        assert!(h.contains("HttpOnly"));
        assert!(h.contains("SameSite=Strict"));
        assert!(h.contains("Max-Age=28800"));
    }

    #[test]
    fn clear_cookie_header_zeroes_max_age() {
        let h = clear_session_cookie_header();
        assert!(h.contains("Max-Age=0"));
        assert!(h.contains("HttpOnly"));
    }

    // ── validate_notify_token ───────────────────────────────────────────────

    /// Canonical 32-byte value used across the notify-token tests.
    const SAMPLE: [u8; 32] = [
        0xde, 0xad, 0xbe, 0xef, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a,
        0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
        0x1a, 0x1b,
    ];

    const SAMPLE_HEX_LOWER: &str =
        "deadbeef000102030405060708090a0b0c0d0e0f101112131415161718191a1b";

    #[test]
    fn notify_token_accepts_matching_lowercase_hex() {
        assert!(validate_notify_token(SAMPLE_HEX_LOWER, &SAMPLE));
    }

    #[test]
    fn notify_token_accepts_matching_uppercase_hex() {
        let upper = SAMPLE_HEX_LOWER.to_uppercase();
        assert!(validate_notify_token(&upper, &SAMPLE));
    }

    #[test]
    fn notify_token_rejects_wrong_token() {
        use std::fmt::Write as _;
        let mut wrong = SAMPLE;
        wrong[0] ^= 0xFF;
        let mut hex = String::with_capacity(64);
        for b in &wrong {
            write!(hex, "{b:02x}").expect("write to String");
        }
        assert!(!validate_notify_token(&hex, &SAMPLE));
    }

    #[test]
    fn notify_token_rejects_short_input() {
        assert!(!validate_notify_token("abc", &SAMPLE));
        assert!(!validate_notify_token(&SAMPLE_HEX_LOWER[..63], &SAMPLE));
    }

    #[test]
    fn notify_token_rejects_long_input() {
        let too_long = format!("{SAMPLE_HEX_LOWER}aa");
        assert!(!validate_notify_token(&too_long, &SAMPLE));
    }

    #[test]
    fn notify_token_rejects_non_hex_chars() {
        // Same length (64), but with a non-hex char somewhere.
        let bad = format!("z{}", &SAMPLE_HEX_LOWER[1..]);
        assert_eq!(bad.len(), 64);
        assert!(!validate_notify_token(&bad, &SAMPLE));
    }
}
