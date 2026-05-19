//! W3C [`TraceParent`] carrier for lightsquad spans.
//!
//! Implements the [W3C Trace Context specification](https://www.w3.org/TR/trace-context/)
//! `traceparent` header format: `{version}-{trace-id}-{parent-id}-{flags}`.
//!
//! # Format
//!
//! ```text
//! 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
//! ```
//!
//! - `version`: always `00` for this implementation.
//! - `trace-id`: 16-byte (32 hex chars) random identifier for the entire trace.
//! - `parent-id`: 8-byte (16 hex chars) identifier for the originating span.
//! - `flags`: sampling flags — `01` = sampled, `00` = not sampled.

use rand::Rng as _;
use thiserror::Error;

/// Error type for [`TraceParent`] parse failures.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum TraceParentError {
    /// The input string does not have exactly four dash-separated segments.
    #[error("invalid traceparent format: expected 4 dash-separated fields, got {0}")]
    InvalidFormat(usize),

    /// The `version` field is not the two-hex-char string `"00"`.
    #[error("unsupported traceparent version: {0:?}")]
    UnsupportedVersion(String),

    /// The `trace-id` field is not a valid 32-char hex string.
    #[error("invalid trace-id: {0:?}")]
    InvalidTraceId(String),

    /// The `parent-id` field is not a valid 16-char hex string.
    #[error("invalid parent-id: {0:?}")]
    InvalidParentId(String),

    /// The `flags` field is not a valid 2-char hex string.
    #[error("invalid flags: {0:?}")]
    InvalidFlags(String),
}

/// W3C Trace Context `traceparent` carrier.
///
/// Holds all four fields of the `traceparent` header and provides ergonomic
/// helpers for generating fresh spans and propagating trace context across
/// lightsquad worker boundaries.
///
/// # Example
///
/// ```
/// use lightarchitects::observability::traceparent::TraceParent;
///
/// let root = TraceParent::new();
/// let child = root.child_span();
///
/// // Same trace, different span.
/// assert_eq!(root.trace_id(), child.trace_id());
/// assert_ne!(root.parent_id(), child.parent_id());
///
/// let header = root.to_header_value();
/// let parsed = TraceParent::parse(&header).unwrap();
/// assert_eq!(root, parsed);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TraceParent {
    /// W3C version byte — always `0x00` for this implementation.
    version: u8,
    /// 16-byte random trace identifier shared by all spans in one logical trace.
    trace_id: [u8; 16],
    /// 8-byte span identifier for the originating span in this hop.
    parent_id: [u8; 8],
    /// Sampling flags. `0x01` = sampled; `0x00` = not sampled.
    flags: u8,
}

impl TraceParent {
    /// Generate a fresh [`TraceParent`] with a random `trace-id` and `parent-id`.
    ///
    /// The `version` is set to `0x00` and `flags` to `0x01` (sampled).
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            version: 0x00,
            trace_id: rng.r#gen(),
            parent_id: rng.r#gen(),
            flags: 0x01,
        }
    }

    /// Create a child span that shares the same `trace-id` but has a fresh `parent-id`.
    ///
    /// Use this when fanning a trace out to a sub-span (e.g. one per lightsquad
    /// worker slot in a wave).
    #[must_use]
    pub fn child_span(&self) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            version: self.version,
            trace_id: self.trace_id,
            parent_id: rng.r#gen(),
            flags: self.flags,
        }
    }

    /// Parse a W3C `traceparent` header value.
    ///
    /// Accepts `"00-<32-hex>-<16-hex>-<2-hex>"`.
    ///
    /// # Errors
    ///
    /// Returns [`TraceParentError`] if any field is absent, malformed, or if
    /// the version is not `"00"`.
    pub fn parse(s: &str) -> Result<Self, TraceParentError> {
        let parts: Vec<&str> = s.splitn(4, '-').collect();
        if parts.len() != 4 {
            return Err(TraceParentError::InvalidFormat(parts.len()));
        }

        let version_str = parts[0];
        if version_str != "00" {
            return Err(TraceParentError::UnsupportedVersion(version_str.to_owned()));
        }

        let trace_id = parse_hex_16(parts[1])
            .ok_or_else(|| TraceParentError::InvalidTraceId(parts[1].to_owned()))?;

        let parent_id = parse_hex_8(parts[2])
            .ok_or_else(|| TraceParentError::InvalidParentId(parts[2].to_owned()))?;

        let flags = parse_hex_u8(parts[3])
            .ok_or_else(|| TraceParentError::InvalidFlags(parts[3].to_owned()))?;

        Ok(Self {
            version: 0x00,
            trace_id,
            parent_id,
            flags,
        })
    }

    /// Render the `traceparent` as a W3C-compliant header value string.
    ///
    /// Output format: `"00-{32-hex}-{16-hex}-{02-hex}"`.
    pub fn to_header_value(&self) -> String {
        format!(
            "{:02x}-{}-{}-{:02x}",
            self.version,
            hex_encode_16(&self.trace_id),
            hex_encode_8(self.parent_id),
            self.flags,
        )
    }

    /// Return the `trace-id` bytes.
    pub fn trace_id(&self) -> &[u8; 16] {
        &self.trace_id
    }

    /// Return the `parent-id` bytes.
    pub fn parent_id(&self) -> &[u8; 8] {
        &self.parent_id
    }

    /// Return the sampling flags byte.
    pub fn flags(&self) -> u8 {
        self.flags
    }

    /// Return `true` if the sampled flag (`0x01`) is set.
    pub fn is_sampled(&self) -> bool {
        self.flags & 0x01 != 0
    }
}

impl Default for TraceParent {
    fn default() -> Self {
        Self::new()
    }
}

// ── Private hex helpers ───────────────────────────────────────────────────────

fn parse_hex_16(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = hex_digit(chunk[0])?;
        let lo = hex_digit(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn parse_hex_8(s: &str) -> Option<[u8; 8]> {
    if s.len() != 16 {
        return None;
    }
    let mut out = [0u8; 8];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = hex_digit(chunk[0])?;
        let lo = hex_digit(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn parse_hex_u8(s: &str) -> Option<u8> {
    if s.len() != 2 {
        return None;
    }
    let b = s.as_bytes();
    let hi = hex_digit(b[0])?;
    let lo = hex_digit(b[1])?;
    Some((hi << 4) | lo)
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

fn byte_to_hex(b: u8) -> [u8; 2] {
    [HEX_CHARS[(b >> 4) as usize], HEX_CHARS[(b & 0x0f) as usize]]
}

fn hex_encode_16(bytes: &[u8; 16]) -> String {
    let mut out = Vec::with_capacity(32);
    for b in bytes {
        out.extend_from_slice(&byte_to_hex(*b));
    }
    // SAFETY: out contains only ASCII hex chars from HEX_CHARS.
    String::from_utf8(out).unwrap_or_default()
}

fn hex_encode_8(bytes: [u8; 8]) -> String {
    let mut out = Vec::with_capacity(16);
    for b in bytes {
        out.extend_from_slice(&byte_to_hex(b));
    }
    // SAFETY: out contains only ASCII hex chars from HEX_CHARS.
    String::from_utf8(out).unwrap_or_default()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const EXAMPLE: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    #[test]
    fn new_produces_valid_header() {
        let tp = TraceParent::new();
        let header = tp.to_header_value();
        assert!(header.starts_with("00-"), "header must start with version");
        assert_eq!(
            header.len(),
            55,
            "00(2) + dash + 32 + dash + 16 + dash + 02"
        );
    }

    #[test]
    fn parse_roundtrip() {
        let tp = TraceParent::parse(EXAMPLE).unwrap();
        assert_eq!(tp.to_header_value(), EXAMPLE);
    }

    #[test]
    fn new_roundtrip() {
        let tp = TraceParent::new();
        let header = tp.to_header_value();
        let parsed = TraceParent::parse(&header).unwrap();
        assert_eq!(tp, parsed);
    }

    #[test]
    fn child_span_shares_trace_id() {
        let root = TraceParent::new();
        let child = root.child_span();
        assert_eq!(root.trace_id(), child.trace_id());
        assert_ne!(root.parent_id(), child.parent_id());
    }

    #[test]
    fn child_span_preserves_flags() {
        let root = TraceParent::new();
        let child = root.child_span();
        assert_eq!(root.flags(), child.flags());
    }

    #[test]
    fn sampled_flag_detected() {
        let tp = TraceParent::parse(EXAMPLE).unwrap();
        assert!(tp.is_sampled());
    }

    #[test]
    fn not_sampled_flag_detected() {
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00";
        let tp = TraceParent::parse(header).unwrap();
        assert!(!tp.is_sampled());
    }

    #[test]
    fn parse_rejects_wrong_field_count() {
        let err = TraceParent::parse("00-abc-01").unwrap_err();
        assert!(matches!(err, TraceParentError::InvalidFormat(_)));
    }

    #[test]
    fn parse_rejects_bad_version() {
        let err = TraceParent::parse("ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
            .unwrap_err();
        assert!(matches!(err, TraceParentError::UnsupportedVersion(_)));
    }

    #[test]
    fn parse_rejects_short_trace_id() {
        let err = TraceParent::parse("00-4bf92f3577b34da6a3ce929d0e0e47-00f067aa0ba902b7-01")
            .unwrap_err();
        assert!(matches!(err, TraceParentError::InvalidTraceId(_)));
    }

    #[test]
    fn parse_rejects_short_parent_id() {
        let err = TraceParent::parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902-01")
            .unwrap_err();
        assert!(matches!(err, TraceParentError::InvalidParentId(_)));
    }

    #[test]
    fn parse_rejects_invalid_hex_in_trace_id() {
        let err = TraceParent::parse("00-4bf92f3577b34da6a3ce929d0e0e47zz-00f067aa0ba902b7-01")
            .unwrap_err();
        assert!(matches!(err, TraceParentError::InvalidTraceId(_)));
    }

    #[test]
    fn multiple_children_have_unique_parent_ids() {
        let root = TraceParent::new();
        let c1 = root.child_span();
        let c2 = root.child_span();
        // Statistically impossible to collide with 64-bit random IDs.
        assert_ne!(c1.parent_id(), c2.parent_id());
    }

    #[test]
    fn default_equals_new_structurally() {
        // Default produces a valid TraceParent (different random values, but valid).
        let tp = TraceParent::default();
        let header = tp.to_header_value();
        assert!(TraceParent::parse(&header).is_ok());
    }
}
