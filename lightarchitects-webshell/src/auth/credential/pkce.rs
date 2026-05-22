//! PKCE challenge generation per RFC 7636 (OA-1).
//!
//! Generates a 256-bit random verifier and its S256 challenge.
//! No external base64 crate required — the alphabet is 64 chars and the
//! encoding is a straightforward 3-byte-to-4-char mapping.

use rand::RngCore;
use sha2::{Digest, Sha256};

/// Generates a PKCE code verifier and S256 challenge pair.
///
/// # Entropy
///
/// 32 bytes (256 bits) drawn from `rand::thread_rng()` — a CSPRNG seeded
/// from the OS on first use.  This satisfies OA-1: ≥ 256 bits of entropy.
///
/// # Return value
///
/// Returns `(code_verifier, code_challenge)` where:
/// - `code_verifier` is the 43-char base64url encoding of the 32 raw bytes.
/// - `code_challenge` is `BASE64URL-NO-PAD(SHA256(ASCII(code_verifier)))` per
///   RFC 7636 §4.2.
pub fn generate_pkce_pair() -> (String, String) {
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    let verifier = base64url(&raw);
    let challenge = base64url(Sha256::digest(verifier.as_bytes()).as_slice());
    (verifier, challenge)
}

/// Base64url encoding without padding per RFC 4648 §5.
///
/// Uses the URL-safe alphabet (`-` instead of `+`, `_` instead of `/`)
/// and omits `=` padding.  This is the encoding required by RFC 7636 for
/// PKCE code verifiers and challenges.
fn base64url(input: &[u8]) -> String {
    const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let full_triplets = input.len() / 3;
    let remainder = input.len() % 3;
    let out_len = full_triplets * 4 + if remainder == 0 { 0 } else { remainder + 1 };
    let mut out = String::with_capacity(out_len);
    let mut i = 0;
    while i < input.len() {
        let b0 = u32::from(input[i]);
        let b1 = if i + 1 < input.len() {
            u32::from(input[i + 1])
        } else {
            0
        };
        let b2 = if i + 2 < input.len() {
            u32::from(input[i + 2])
        } else {
            0
        };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(char::from(ALPHA[((n >> 18) & 0x3F) as usize]));
        out.push(char::from(ALPHA[((n >> 12) & 0x3F) as usize]));
        if i + 1 < input.len() {
            out.push(char::from(ALPHA[((n >> 6) & 0x3F) as usize]));
        }
        if i + 2 < input.len() {
            out.push(char::from(ALPHA[(n & 0x3F) as usize]));
        }
        i += 3;
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn verifier_is_base64url_no_padding() {
        let (verifier, _) = generate_pkce_pair();
        assert!(
            verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "verifier must only contain base64url chars: {verifier}"
        );
        assert!(
            !verifier.contains('='),
            "verifier must not have padding: {verifier}"
        );
    }

    #[test]
    fn challenge_is_sha256_of_verifier_base64url() {
        let (verifier, challenge) = generate_pkce_pair();
        let expected = base64url(Sha256::digest(verifier.as_bytes()).as_slice());
        assert_eq!(
            challenge, expected,
            "challenge must equal BASE64URL(SHA256(verifier))"
        );
    }

    #[test]
    fn verifier_encodes_32_bytes() {
        // 32 raw bytes → base64url without padding:
        // 10 full triplets → 40 chars, 2 remaining bytes → 3 chars. Total = 43.
        let (verifier, _) = generate_pkce_pair();
        assert_eq!(
            verifier.len(),
            43,
            "32 bytes must encode to 43 base64url chars"
        );
    }

    #[test]
    fn pairs_are_unique_csprng() {
        let (v1, _) = generate_pkce_pair();
        let (v2, _) = generate_pkce_pair();
        assert_ne!(v1, v2, "consecutive PKCE pairs must differ (CSPRNG)");
    }

    #[test]
    fn base64url_known_vector() {
        // RFC 4648 test vector: empty input → empty output.
        assert_eq!(base64url(&[]), "");
        // Single byte 0x00 → "AA" (no padding).
        assert_eq!(base64url(&[0x00]), "AA");
        // Three bytes of 0xFF → "____" (URL-safe alphabet).
        assert_eq!(base64url(&[0xFF, 0xFF, 0xFF]), "____");
    }
}
