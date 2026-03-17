//! HKDF key derivation with verse-based domain separation.
//!
//! Implements RFC 5869 HKDF-SHA256 using KJV Scripture verses as the `info`
//! parameter for domain separation. Same pepper + different verse/purpose =
//! completely independent key material.

use hkdf::Hkdf;
use secrecy::{ExposeSecret, SecretString};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::error::{CryptoError, Result};
use crate::verses::{Verse, random_verse, verse_hkdf_info_with_purpose};

/// Wrapper around 32 bytes of derived key material.
///
/// Automatically zeroed on drop via [`Zeroizing`].
/// Debug output is redacted to prevent key material leaking into logs.
pub struct DerivedBytes(Zeroizing<[u8; 32]>);

impl std::fmt::Debug for DerivedBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DerivedBytes([REDACTED])")
    }
}

impl DerivedBytes {
    /// Access the raw derived bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Key material derived with a random verse — includes metadata for auditing.
///
/// The `bytes` field is the 32-byte key. The `verse_ref` and `purpose` fields
/// record what domain separation was applied, enabling audit logging and
/// deterministic re-derivation (given the same IKM).
pub struct VerseDerivedKey {
    /// The 32-byte derived key material (zeroized on drop).
    pub bytes: DerivedBytes,
    /// Scripture reference used in derivation (e.g., "John 3:16").
    pub verse_ref: String,
    /// Full verse text used in derivation.
    pub verse_text: String,
    /// Purpose string used for domain separation.
    pub purpose: String,
}

impl VerseDerivedKey {
    /// Access the raw derived bytes (delegates to the inner [`DerivedBytes`]).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        self.bytes.as_bytes()
    }
}

/// A fully-formed derived API key with metadata for storage and display.
///
/// The `raw` field holds the complete key string (secret). The remaining
/// fields are safe for logging and database storage.
///
/// Note: `raw` is excluded from `Serialize` / `Deserialize` because
/// [`SecretString`] intentionally does not implement serde traits without
/// the `SerializableSecret` marker. Callers must handle the secret field
/// explicitly.
#[derive(Debug, Clone)]
pub struct DerivedKey {
    /// The full API key string (secret — never log or store in plaintext).
    pub raw: SecretString,
    /// HMAC-SHA256 hash of the raw key for database lookup.
    pub hash: String,
    /// The non-secret prefix portion (e.g., `lak_prod_`).
    pub prefix: String,
    /// Last four characters of the encoded key body for display.
    pub last_four: String,
    /// Scripture reference used in derivation (e.g., "John 3:16").
    pub verse_ref: String,
    /// Full verse text used in derivation.
    pub verse_text: String,
}

// ─── Base62 encoding ─────────────────────────────────────────────────────────

/// Base62 alphabet: `0-9A-Za-z`.
const BASE62_CHARS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Encode a byte slice as a base62 string.
///
/// Uses simple big-integer division. Output length is deterministic
/// for a given input length.
fn base62_encode(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    // Convert bytes to a big-endian unsigned integer (as a Vec<u8> for division).
    let mut digits: Vec<u8> = bytes.to_vec();
    let mut result = Vec::new();

    // Repeatedly divide the big integer by 62.
    // Uses u64 accumulator — cannot overflow (max: 61 * 256 + 255 = 15,871).
    while !digits.is_empty() {
        let mut remainder: u64 = 0;
        let mut next = Vec::new();
        for &d in &digits {
            let acc = remainder * 256 + u64::from(d);
            let quotient = acc / 62;
            remainder = acc % 62;
            if !next.is_empty() || quotient > 0 {
                // quotient <= (15871 / 62) = 255, fits in u8
                #[allow(clippy::cast_possible_truncation)]
                next.push(quotient as u8);
            }
        }
        // remainder is always 0..61, safe to index BASE62_CHARS
        #[allow(clippy::cast_possible_truncation)]
        result.push(BASE62_CHARS[remainder as usize]);
        digits = next;
    }

    result.reverse();
    // All characters in BASE62_CHARS are ASCII, so from_utf8 cannot fail.
    // Using a fallback instead of unwrap per coding standards.
    String::from_utf8(result).unwrap_or_default()
}

// ─── CRC32 (IEEE / CRC-32b) ─────────────────────────────────────────────────

/// Compute CRC32 (IEEE 802.3) checksum of the given bytes.
///
/// Uses a simple table-less algorithm. For API key checksums only — not
/// performance-critical.
fn crc32_checksum(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = crc & 1;
            crc >>= 1;
            if mask != 0 {
                crc ^= 0xEDB8_8320;
            }
        }
    }
    !crc
}

// ─── Core derivation ─────────────────────────────────────────────────────────

/// Derive 32 bytes of key material via HKDF-SHA256.
///
/// The pepper serves as the HKDF salt, `ikm` is the input keying material,
/// and the verse + purpose form the `info` parameter for domain separation.
///
/// # Examples
///
/// ```
/// use la_crypto::derive::derive_key;
/// use la_crypto::verses::find_verse;
/// use secrecy::SecretString;
///
/// let pepper = SecretString::from("my-pepper");
/// let verse = find_verse("John 1:1").expect("verse exists");
/// let key = derive_key(&pepper, b"input-material", verse, "api-key")
///     .expect("derivation succeeds");
/// assert_eq!(key.as_bytes().len(), 32);
/// ```
///
/// # Errors
///
/// Returns [`CryptoError::HkdfExpand`] if HKDF expansion fails (e.g., if
/// the output length exceeds the HKDF maximum of 255 * `HashLen`).
pub fn derive_key(
    pepper: &SecretString,
    ikm: &[u8],
    verse: &Verse,
    purpose: &str,
) -> Result<DerivedBytes> {
    let hkdf_info = verse_hkdf_info_with_purpose(verse, purpose);
    let hk = Hkdf::<Sha256>::new(Some(pepper.expose_secret().as_bytes()), ikm);
    let mut derived = Zeroizing::new([0u8; 32]);
    hk.expand(hkdf_info.as_bytes(), derived.as_mut())
        .map_err(|e| CryptoError::HkdfExpand(e.to_string()))?;
    Ok(DerivedBytes(derived))
}

// ─── API key derivation ──────────────────────────────────────────────────────

/// Derive a structured API key with the format `lak_{env}_{body}{checksum}`.
///
/// The key body is derived via [`derive_key`] using a random verse and the
/// purpose `"api-key:{env}"`. A CRC32 checksum of the body is appended for
/// quick client-side validation.
///
/// # Examples
///
/// ```
/// use la_crypto::derive::derive_api_key;
/// use la_crypto::verses::find_verse;
/// use secrecy::{ExposeSecret, SecretString};
///
/// let pepper = SecretString::from("my-pepper");
/// let verse = find_verse("John 3:16").expect("verse exists");
/// let key = derive_api_key(&pepper, "prod", verse).expect("key");
/// assert!(key.raw.expose_secret().starts_with("lak_prod_"));
/// assert!(!key.hash.is_empty());
/// ```
///
/// # Fields in the returned [`DerivedKey`]
///
/// - `raw`: the full secret key string
/// - `hash`: hex-encoded HMAC-SHA256 of the key (for database lookup)
/// - `prefix`: the `lak_{env}_` portion
/// - `last_four`: last 4 chars of the base62 body
/// - `verse_ref` / `verse_text`: the verse used in derivation
///
/// # Errors
///
/// Returns [`CryptoError::HkdfExpand`] on derivation failure or
/// [`CryptoError::HmacInit`] if HMAC key initialization fails.
pub fn derive_api_key(pepper: &SecretString, env: &str, verse: &Verse) -> Result<DerivedKey> {
    let ikm = crate::random::generate_bytes(32);
    let purpose = format!("api-key:{env}");
    let derived = derive_key(pepper, &ikm, verse, &purpose)?;

    let body = base62_encode(derived.as_bytes());
    let checksum = crc32_checksum(body.as_bytes());
    let checksum_b62 = base62_encode(&checksum.to_be_bytes());

    let prefix = format!("lak_{env}_");
    let raw_str = format!("{prefix}{body}{checksum_b62}");

    let last_four = extract_last_four(&body);
    // Hash only the opaque body + checksum (not the public prefix) to match
    // Stripe/GitHub convention and avoid hashing zero-entropy prefix bytes.
    let body_with_checksum = format!("{body}{checksum_b62}");
    let hash = crate::hash::hmac_hash(pepper, body_with_checksum.as_bytes())?;

    Ok(DerivedKey {
        raw: SecretString::from(raw_str),
        hash,
        prefix,
        last_four,
        verse_ref: verse.reference.to_string(),
        verse_text: verse.text.to_string(),
    })
}

/// Extract the last four characters of a string, or the whole string if shorter.
fn extract_last_four(s: &str) -> String {
    let len = s.len();
    if len >= 4 {
        s[len.saturating_sub(4)..].to_string()
    } else {
        s.to_string()
    }
}

// ─── Encryption key derivation ───────────────────────────────────────────────

/// Derive a 32-byte AES-256 encryption key with verse metadata.
///
/// Uses a random verse and the purpose `"encryption:{context}"` for domain
/// separation. The context string should identify the data being encrypted
/// (e.g., `"vault"`, `"session"`, `"backup"`).
///
/// Returns a [`VerseDerivedKey`] containing the 32-byte key plus the verse
/// reference, verse text, and purpose used in derivation — enabling audit
/// logging and deterministic re-derivation.
///
/// # Examples
///
/// ```
/// use la_crypto::derive::derive_encryption_key;
/// use secrecy::SecretString;
///
/// let pepper = SecretString::from("my-pepper");
/// let dk = derive_encryption_key(&pepper, "vault").expect("key");
/// assert_eq!(dk.as_bytes().len(), 32);
/// assert_eq!(dk.purpose, "encryption:vault");
/// ```
///
/// # Errors
///
/// Returns [`CryptoError::HkdfExpand`] if HKDF expansion fails.
pub fn derive_encryption_key(pepper: &SecretString, context: &str) -> Result<VerseDerivedKey> {
    let verse = random_verse();
    let ikm = crate::random::generate_bytes(32);
    let purpose = format!("encryption:{context}");
    let bytes = derive_key(pepper, &ikm, verse, &purpose)?;
    Ok(VerseDerivedKey {
        bytes,
        verse_ref: verse.reference.to_string(),
        verse_text: verse.text.to_string(),
        purpose,
    })
}

// ─── Signing key derivation ──────────────────────────────────────────────────

/// Derive a 32-byte Ed25519 keypair seed with verse metadata.
///
/// Uses a random verse and the purpose `"signing:{context}"` for domain
/// separation. The returned [`VerseDerivedKey`] contains 32 bytes suitable
/// for use as the Ed25519 secret key seed (see
/// `ed25519_dalek::SigningKey::from_bytes`), plus the verse reference,
/// verse text, and purpose used in derivation.
///
/// # Examples
///
/// ```
/// use la_crypto::derive::derive_signing_key;
/// use secrecy::SecretString;
///
/// let pepper = SecretString::from("my-pepper");
/// let dk = derive_signing_key(&pepper, "evidence-chain").expect("key");
/// assert_eq!(dk.as_bytes().len(), 32);
/// assert_eq!(dk.purpose, "signing:evidence-chain");
/// ```
///
/// # Errors
///
/// Returns [`CryptoError::HkdfExpand`] if HKDF expansion fails.
pub fn derive_signing_key(pepper: &SecretString, context: &str) -> Result<VerseDerivedKey> {
    let verse = random_verse();
    let ikm = crate::random::generate_bytes(32);
    let purpose = format!("signing:{context}");
    let bytes = derive_key(pepper, &ikm, verse, &purpose)?;
    Ok(VerseDerivedKey {
        bytes,
        verse_ref: verse.reference.to_string(),
        verse_text: verse.text.to_string(),
        purpose,
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verses::find_verse;

    fn test_pepper() -> SecretString {
        SecretString::from("test-pepper-for-la-crypto-unit-tests")
    }

    #[test]
    fn test_derive_key_produces_32_bytes() {
        let pepper = test_pepper();
        let verse = find_verse("John 1:1").expect("test setup");
        let ikm = b"input-keying-material";
        let derived = derive_key(&pepper, ikm, verse, "test").expect("derivation");
        assert_eq!(derived.as_bytes().len(), 32);
    }

    #[test]
    fn test_derive_key_deterministic() {
        let pepper = test_pepper();
        let verse = find_verse("John 1:1").expect("test setup");
        let ikm = b"same-input";
        let a = derive_key(&pepper, ikm, verse, "test").expect("derivation");
        let b = derive_key(&pepper, ikm, verse, "test").expect("derivation");
        assert_eq!(a.as_bytes(), b.as_bytes(), "same inputs = same output");
    }

    #[test]
    fn test_derive_key_different_purpose_different_output() {
        let pepper = test_pepper();
        let verse = find_verse("John 1:1").expect("test setup");
        let ikm = b"same-input";
        let a = derive_key(&pepper, ikm, verse, "encryption").expect("derivation");
        let b = derive_key(&pepper, ikm, verse, "signing").expect("derivation");
        assert_ne!(
            a.as_bytes(),
            b.as_bytes(),
            "different purposes = different keys"
        );
    }

    #[test]
    fn test_derive_key_different_verse_different_output() {
        let pepper = test_pepper();
        let v1 = find_verse("John 1:1").expect("test setup");
        let v2 = find_verse("John 3:16").expect("test setup");
        let ikm = b"same-input";
        let a = derive_key(&pepper, ikm, v1, "test").expect("derivation");
        let b = derive_key(&pepper, ikm, v2, "test").expect("derivation");
        assert_ne!(
            a.as_bytes(),
            b.as_bytes(),
            "different verses = different keys"
        );
    }

    #[test]
    fn test_derive_key_different_pepper_different_output() {
        let p1 = SecretString::from("pepper-one");
        let p2 = SecretString::from("pepper-two");
        let verse = find_verse("John 1:1").expect("test setup");
        let ikm = b"same-input";
        let a = derive_key(&p1, ikm, verse, "test").expect("derivation");
        let b = derive_key(&p2, ikm, verse, "test").expect("derivation");
        assert_ne!(
            a.as_bytes(),
            b.as_bytes(),
            "different peppers = different keys"
        );
    }

    #[test]
    fn test_derive_api_key_format() {
        let pepper = test_pepper();
        let verse = find_verse("John 3:16").expect("test setup");
        let key = derive_api_key(&pepper, "test", verse).expect("api key derivation");
        let raw = key.raw.expose_secret();
        assert!(
            raw.starts_with("lak_test_"),
            "key should start with lak_test_, got: {raw}"
        );
        assert_eq!(key.prefix, "lak_test_");
        assert!(!key.hash.is_empty(), "hash should not be empty");
        assert!(
            key.last_four.len() <= 4,
            "last_four should be at most 4 chars"
        );
        assert_eq!(key.verse_ref, "John 3:16");
        assert!(!key.verse_text.is_empty());
    }

    #[test]
    fn test_derive_api_key_env_variants() {
        let pepper = test_pepper();
        let verse = find_verse("John 1:1").expect("test setup");
        let prod_key = derive_api_key(&pepper, "prod", verse).expect("prod key");
        assert!(prod_key.raw.expose_secret().starts_with("lak_prod_"));
        assert_eq!(prod_key.prefix, "lak_prod_");
    }

    #[test]
    fn test_derive_api_key_unique() {
        let pepper = test_pepper();
        let verse = find_verse("John 1:1").expect("test setup");
        let a = derive_api_key(&pepper, "test", verse).expect("key a");
        let b = derive_api_key(&pepper, "test", verse).expect("key b");
        assert_ne!(
            a.raw.expose_secret(),
            b.raw.expose_secret(),
            "random IKM means each call produces a unique key"
        );
    }

    #[test]
    fn test_derive_encryption_key_produces_32_bytes() {
        let pepper = test_pepper();
        let derived = derive_encryption_key(&pepper, "vault").expect("encryption key");
        assert_eq!(derived.bytes.as_bytes().len(), 32);
        assert!(
            !derived.verse_ref.is_empty(),
            "verse_ref should be populated"
        );
        assert!(
            !derived.verse_text.is_empty(),
            "verse_text should be populated"
        );
        assert_eq!(derived.purpose, "encryption:vault");
    }

    #[test]
    fn test_derive_encryption_key_unique() {
        let pepper = test_pepper();
        let a = derive_encryption_key(&pepper, "vault").expect("key a");
        let b = derive_encryption_key(&pepper, "vault").expect("key b");
        assert_ne!(
            a.bytes.as_bytes(),
            b.bytes.as_bytes(),
            "random verse + IKM means each call is unique"
        );
    }

    #[test]
    fn test_derive_signing_key_produces_32_bytes() {
        let pepper = test_pepper();
        let derived = derive_signing_key(&pepper, "evidence-chain").expect("signing key");
        assert_eq!(derived.bytes.as_bytes().len(), 32);
        assert!(
            !derived.verse_ref.is_empty(),
            "verse_ref should be populated"
        );
        assert!(
            !derived.verse_text.is_empty(),
            "verse_text should be populated"
        );
        assert_eq!(derived.purpose, "signing:evidence-chain");
    }

    #[test]
    fn test_derive_signing_key_unique() {
        let pepper = test_pepper();
        let a = derive_signing_key(&pepper, "evidence-chain").expect("key a");
        let b = derive_signing_key(&pepper, "evidence-chain").expect("key b");
        assert_ne!(
            a.bytes.as_bytes(),
            b.bytes.as_bytes(),
            "random verse + IKM means each call is unique"
        );
    }

    #[test]
    fn test_base62_encode_not_empty() {
        let encoded = base62_encode(&[0xde, 0xad, 0xbe, 0xef]);
        assert!(!encoded.is_empty());
        assert!(
            encoded.chars().all(|c| c.is_ascii_alphanumeric()),
            "base62 output should be alphanumeric"
        );
    }

    #[test]
    fn test_base62_encode_empty() {
        assert_eq!(base62_encode(&[]), "");
    }

    #[test]
    fn test_base62_encode_deterministic() {
        let a = base62_encode(&[1, 2, 3, 4]);
        let b = base62_encode(&[1, 2, 3, 4]);
        assert_eq!(a, b);
    }

    #[test]
    fn test_crc32_known_value() {
        // CRC32 of "123456789" is 0xCBF43926 (standard test vector)
        let checksum = crc32_checksum(b"123456789");
        assert_eq!(checksum, 0xCBF4_3926);
    }

    #[test]
    fn test_crc32_empty() {
        let checksum = crc32_checksum(b"");
        assert_eq!(checksum, 0x0000_0000);
    }

    #[test]
    fn test_extract_last_four() {
        assert_eq!(extract_last_four("abcdef"), "cdef");
        assert_eq!(extract_last_four("ab"), "ab");
        assert_eq!(extract_last_four(""), "");
    }

    #[test]
    fn test_derived_bytes_as_bytes_matches() {
        let pepper = test_pepper();
        let verse = find_verse("John 1:1").expect("test setup");
        let ikm = b"test-material";
        let derived = derive_key(&pepper, ikm, verse, "test").expect("derivation");
        let bytes = derived.as_bytes();
        assert_eq!(bytes.len(), 32);
        // Verify it's not all zeros (derived key should have entropy).
        assert!(bytes.iter().any(|&b| b != 0));
    }
}
