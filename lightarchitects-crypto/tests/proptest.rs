//! Property-based tests for la-crypto — powered by proptest.
//!
//! These tests verify cryptographic invariants hold across a wide range of
//! random inputs. Static fixtures (pepper, verse) are used where proptest
//! cannot generate the required types.

use lightarchitects_crypto::compare::constant_time_eq;
use lightarchitects_crypto::derive::derive_key;
use lightarchitects_crypto::encrypt::{open, seal};
use lightarchitects_crypto::hash::{hmac_hash, hmac_verify};
use lightarchitects_crypto::sign::{keypair_from_seed, sign, verify};
use lightarchitects_crypto::verses::find_verse;
use proptest::prelude::*;
use secrecy::SecretString;

/// Static pepper for property tests (proptest cannot generate `SecretString`).
fn test_pepper() -> SecretString {
    SecretString::from("proptest-pepper")
}

// ---- HKDF ----------------------------------------------------------------

proptest! {
    #[test]
    fn hkdf_always_32_bytes(ikm in prop::collection::vec(any::<u8>(), 1..256)) {
        let pepper = test_pepper();
        let verse = find_verse("John 1:1").expect("test setup");
        let derived = derive_key(&pepper, &ikm, verse, "proptest");
        prop_assert!(derived.is_ok(), "derive_key should not fail");
        let derived = derived.expect("checked above");
        prop_assert_eq!(derived.as_bytes().len(), 32);
    }
}

// ---- HMAC -----------------------------------------------------------------

proptest! {
    #[test]
    fn hmac_deterministic(data in prop::collection::vec(any::<u8>(), 0..1024)) {
        let pepper = test_pepper();
        let a = hmac_hash(&pepper, &data);
        let b = hmac_hash(&pepper, &data);
        prop_assert!(a.is_ok());
        prop_assert!(b.is_ok());
        prop_assert_eq!(a.expect("checked"), b.expect("checked"));
    }

    #[test]
    fn hmac_verify_roundtrip(data in prop::collection::vec(any::<u8>(), 0..1024)) {
        let pepper = test_pepper();
        let hash = hmac_hash(&pepper, &data);
        prop_assert!(hash.is_ok());
        let hash = hash.expect("checked");
        let valid = hmac_verify(&pepper, &data, &hash);
        prop_assert!(valid.is_ok());
        prop_assert!(valid.expect("checked"), "hmac_verify must accept its own hash");
    }
}

// ---- AES-GCM --------------------------------------------------------------

proptest! {
    #[test]
    fn aes_gcm_roundtrip(
        plaintext in prop::collection::vec(any::<u8>(), 0..4096),
        aad in prop::collection::vec(any::<u8>(), 0..256),
    ) {
        let key = [0x42u8; 32]; // fixed key — proptest exercises plaintext/aad variety
        let sealed = seal(&key, &plaintext, &aad);
        prop_assert!(sealed.is_ok(), "seal should not fail");
        let sealed = sealed.expect("checked");
        let recovered = open(&key, &sealed, &aad);
        prop_assert!(recovered.is_ok(), "open should not fail with correct key+aad");
        prop_assert_eq!(recovered.expect("checked"), plaintext);
    }

    #[test]
    fn aes_gcm_tamper_detection(plaintext in prop::collection::vec(any::<u8>(), 1..4096)) {
        let key = [0x42u8; 32];
        let aad = b"tamper-test";
        let sealed = seal(&key, &plaintext, aad);
        prop_assert!(sealed.is_ok());
        let mut tampered = sealed.expect("checked");

        // Flip first byte of ciphertext.
        if let Some(byte) = tampered.ciphertext_with_tag.first_mut() {
            *byte ^= 0xFF;
        }

        let result = open(&key, &tampered, aad);
        prop_assert!(result.is_err(), "tampered ciphertext must be rejected");
    }
}

// ---- Ed25519 ---------------------------------------------------------------

proptest! {
    #[test]
    fn ed25519_roundtrip(message in prop::collection::vec(any::<u8>(), 0..4096)) {
        let seed = [0xAB_u8; 32]; // fixed seed — proptest exercises message variety
        let (sk, vk) = keypair_from_seed(&seed);
        let sig = sign(&sk, &message);
        prop_assert!(verify(&vk, &message, &sig), "valid signature must verify");
    }

    #[test]
    fn ed25519_tamper_detection(message in prop::collection::vec(any::<u8>(), 1..4096)) {
        let seed = [0xAB_u8; 32];
        let (sk, vk) = keypair_from_seed(&seed);
        let sig = sign(&sk, &message);

        // Flip the first byte of the message.
        let mut tampered = message.clone();
        tampered[0] ^= 0xFF;

        // Tampered message must not verify (unless the flip was a no-op, which
        // cannot happen because XOR 0xFF always changes the byte).
        prop_assert!(!verify(&vk, &tampered, &sig), "tampered message must be rejected");
    }
}

// ---- Constant-time eq ------------------------------------------------------

proptest! {
    #[test]
    fn constant_time_eq_reflexive(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        prop_assert!(constant_time_eq(&bytes, &bytes), "x == x must always hold");
    }
}

// ---- Cross-purpose independence --------------------------------------------

proptest! {
    #[test]
    fn cross_purpose_independence(ikm in prop::collection::vec(any::<u8>(), 1..256)) {
        let pepper = test_pepper();
        let verse = find_verse("John 1:1").expect("test setup");

        let key_enc = derive_key(&pepper, &ikm, verse, "encryption");
        let key_sig = derive_key(&pepper, &ikm, verse, "signing");

        prop_assert!(key_enc.is_ok());
        prop_assert!(key_sig.is_ok());

        let enc_bytes = key_enc.expect("checked");
        let sig_bytes = key_sig.expect("checked");

        prop_assert_ne!(
            enc_bytes.as_bytes(),
            sig_bytes.as_bytes(),
            "different purposes must produce different keys"
        );
    }
}
