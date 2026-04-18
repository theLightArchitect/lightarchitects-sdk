//! CSPRNG wrappers — centralized random byte generation.

use rand::RngCore;

/// Generate `len` bytes of cryptographic randomness.
///
/// # Examples
///
/// ```
/// use lightarchitects::crypto::random::generate_bytes;
///
/// let bytes = generate_bytes(32);
/// assert_eq!(bytes.len(), 32);
/// ```
#[must_use]
pub fn generate_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

/// Generate `len` random bytes and return as a hex-encoded string.
///
/// # Examples
///
/// ```
/// use lightarchitects::crypto::random::generate_hex;
///
/// let hex = generate_hex(16);
/// assert_eq!(hex.len(), 32); // 16 bytes = 32 hex chars
/// assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
/// ```
#[must_use]
pub fn generate_hex(len: usize) -> String {
    let bytes = generate_bytes(len);
    hex_encode(&bytes)
}

/// Generate a 96-bit (12-byte) nonce suitable for AES-256-GCM.
///
/// Each nonce MUST be unique per (key, message) pair. Never reuse nonces.
///
/// # Examples
///
/// ```
/// use lightarchitects::crypto::random::generate_nonce;
///
/// let nonce = generate_nonce();
/// assert_eq!(nonce.len(), 12);
/// ```
#[must_use]
pub fn generate_nonce() -> [u8; 12] {
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    nonce
}

/// Encode bytes as lowercase hex string.
pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Decode a hex string into bytes. Returns `None` on invalid hex.
///
/// Used by downstream modules and tests for hash verification.
#[allow(dead_code)]
pub(crate) fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut chars = hex.chars();
    while let (Some(hi), Some(lo)) = (chars.next(), chars.next()) {
        // to_digit(16) returns 0..=15 — always fits in u8.
        #[allow(clippy::cast_possible_truncation)]
        let byte = (hi.to_digit(16)? as u8) << 4 | lo.to_digit(16)? as u8;
        bytes.push(byte);
    }
    Some(bytes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_bytes_length() {
        assert_eq!(generate_bytes(32).len(), 32);
        assert_eq!(generate_bytes(0).len(), 0);
        assert_eq!(generate_bytes(64).len(), 64);
    }

    #[test]
    fn test_generate_bytes_randomness() {
        let a = generate_bytes(32);
        let b = generate_bytes(32);
        assert_ne!(a, b, "two random 32-byte sequences should differ");
    }

    #[test]
    fn test_generate_hex_length() {
        let hex = generate_hex(32);
        assert_eq!(hex.len(), 64, "32 bytes = 64 hex chars");
    }

    #[test]
    fn test_generate_hex_valid_hex() {
        let hex = generate_hex(16);
        assert!(
            hex.chars().all(|c| c.is_ascii_hexdigit()),
            "output should be valid hex"
        );
    }

    #[test]
    fn test_generate_nonce_length() {
        let nonce = generate_nonce();
        assert_eq!(nonce.len(), 12, "AES-GCM nonce is 12 bytes");
    }

    #[test]
    fn test_hex_encode_known() {
        assert_eq!(hex_encode(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
        assert_eq!(hex_encode(&[0x00, 0xff]), "00ff");
        assert_eq!(hex_encode(&[]), "");
    }

    #[test]
    fn test_hex_decode_known() {
        assert_eq!(hex_decode("deadbeef"), Some(vec![0xde, 0xad, 0xbe, 0xef]));
        assert_eq!(hex_decode("00ff"), Some(vec![0x00, 0xff]));
        assert_eq!(hex_decode(""), Some(vec![]));
    }

    #[test]
    fn test_hex_decode_invalid() {
        assert_eq!(hex_decode("xyz"), None);
        assert_eq!(hex_decode("0"), None); // odd length
    }

    #[test]
    fn test_hex_roundtrip() {
        let original = generate_bytes(32);
        let encoded = hex_encode(&original);
        let decoded = hex_decode(&encoded).expect("valid hex");
        assert_eq!(original, decoded);
    }
}
