//! ETag computation and conditional-GET helpers.
//!
//! ETag is handler-level — handlers compute the tag from the stored
//! `content_hash` (precomputed SHA-256) or from the serialised response body,
//! check `If-None-Match`, and short-circuit to 304 before writing a body.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Compute a lowercase hex SHA-256 digest of `data` (no quoting).
pub fn sha256_hex(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    let mut hex = String::with_capacity(64);
    for b in &hash {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Compute a strong ETag for a byte slice (SHA-256, RFC 7232 quoted).
///
/// Example output: `"a3f1c9..."` (64 hex chars surrounded by double-quotes).
pub fn compute_etag(body: &[u8]) -> String {
    format!("\"{}\"", sha256_hex(body))
}

/// Wrap a precomputed hex hash (no quotes) into a quoted ETag string.
pub fn etag_from_hash(hex: &str) -> String {
    format!("\"{hex}\"")
}

/// Returns `true` if the client already holds a current copy (→ 304 Not Modified).
pub fn is_not_modified(if_none_match: Option<&str>, etag: &str) -> bool {
    if_none_match.is_some_and(|inm| inm == etag || inm == "*")
}
