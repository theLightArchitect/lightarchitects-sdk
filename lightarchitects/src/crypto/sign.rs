//! Ed25519 digital signatures.
//!
//! Provides sign/verify operations with optional verse-based key derivation.
//! Used for evidence chain integrity (SERAPH) and report signing.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use secrecy::SecretString;
use zeroize::Zeroizing;

use crate::crypto::error::Result;
use crate::crypto::verses::Verse;

/// Sign `message` with an Ed25519 signing key.
///
/// Returns the 64-byte Ed25519 signature. The signing key is not consumed.
///
/// # Examples
///
/// ```
/// use lightarchitects::crypto::sign::{keypair_from_seed, sign, verify};
///
/// let (sk, vk) = keypair_from_seed(&[0xABu8; 32]);
/// let sig = sign(&sk, b"hello");
/// assert!(verify(&vk, b"hello", &sig));
/// ```
#[must_use]
pub fn sign(signing_key: &SigningKey, message: &[u8]) -> Signature {
    signing_key.sign(message)
}

/// Verify an Ed25519 signature against a message and public key.
///
/// Returns `true` if the signature is valid, `false` otherwise.
/// Verification failure is a boolean result — not an error — because
/// invalid signatures are an expected condition (e.g., tampered data).
///
/// # Examples
///
/// ```
/// use lightarchitects::crypto::sign::{keypair_from_seed, sign, verify};
///
/// let (sk, vk) = keypair_from_seed(&[0xABu8; 32]);
/// let sig = sign(&sk, b"message");
/// assert!(verify(&vk, b"message", &sig));
/// assert!(!verify(&vk, b"tampered", &sig));
/// ```
#[must_use]
pub fn verify(verifying_key: &VerifyingKey, message: &[u8], signature: &Signature) -> bool {
    verifying_key.verify(message, signature).is_ok()
}

/// Derive a deterministic Ed25519 keypair from a 32-byte seed.
///
/// The same seed always produces the same keypair. Use this with output
/// from [`lightarchitects::crypto::derive::derive_signing_key`] or any 32-byte secret.
///
/// The returned tuple is `(signing_key, verifying_key)` where the
/// verifying key is the public half suitable for distribution.
///
/// # Examples
///
/// ```
/// use lightarchitects::crypto::sign::keypair_from_seed;
///
/// let seed = [0x42u8; 32];
/// let (sk, vk) = keypair_from_seed(&seed);
/// // Same seed always produces the same keys.
/// let (sk2, _) = keypair_from_seed(&seed);
/// assert_eq!(sk.to_bytes(), sk2.to_bytes());
/// ```
#[must_use]
pub fn keypair_from_seed(seed: &[u8; 32]) -> (SigningKey, VerifyingKey) {
    let signing_key = SigningKey::from_bytes(seed);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

/// Derive a signing key from a pepper + verse, sign the message, and return
/// both the signature and the public (verifying) key.
///
/// Uses [`lightarchitects::crypto::derive::derive_key`] with a random IKM and the purpose
/// `"signing"` for domain separation. The derived seed is zeroized on drop.
///
/// The caller needs the returned [`VerifyingKey`] for later verification,
/// since the signing key cannot be re-derived (random IKM).
///
/// # Errors
///
/// Returns [`lightarchitects::crypto::CryptoError::HkdfExpand`] if key derivation fails.
pub fn sign_with_verse(
    pepper: &SecretString,
    verse: &Verse,
    message: &[u8],
) -> Result<(Signature, VerifyingKey)> {
    let ikm = Zeroizing::new(crate::crypto::random::generate_bytes(32));
    let derived = crate::crypto::derive::derive_key(pepper, &ikm, verse, "signing")?;
    let (signing_key, verifying_key) = keypair_from_seed(derived.as_bytes());
    let signature = sign(&signing_key, message);
    Ok((signature, verifying_key))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::crypto::verses::find_verse;

    fn test_seed() -> [u8; 32] {
        let mut seed = [0u8; 32];
        seed[0] = 0x42;
        seed[15] = 0xAB;
        seed[31] = 0xCD;
        seed
    }

    fn test_pepper() -> SecretString {
        SecretString::from("test-pepper-for-la-crypto-unit-tests")
    }

    // ── sign then verify ──────────────────────────────────────────────────

    #[test]
    fn sign_verify_roundtrip() {
        let (sk, vk) = keypair_from_seed(&test_seed());
        let message = b"This is a test message";
        let sig = sign(&sk, message);
        assert!(verify(&vk, message, &sig), "valid signature should verify");
    }

    #[test]
    fn sign_verify_empty_message() {
        let (sk, vk) = keypair_from_seed(&test_seed());
        let sig = sign(&sk, b"");
        assert!(verify(&vk, b"", &sig), "empty message should roundtrip");
    }

    #[test]
    fn sign_verify_large_message() {
        let (sk, vk) = keypair_from_seed(&test_seed());
        let message = vec![0xFFu8; 100_000];
        let sig = sign(&sk, &message);
        assert!(verify(&vk, &message, &sig), "large message should verify");
    }

    // ── Tamper detection ──────────────────────────────────────────────────

    #[test]
    fn tampered_message_fails() {
        let (sk, vk) = keypair_from_seed(&test_seed());
        let sig = sign(&sk, b"original message");
        assert!(
            !verify(&vk, b"tampered message", &sig),
            "tampered message should fail verification"
        );
    }

    #[test]
    fn single_bit_tamper_fails() {
        let (sk, vk) = keypair_from_seed(&test_seed());
        let message = b"precise message";
        let sig = sign(&sk, message);

        let mut tampered = message.to_vec();
        tampered[0] ^= 0x01;
        assert!(
            !verify(&vk, &tampered, &sig),
            "single bit flip should fail verification"
        );
    }

    #[test]
    fn wrong_key_fails() {
        let (sk, _vk) = keypair_from_seed(&test_seed());
        let sig = sign(&sk, b"message");

        // Different seed = different keypair.
        let mut other_seed = [0xFFu8; 32];
        other_seed[0] = 0x01;
        let (_other_sk, other_vk) = keypair_from_seed(&other_seed);

        assert!(
            !verify(&other_vk, b"message", &sig),
            "wrong verifying key should fail"
        );
    }

    // ── keypair_from_seed deterministic ───────────────────────────────────

    #[test]
    fn keypair_from_seed_deterministic() {
        let seed = test_seed();
        let (sk1, vk1) = keypair_from_seed(&seed);
        let (sk2, vk2) = keypair_from_seed(&seed);
        assert_eq!(
            sk1.to_bytes(),
            sk2.to_bytes(),
            "same seed = same signing key"
        );
        assert_eq!(
            vk1.to_bytes(),
            vk2.to_bytes(),
            "same seed = same verifying key"
        );
    }

    #[test]
    fn different_seeds_different_keys() {
        let mut seed_a = [0u8; 32];
        seed_a[0] = 0x01;
        let mut seed_b = [0u8; 32];
        seed_b[0] = 0x02;

        let (_sk_a, vk_a) = keypair_from_seed(&seed_a);
        let (_sk_b, vk_b) = keypair_from_seed(&seed_b);
        assert_ne!(
            vk_a.to_bytes(),
            vk_b.to_bytes(),
            "different seeds = different public keys"
        );
    }

    // ── Signature is 64 bytes ─────────────────────────────────────────────

    #[test]
    fn signature_is_64_bytes() {
        let (sk, _vk) = keypair_from_seed(&test_seed());
        let sig = sign(&sk, b"test");
        assert_eq!(sig.to_bytes().len(), 64, "Ed25519 signature is 64 bytes");
    }

    // ── sign_with_verse ───────────────────────────────────────────────────

    #[test]
    fn sign_with_verse_roundtrip() {
        let pepper = test_pepper();
        let verse = find_verse("John 1:1").expect("test setup");
        let message = b"evidence chain entry";

        let (sig, vk) = sign_with_verse(&pepper, verse, message).expect("sign");
        assert!(
            verify(&vk, message, &sig),
            "verse-derived signature should verify"
        );
    }

    #[test]
    fn sign_with_verse_unique_each_call() {
        let pepper = test_pepper();
        let verse = find_verse("John 3:16").expect("test setup");
        let message = b"same message";

        let (sig_a, vk_a) = sign_with_verse(&pepper, verse, message).expect("sign a");
        let (sig_b, vk_b) = sign_with_verse(&pepper, verse, message).expect("sign b");

        // Different random IKM means different keys and signatures.
        assert_ne!(
            vk_a.to_bytes(),
            vk_b.to_bytes(),
            "random IKM means different verifying keys"
        );
        assert_ne!(
            sig_a.to_bytes(),
            sig_b.to_bytes(),
            "random IKM means different signatures"
        );
    }

    #[test]
    fn sign_with_verse_tamper_fails() {
        let pepper = test_pepper();
        let verse = find_verse("Psalm 23:1").expect("test setup");

        let (sig, vk) = sign_with_verse(&pepper, verse, b"original").expect("sign");
        assert!(
            !verify(&vk, b"tampered", &sig),
            "tampered message should fail verse-derived verification"
        );
    }

    // ── Cross-verify: sign with seed, verify with derived ─────────────────

    #[test]
    fn cross_verify_seed_and_keypair() {
        let seed = test_seed();
        let (sk, vk) = keypair_from_seed(&seed);

        let message = b"cross-verify test";
        let sig = sign(&sk, message);

        // Re-derive the verifying key from the same seed.
        let (_, vk2) = keypair_from_seed(&seed);
        assert!(
            verify(&vk2, message, &sig),
            "re-derived verifying key should verify"
        );
        assert_eq!(vk.to_bytes(), vk2.to_bytes());
    }
}
