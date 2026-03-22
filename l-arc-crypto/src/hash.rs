//! HMAC-SHA256 hashing and webhook signatures.
//!
//! Provides keyed hashing for API key storage and Stripe-pattern webhook
//! signature verification with timestamp tolerance.

use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret, SecretString};
use sha2::Sha256;

use crate::compare::constant_time_eq;
use crate::error::{CryptoError, Result};
use crate::random::hex_encode;

/// HMAC-SHA256 type alias.
type HmacSha256 = Hmac<Sha256>;

/// Compute HMAC-SHA256 of `data` keyed with `pepper`, returning the hex digest.
///
/// The pepper is a [`SecretString`] to enforce secret handling. The output
/// is a lowercase hex string suitable for database storage.
///
/// # Examples
///
/// ```
/// use l_arc_crypto::hash::hmac_hash;
/// use secrecy::SecretString;
///
/// let pepper = SecretString::from("my-pepper");
/// let hash = hmac_hash(&pepper, b"hello world").expect("hmac");
/// assert_eq!(hash.len(), 64); // SHA-256 = 64 hex chars
/// ```
///
/// # Errors
///
/// Returns [`CryptoError::HmacInit`] if the HMAC key is rejected (should
/// not happen with SHA-256, which accepts any key length).
pub fn hmac_hash(pepper: &SecretString, data: &[u8]) -> Result<String> {
    let mut mac = HmacSha256::new_from_slice(pepper.expose_secret().as_bytes())
        .map_err(|e| CryptoError::HmacInit(e.to_string()))?;
    mac.update(data);
    let result = mac.finalize();
    Ok(hex_encode(result.into_bytes().as_ref()))
}

/// Verify an HMAC-SHA256 hash using constant-time comparison.
///
/// Computes the HMAC of `data` with `pepper` and compares it against
/// `expected_hex` using [`crate::compare::constant_time_eq`] to prevent
/// timing side-channels.
///
/// # Examples
///
/// ```
/// use l_arc_crypto::hash::{hmac_hash, hmac_verify};
/// use secrecy::SecretString;
///
/// let pepper = SecretString::from("my-pepper");
/// let hash = hmac_hash(&pepper, b"data").expect("hmac");
/// let valid = hmac_verify(&pepper, b"data", &hash).expect("verify");
/// assert!(valid);
/// ```
///
/// # Errors
///
/// Returns [`CryptoError::HmacInit`] if HMAC initialization fails.
pub fn hmac_verify(pepper: &SecretString, data: &[u8], expected_hex: &str) -> Result<bool> {
    let computed = hmac_hash(pepper, data)?;
    let computed_bytes = computed.as_bytes();
    let expected_bytes = expected_hex.as_bytes();
    Ok(constant_time_eq(computed_bytes, expected_bytes))
}

// ─── Webhook signatures (Stripe pattern) ─────────────────────────────────────

/// Create a Stripe-pattern webhook signature.
///
/// Signs the message `"{timestamp}.{body}"` with HMAC-SHA256 using `secret`,
/// and returns the signature header in the format `"t={timestamp},v1={hex}"`.
///
/// # Arguments
///
/// - `secret`: the webhook signing secret
/// - `body`: the raw request body bytes
/// - `timestamp`: Unix epoch seconds as a string (e.g., `"1700000000"`)
///
/// # Examples
///
/// ```
/// use l_arc_crypto::hash::{webhook_sign, webhook_verify};
/// use secrecy::SecretString;
///
/// let secret = SecretString::from("whsec_test");
/// let sig = webhook_sign(&secret, b"{\"event\":\"ok\"}", "1700000000")
///     .expect("sign");
/// assert!(sig.starts_with("t=1700000000,v1="));
/// ```
///
/// # Errors
///
/// Returns [`CryptoError::HmacInit`] if HMAC initialization fails.
pub fn webhook_sign(secret: &SecretString, body: &[u8], timestamp: &str) -> Result<String> {
    let payload = build_webhook_payload(timestamp, body);
    let sig = hmac_hash(secret, &payload)?;
    Ok(format!("t={timestamp},v1={sig}"))
}

/// Verify a Stripe-pattern webhook signature with timestamp tolerance.
///
/// Parses the `signature` header to extract the timestamp and `v1` HMAC,
/// then verifies both the HMAC (constant-time) and the timestamp freshness.
///
/// # Arguments
///
/// - `secret`: the webhook signing secret
/// - `body`: the raw request body bytes
/// - `signature`: the full signature header (`"t=...,v1=..."`)
/// - `tolerance`: maximum age of the signature in seconds.
///   - `None` = skip timestamp check entirely (disabled)
///   - `Some(300)` = 300-second tolerance window
///   - `Some(0)` = zero tolerance (only accept timestamps matching the current second)
///
/// # Examples
///
/// ```
/// use l_arc_crypto::hash::{webhook_sign, webhook_verify};
/// use secrecy::SecretString;
///
/// let secret = SecretString::from("whsec_test");
/// let sig = webhook_sign(&secret, b"body", "1700000000").expect("sign");
/// // None = skip timestamp check (useful in tests)
/// let valid = webhook_verify(&secret, b"body", &sig, None).expect("verify");
/// assert!(valid);
/// ```
///
/// # Errors
///
/// Returns [`CryptoError::HmacInit`] if HMAC initialization fails.
pub fn webhook_verify(
    secret: &SecretString,
    body: &[u8],
    signature: &str,
    tolerance: Option<u64>,
) -> Result<bool> {
    let Some((ts, sig_hex)) = parse_webhook_signature(signature) else {
        return Ok(false);
    };

    // Check timestamp freshness when a tolerance window is specified.
    if let Some(max_age) = tolerance
        && !is_timestamp_fresh(ts, max_age)
    {
        return Ok(false);
    }

    let ts_str = ts.to_string();
    let payload = build_webhook_payload(&ts_str, body);
    let computed = hmac_hash(secret, &payload)?;

    // Constant-time comparison of hex-encoded signatures.
    Ok(constant_time_eq(computed.as_bytes(), sig_hex.as_bytes()))
}

/// Build the `"{timestamp}.{body}"` payload for webhook signing.
fn build_webhook_payload(timestamp: &str, body: &[u8]) -> Vec<u8> {
    let mut payload =
        Vec::with_capacity(timestamp.len().saturating_add(1).saturating_add(body.len()));
    payload.extend_from_slice(timestamp.as_bytes());
    payload.push(b'.');
    payload.extend_from_slice(body);
    payload
}

/// Parse a webhook signature header into `(timestamp, v1_hex)`.
///
/// Expected format: `"t={unix_secs},v1={hex_digest}"`.
/// Returns `None` if the format is invalid.
fn parse_webhook_signature(header: &str) -> Option<(u64, &str)> {
    let mut timestamp: Option<u64> = None;
    let mut sig_hex: Option<&str> = None;

    for part in header.split(',') {
        if let Some(ts_str) = part.strip_prefix("t=") {
            timestamp = ts_str.parse().ok();
        } else if let Some(hex) = part.strip_prefix("v1=") {
            sig_hex = Some(hex);
        }
    }

    match (timestamp, sig_hex) {
        (Some(ts), Some(hex)) => Some((ts, hex)),
        _ => None,
    }
}

/// Check whether a timestamp is within `tolerance_secs` of the current time.
fn is_timestamp_fresh(timestamp: u64, tolerance_secs: u64) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Compute absolute difference without underflow.
    let diff = if now >= timestamp {
        now.saturating_sub(timestamp)
    } else {
        timestamp.saturating_sub(now)
    };

    diff <= tolerance_secs
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_secret() -> SecretString {
        SecretString::from("test-webhook-secret-for-la-crypto")
    }

    fn test_pepper() -> SecretString {
        SecretString::from("test-pepper-for-la-crypto-unit-tests")
    }

    // ── hmac_hash ────────────────────────────────────────────────────────

    #[test]
    fn test_hmac_hash_produces_hex() {
        let pepper = test_pepper();
        let hash = hmac_hash(&pepper, b"hello world").expect("hmac");
        assert_eq!(hash.len(), 64, "SHA-256 hex digest is 64 chars");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "output should be valid hex"
        );
    }

    #[test]
    fn test_hmac_hash_deterministic() {
        let pepper = test_pepper();
        let a = hmac_hash(&pepper, b"same data").expect("hmac a");
        let b = hmac_hash(&pepper, b"same data").expect("hmac b");
        assert_eq!(a, b, "same input = same HMAC");
    }

    #[test]
    fn test_hmac_hash_different_data() {
        let pepper = test_pepper();
        let a = hmac_hash(&pepper, b"data one").expect("hmac a");
        let b = hmac_hash(&pepper, b"data two").expect("hmac b");
        assert_ne!(a, b, "different data = different HMAC");
    }

    #[test]
    fn test_hmac_hash_different_pepper() {
        let p1 = SecretString::from("pepper-one");
        let p2 = SecretString::from("pepper-two");
        let a = hmac_hash(&p1, b"same data").expect("hmac a");
        let b = hmac_hash(&p2, b"same data").expect("hmac b");
        assert_ne!(a, b, "different pepper = different HMAC");
    }

    #[test]
    fn test_hmac_hash_empty_data() {
        let pepper = test_pepper();
        let hash = hmac_hash(&pepper, b"").expect("hmac");
        assert_eq!(hash.len(), 64);
    }

    // ── hmac_verify ──────────────────────────────────────────────────────

    #[test]
    fn test_hmac_verify_valid() {
        let pepper = test_pepper();
        let hash = hmac_hash(&pepper, b"verify me").expect("hmac");
        let valid = hmac_verify(&pepper, b"verify me", &hash).expect("verify");
        assert!(valid);
    }

    #[test]
    fn test_hmac_verify_invalid_data() {
        let pepper = test_pepper();
        let hash = hmac_hash(&pepper, b"original").expect("hmac");
        let valid = hmac_verify(&pepper, b"tampered", &hash).expect("verify");
        assert!(!valid);
    }

    #[test]
    fn test_hmac_verify_invalid_hash() {
        let pepper = test_pepper();
        let valid = hmac_verify(
            &pepper,
            b"data",
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .expect("verify");
        assert!(!valid);
    }

    #[test]
    fn test_hmac_verify_wrong_pepper() {
        let p1 = SecretString::from("pepper-one");
        let p2 = SecretString::from("pepper-two");
        let hash = hmac_hash(&p1, b"data").expect("hmac");
        let valid = hmac_verify(&p2, b"data", &hash).expect("verify");
        assert!(!valid, "wrong pepper should fail verification");
    }

    // ── webhook_sign ─────────────────────────────────────────────────────

    #[test]
    fn test_webhook_sign_format() {
        let secret = test_secret();
        let sig = webhook_sign(&secret, b"{\"event\":\"test\"}", "1700000000").expect("sign");
        assert!(sig.starts_with("t=1700000000,v1="), "got: {sig}");
        // Extract hex portion after "v1="
        let hex = sig.split("v1=").nth(1).expect("v1= should exist");
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_webhook_sign_deterministic() {
        let secret = test_secret();
        let a = webhook_sign(&secret, b"body", "12345").expect("sign a");
        let b = webhook_sign(&secret, b"body", "12345").expect("sign b");
        assert_eq!(a, b);
    }

    #[test]
    fn test_webhook_sign_different_timestamp() {
        let secret = test_secret();
        let a = webhook_sign(&secret, b"body", "1000").expect("sign a");
        let b = webhook_sign(&secret, b"body", "2000").expect("sign b");
        assert_ne!(a, b, "different timestamp = different signature");
    }

    #[test]
    fn test_webhook_sign_different_body() {
        let secret = test_secret();
        let a = webhook_sign(&secret, b"body-a", "1000").expect("sign a");
        let b = webhook_sign(&secret, b"body-b", "1000").expect("sign b");
        assert_ne!(a, b, "different body = different signature");
    }

    // ── webhook_verify ───────────────────────────────────────────────────

    #[test]
    fn test_webhook_verify_valid() {
        let secret = test_secret();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let ts = now.to_string();
        let sig = webhook_sign(&secret, b"payload", &ts).expect("sign");
        let valid = webhook_verify(&secret, b"payload", &sig, Some(300)).expect("verify");
        assert!(valid);
    }

    #[test]
    fn test_webhook_verify_tampered_body() {
        let secret = test_secret();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let ts = now.to_string();
        let sig = webhook_sign(&secret, b"original", &ts).expect("sign");
        let valid = webhook_verify(&secret, b"tampered", &sig, Some(300)).expect("verify");
        assert!(!valid, "tampered body should fail");
    }

    #[test]
    fn test_webhook_verify_expired() {
        let secret = test_secret();
        // Use a timestamp far in the past.
        let sig = webhook_sign(&secret, b"body", "1000000000").expect("sign");
        let valid = webhook_verify(&secret, b"body", &sig, Some(300)).expect("verify");
        assert!(!valid, "expired timestamp should fail");
    }

    #[test]
    fn test_webhook_verify_zero_tolerance() {
        let secret = test_secret();
        // Some(0) = strictest: only accept timestamps matching the current second.
        let sig = webhook_sign(&secret, b"body", "1000000000").expect("sign");
        let valid = webhook_verify(&secret, b"body", &sig, Some(0)).expect("verify");
        assert!(!valid, "zero tolerance should reject old timestamps");
    }

    #[test]
    fn test_webhook_verify_disabled() {
        let secret = test_secret();
        // None = timestamp check disabled entirely.
        let sig = webhook_sign(&secret, b"body", "1000000000").expect("sign");
        let valid = webhook_verify(&secret, b"body", &sig, None).expect("verify");
        assert!(valid, "None tolerance should skip timestamp check");
    }

    #[test]
    fn test_webhook_verify_invalid_format() {
        let secret = test_secret();
        let valid =
            webhook_verify(&secret, b"body", "not-a-valid-header", Some(300)).expect("verify");
        assert!(!valid, "invalid signature format should return false");
    }

    #[test]
    fn test_webhook_verify_wrong_secret() {
        let s1 = SecretString::from("secret-one");
        let s2 = SecretString::from("secret-two");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let ts = now.to_string();
        let sig = webhook_sign(&s1, b"body", &ts).expect("sign");
        let valid = webhook_verify(&s2, b"body", &sig, Some(300)).expect("verify");
        assert!(!valid, "wrong secret should fail verification");
    }

    // ── parse_webhook_signature ──────────────────────────────────────────

    #[test]
    fn test_parse_webhook_signature_valid() {
        let parsed = parse_webhook_signature("t=1700000000,v1=abcdef0123456789");
        assert!(parsed.is_some());
        let (ts, hex) = parsed.expect("should parse");
        assert_eq!(ts, 1_700_000_000);
        assert_eq!(hex, "abcdef0123456789");
    }

    #[test]
    fn test_parse_webhook_signature_missing_t() {
        assert!(parse_webhook_signature("v1=abcdef").is_none());
    }

    #[test]
    fn test_parse_webhook_signature_missing_v1() {
        assert!(parse_webhook_signature("t=12345").is_none());
    }

    #[test]
    fn test_parse_webhook_signature_empty() {
        assert!(parse_webhook_signature("").is_none());
    }

    #[test]
    fn test_parse_webhook_signature_invalid_timestamp() {
        assert!(parse_webhook_signature("t=not_a_number,v1=abc").is_none());
    }

    // ── is_timestamp_fresh ───────────────────────────────────────────────

    #[test]
    fn test_timestamp_fresh_current() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        assert!(is_timestamp_fresh(now, 300));
    }

    #[test]
    fn test_timestamp_stale() {
        assert!(!is_timestamp_fresh(1_000_000_000, 300));
    }

    // ── hex_decode integration ───────────────────────────────────────────

    #[test]
    fn test_hmac_hash_hex_decodes() {
        let pepper = test_pepper();
        let hash = hmac_hash(&pepper, b"data").expect("hmac");
        let decoded = crate::random::hex_decode(&hash);
        assert!(decoded.is_some(), "HMAC output should be valid hex");
        assert_eq!(decoded.expect("valid hex").len(), 32, "SHA-256 is 32 bytes");
    }
}
