//! AES-256-GCM authenticated encryption.
//!
//! Provides seal/open operations with optional verse-based key derivation.
//! Nonces are 96-bit, generated via CSPRNG — never user-supplied.

use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit};
use secrecy::SecretString;
use zeroize::Zeroizing;

use crate::derive::DerivedBytes;
use crate::error::{CryptoError, Result};
use crate::verses::Verse;

/// Sealed (encrypted + authenticated) data produced by [`seal`].
///
/// The `ciphertext_with_tag` field contains the ciphertext with the 16-byte
/// GCM authentication tag appended (as produced by `aes-gcm`). The `nonce`
/// is the 96-bit IV used during encryption — both are needed for decryption.
///
/// Implements `Serialize`/`Deserialize` for portable storage and transmission.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SealedData {
    /// Ciphertext concatenated with the 16-byte GCM authentication tag.
    pub ciphertext_with_tag: Vec<u8>,
    /// 96-bit (12-byte) nonce used during encryption.
    pub nonce: [u8; 12],
}

/// Encrypt `plaintext` with AES-256-GCM using the given 32-byte key.
///
/// The nonce is generated internally via [`crate::random::generate_nonce`]
/// (96-bit CSPRNG). The `associated_data` (AAD) is authenticated but not
/// encrypted — the receiver must supply the same AAD to decrypt.
///
/// # Examples
///
/// ```
/// use lightarchitects_crypto::encrypt::{seal, open};
///
/// let key = [0x42u8; 32];
/// let sealed = seal(&key, b"secret message", b"context").expect("seal");
/// let plain = open(&key, &sealed, b"context").expect("open");
/// assert_eq!(plain, b"secret message");
/// ```
///
/// # Errors
///
/// Returns [`CryptoError::Encryption`] if the AES-GCM cipher fails.
pub fn seal(key: &[u8; 32], plaintext: &[u8], associated_data: &[u8]) -> Result<SealedData> {
    let cipher = Aes256Gcm::new(GenericArray::from_slice(key));
    let nonce_bytes = crate::random::generate_nonce();
    let nonce = GenericArray::from_slice(&nonce_bytes);

    let payload = Payload {
        msg: plaintext,
        aad: associated_data,
    };

    let ciphertext_with_tag = cipher
        .encrypt(nonce, payload)
        .map_err(|e| CryptoError::Encryption(e.to_string()))?;

    Ok(SealedData {
        ciphertext_with_tag,
        nonce: nonce_bytes,
    })
}

/// Decrypt and verify [`SealedData`] with AES-256-GCM.
///
/// The caller must supply the same `associated_data` (AAD) that was used
/// during encryption. Returns the original plaintext on success.
///
/// # Examples
///
/// ```
/// use lightarchitects_crypto::encrypt::{seal, open};
///
/// let key = [0x42u8; 32];
/// let sealed = seal(&key, b"hello", b"aad").expect("seal");
/// let plain = open(&key, &sealed, b"aad").expect("open");
/// assert_eq!(plain, b"hello");
/// ```
///
/// # Errors
///
/// Returns [`CryptoError::Decryption`] if decryption fails (wrong key,
/// tampered ciphertext, incorrect nonce, or AAD mismatch).
pub fn open(key: &[u8; 32], sealed: &SealedData, associated_data: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(GenericArray::from_slice(key));
    let nonce = GenericArray::from_slice(&sealed.nonce);

    let payload = Payload {
        msg: &sealed.ciphertext_with_tag,
        aad: associated_data,
    };

    cipher
        .decrypt(nonce, payload)
        .map_err(|e| CryptoError::Decryption(e.to_string()))
}

/// Derive an encryption key from a pepper + verse, then seal the plaintext.
///
/// Uses [`crate::derive::derive_key`] with a random IKM and the purpose
/// `"encryption"` for domain separation. The derived key is zeroized on drop.
///
/// Returns both the [`SealedData`] and the [`DerivedBytes`] key so the caller
/// can persist the key for later decryption via [`open`]. The IKM is random,
/// so each call produces a unique key — forward secrecy per message.
///
/// # Errors
///
/// Returns [`CryptoError::HkdfExpand`] if key derivation fails, or
/// [`CryptoError::Encryption`] if AES-GCM encryption fails.
pub fn seal_with_verse(
    pepper: &SecretString,
    verse: &Verse,
    plaintext: &[u8],
    associated_data: &[u8],
) -> Result<(SealedData, DerivedBytes)> {
    let ikm = Zeroizing::new(crate::random::generate_bytes(32));
    let derived = crate::derive::derive_key(pepper, &ikm, verse, "encryption")?;
    let sealed = seal(derived.as_bytes(), plaintext, associated_data)?;
    Ok((sealed, derived))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::verses::find_verse;

    fn test_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        key[0] = 0xDE;
        key[1] = 0xAD;
        key[31] = 0xEF;
        key
    }

    fn test_pepper() -> SecretString {
        SecretString::from("test-pepper-for-la-crypto-unit-tests")
    }

    // ── Roundtrip: seal then open ─────────────────────────────────────────

    #[test]
    fn roundtrip_basic() {
        let key = test_key();
        let plaintext = b"In the beginning was the Word";
        let aad = b"context:test";

        let sealed = seal(&key, plaintext, aad).expect("seal");
        let recovered = open(&key, &sealed, aad).expect("open");
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn roundtrip_empty_plaintext() {
        let key = test_key();
        let sealed = seal(&key, b"", b"aad").expect("seal");
        let recovered = open(&key, &sealed, b"aad").expect("open");
        assert!(recovered.is_empty(), "empty plaintext should roundtrip");
    }

    #[test]
    fn roundtrip_empty_aad() {
        let key = test_key();
        let plaintext = b"secret data";
        let sealed = seal(&key, plaintext, b"").expect("seal");
        let recovered = open(&key, &sealed, b"").expect("open");
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn roundtrip_large_plaintext() {
        let key = test_key();
        let plaintext = vec![0xABu8; 65_536];
        let sealed = seal(&key, &plaintext, b"").expect("seal");
        let recovered = open(&key, &sealed, b"").expect("open");
        assert_eq!(recovered, plaintext);
    }

    // ── Tamper detection ──────────────────────────────────────────────────

    #[test]
    fn tamper_ciphertext_fails() {
        let key = test_key();
        let sealed = seal(&key, b"plaintext", b"aad").expect("seal");

        let mut tampered = sealed.clone();
        if let Some(byte) = tampered.ciphertext_with_tag.first_mut() {
            *byte ^= 0xFF;
        }

        let result = open(&key, &tampered, b"aad");
        assert!(
            result.is_err(),
            "tampered ciphertext should fail decryption"
        );
    }

    #[test]
    fn tamper_nonce_fails() {
        let key = test_key();
        let sealed = seal(&key, b"plaintext", b"aad").expect("seal");

        let mut tampered = sealed.clone();
        tampered.nonce[0] ^= 0xFF;

        let result = open(&key, &tampered, b"aad");
        assert!(result.is_err(), "tampered nonce should fail decryption");
    }

    #[test]
    fn tamper_tag_fails() {
        let key = test_key();
        let sealed = seal(&key, b"plaintext", b"aad").expect("seal");

        let mut tampered = sealed.clone();
        // The tag is the last 16 bytes of ciphertext_with_tag.
        if let Some(byte) = tampered.ciphertext_with_tag.last_mut() {
            *byte ^= 0xFF;
        }

        let result = open(&key, &tampered, b"aad");
        assert!(result.is_err(), "tampered tag should fail decryption");
    }

    // ── Wrong key fails ───────────────────────────────────────────────────

    #[test]
    fn wrong_key_fails() {
        let key = test_key();
        let sealed = seal(&key, b"plaintext", b"aad").expect("seal");

        let mut wrong_key = [0u8; 32];
        wrong_key[0] = 0x01;

        let result = open(&wrong_key, &sealed, b"aad");
        assert!(result.is_err(), "wrong key should fail decryption");
    }

    // ── AAD mismatch fails ────────────────────────────────────────────────

    #[test]
    fn aad_mismatch_fails() {
        let key = test_key();
        let sealed = seal(&key, b"plaintext", b"correct-aad").expect("seal");

        let result = open(&key, &sealed, b"wrong-aad");
        assert!(result.is_err(), "AAD mismatch should fail decryption");
    }

    #[test]
    fn aad_present_vs_absent_fails() {
        let key = test_key();
        let sealed = seal(&key, b"plaintext", b"some-aad").expect("seal");

        let result = open(&key, &sealed, b"");
        assert!(result.is_err(), "missing AAD should fail decryption");
    }

    // ── Unique nonces ─────────────────────────────────────────────────────

    #[test]
    fn seal_produces_unique_nonces() {
        let key = test_key();
        let a = seal(&key, b"same", b"").expect("seal a");
        let b = seal(&key, b"same", b"").expect("seal b");
        assert_ne!(
            a.nonce, b.nonce,
            "two seals should produce different nonces"
        );
    }

    // ── SealedData structure ──────────────────────────────────────────────

    #[test]
    fn sealed_data_nonce_is_12_bytes() {
        let key = test_key();
        let sealed = seal(&key, b"test", b"").expect("seal");
        assert_eq!(sealed.nonce.len(), 12, "AES-GCM nonce must be 12 bytes");
    }

    #[test]
    fn sealed_data_ciphertext_includes_tag() {
        let key = test_key();
        let plaintext = b"hello";
        let sealed = seal(&key, plaintext, b"").expect("seal");
        // ciphertext_with_tag = plaintext_len + 16-byte tag
        let expected_len = plaintext.len().saturating_add(16);
        assert_eq!(
            sealed.ciphertext_with_tag.len(),
            expected_len,
            "ciphertext should be plaintext + 16-byte GCM tag"
        );
    }

    // ── seal_with_verse ───────────────────────────────────────────────────

    #[test]
    fn seal_with_verse_produces_sealed_data_and_key() {
        let pepper = test_pepper();
        let verse = find_verse("John 1:1").expect("test setup");
        let (sealed, key) =
            seal_with_verse(&pepper, verse, b"secret message", b"context").expect("seal");
        assert!(!sealed.ciphertext_with_tag.is_empty());
        assert_eq!(sealed.nonce.len(), 12);

        // Verify decryption works with the returned key.
        let recovered = open(key.as_bytes(), &sealed, b"context").expect("open");
        assert_eq!(recovered, b"secret message");
    }

    #[test]
    fn seal_with_verse_unique_each_call() {
        let pepper = test_pepper();
        let verse = find_verse("John 3:16").expect("test setup");
        let (a, _ka) = seal_with_verse(&pepper, verse, b"same", b"").expect("seal a");
        let (b, _kb) = seal_with_verse(&pepper, verse, b"same", b"").expect("seal b");
        assert_ne!(
            a.ciphertext_with_tag, b.ciphertext_with_tag,
            "random IKM means each seal is unique"
        );
    }
}
