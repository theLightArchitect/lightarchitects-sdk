//! Integration tests for la-crypto — full pipeline verification.
//!
//! These tests exercise the complete cryptographic pipeline end-to-end:
//! pepper → derive → hash/encrypt/sign → verify.

use l_arc_crypto::compare::constant_time_eq;
use l_arc_crypto::derive::{derive_api_key, derive_encryption_key, derive_key, derive_signing_key};
use l_arc_crypto::encrypt::{open, seal, seal_with_verse};
use l_arc_crypto::hash::{hmac_hash, hmac_verify};
use l_arc_crypto::secrets::{FileStore, SecretStore, auto_generate_and_persist, resolve_secret};
use l_arc_crypto::sign::{keypair_from_seed, sign, sign_with_verse, verify};
use l_arc_crypto::verses::{find_verse, random_verse};
use secrecy::{ExposeSecret, SecretString};

fn test_pepper() -> SecretString {
    SecretString::from("integration-test-pepper-32chars!!")
}

// ─── Pipeline 1: Pepper → Derive API Key → Hash → Verify ────────────────────

#[test]
fn pipeline_derive_hash_verify() {
    let pepper = test_pepper();
    let verse = find_verse("John 3:16").expect("test setup");

    // Derive an API key.
    let key = derive_api_key(&pepper, "test", verse).expect("derive");
    let raw = key.raw.expose_secret();

    // Key format is correct.
    assert!(raw.starts_with("lak_test_"));
    assert!(!key.hash.is_empty());
    assert_eq!(key.verse_ref, "John 3:16");

    // Hash the raw key and verify.
    let hash = hmac_hash(&pepper, raw.as_bytes()).expect("hash");
    assert!(
        hmac_verify(&pepper, raw.as_bytes(), &hash).expect("verify"),
        "hash should verify"
    );

    // Wrong data should not verify.
    assert!(
        !hmac_verify(&pepper, b"wrong-key", &hash).expect("verify"),
        "wrong data should fail"
    );
}

// ─── Pipeline 2: Derive Encryption Key → Seal → Open → Verify ───────────────

#[test]
fn pipeline_encrypt_decrypt() {
    let pepper = test_pepper();
    let derived = derive_encryption_key(&pepper, "vault").expect("derive");

    let plaintext = b"The LORD is my shepherd, I shall not want.";
    let aad = b"psalm-23:1";

    // Seal with derived key.
    let sealed = seal(derived.as_bytes(), plaintext, aad).expect("seal");

    // Open with same key + AAD.
    let recovered = open(derived.as_bytes(), &sealed, aad).expect("open");
    assert_eq!(recovered, plaintext);

    // Wrong AAD fails.
    let wrong_aad = open(derived.as_bytes(), &sealed, b"wrong-context");
    assert!(wrong_aad.is_err(), "wrong AAD should fail");

    // Wrong key fails.
    let wrong_key = [0xFFu8; 32];
    let wrong = open(&wrong_key, &sealed, aad);
    assert!(wrong.is_err(), "wrong key should fail");
}

// ─── Pipeline 3: Derive Signing Key → Sign → Verify → Tamper → Reject ───────

#[test]
fn pipeline_sign_verify_tamper() {
    let pepper = test_pepper();
    let derived = derive_signing_key(&pepper, "evidence-chain").expect("derive");

    let (sk, vk) = keypair_from_seed(derived.as_bytes());
    let message = b"Evidence entry: scan complete, 0 findings.";

    // Sign.
    let sig = sign(&sk, message);

    // Verify.
    assert!(verify(&vk, message, &sig), "signature should verify");

    // Tamper → reject.
    let tampered = b"Evidence entry: scan complete, 3 findings.";
    assert!(
        !verify(&vk, tampered, &sig),
        "tampered message should fail verification"
    );
}

// ─── Cross-Derivation Independence: Same Pepper + Different Verse ────────────

#[test]
fn cross_derivation_different_verse() {
    let pepper = test_pepper();
    let v1 = find_verse("John 1:1").expect("test setup");
    let v2 = find_verse("John 3:16").expect("test setup");
    let ikm = b"same-input-keying-material";

    let a = derive_key(&pepper, ikm, v1, "test").expect("derive a");
    let b = derive_key(&pepper, ikm, v2, "test").expect("derive b");

    assert_ne!(
        a.as_bytes(),
        b.as_bytes(),
        "same pepper + different verse = different keys"
    );
}

// ─── Cross-Purpose Independence: Same Pepper + Same Verse + Different Purpose ─

#[test]
fn cross_purpose_different_purpose() {
    let pepper = test_pepper();
    let verse = find_verse("John 1:1").expect("test setup");
    let ikm = b"same-input-keying-material";

    let enc = derive_key(&pepper, ikm, verse, "encryption").expect("derive enc");
    let sig = derive_key(&pepper, ikm, verse, "signing").expect("derive sig");
    let api = derive_key(&pepper, ikm, verse, "api-key:test").expect("derive api");

    assert_ne!(
        enc.as_bytes(),
        sig.as_bytes(),
        "encryption vs signing = different keys"
    );
    assert_ne!(
        enc.as_bytes(),
        api.as_bytes(),
        "encryption vs api-key = different keys"
    );
    assert_ne!(
        sig.as_bytes(),
        api.as_bytes(),
        "signing vs api-key = different keys"
    );
}

// ─── Verse-Based Encrypt Pipeline ────────────────────────────────────────────

#[test]
fn pipeline_seal_with_verse() {
    let pepper = test_pepper();
    let verse = find_verse("Lamentations 3:22-23").expect("test setup");

    let plaintext = b"Great is thy faithfulness.";
    let aad = b"soul-vault-entry";

    // seal_with_verse now returns (SealedData, DerivedBytes) — caller can decrypt.
    let (sealed, key) = seal_with_verse(&pepper, verse, plaintext, aad).expect("seal");
    assert!(!sealed.ciphertext_with_tag.is_empty());
    assert_eq!(sealed.nonce.len(), 12);

    // Verify we can decrypt with the returned key.
    let recovered = open(key.as_bytes(), &sealed, aad).expect("open");
    assert_eq!(recovered, plaintext, "verse-sealed data should roundtrip");
}

// ─── Verse-Based Sign Pipeline ───────────────────────────────────────────────

#[test]
fn pipeline_sign_with_verse() {
    let pepper = test_pepper();
    let verse = find_verse("Ephesians 6:11").expect("test setup");
    let message = b"Put on the whole armour of God.";

    let (sig, vk) = sign_with_verse(&pepper, verse, message).expect("sign");

    // Verify with the returned public key.
    assert!(
        verify(&vk, message, &sig),
        "verse-derived signature should verify"
    );

    // Tamper → reject.
    assert!(
        !verify(&vk, b"tampered armour", &sig),
        "tampered message should fail"
    );
}

// ─── SecretStore Resolution Pipeline ─────────────────────────────────────────

#[test]
fn pipeline_secret_store_resolve() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("test-secrets.toml");
    let store = FileStore::with_path(path);

    // Auto-generate and persist a pepper (now returns SecretString).
    let generated = auto_generate_and_persist("hmac-pepper", &store, 32).expect("generate");
    let gen_exposed = generated.expose_secret();
    assert_eq!(gen_exposed.len(), 64, "32 bytes = 64 hex chars");

    // Resolve should find it (returns SecretString).
    let stores: Vec<&dyn SecretStore> = vec![&store];
    let found = resolve_secret("hmac-pepper", &stores).expect("resolve");
    let found_exposed: Option<&str> = found.as_ref().map(ExposeSecret::expose_secret);
    assert_eq!(found_exposed, Some(gen_exposed));

    // Use the generated pepper for an HMAC operation.
    let hash = hmac_hash(&generated, b"test-data").expect("hmac");
    assert!(hmac_verify(&generated, b"test-data", &hash).expect("verify"));
}

// ─── Full Ecosystem Pipeline: Generate → Derive → Encrypt → Sign ────────────

#[test]
fn full_ecosystem_pipeline() {
    let pepper = test_pepper();
    let verse = random_verse();

    // Step 1: Derive API key.
    let api_key = derive_api_key(&pepper, "prod", verse).expect("api key");
    assert!(api_key.raw.expose_secret().starts_with("lak_prod_"));

    // Step 2: Derive encryption key and encrypt the API key hash.
    let enc_key = derive_encryption_key(&pepper, "api-key-backup").expect("enc key");
    let sealed = seal(
        enc_key.as_bytes(),
        api_key.hash.as_bytes(),
        b"api-key-backup",
    )
    .expect("seal");

    // Step 3: Decrypt and verify the hash matches.
    let decrypted = open(enc_key.as_bytes(), &sealed, b"api-key-backup").expect("open");
    assert!(
        constant_time_eq(&decrypted, api_key.hash.as_bytes()),
        "decrypted hash should match original"
    );

    // Step 4: Sign the API key hash for non-repudiation.
    let sig_key = derive_signing_key(&pepper, "api-key-audit").expect("sig key");
    let (sk, vk) = keypair_from_seed(sig_key.as_bytes());
    let signature = sign(&sk, api_key.hash.as_bytes());
    assert!(
        verify(&vk, api_key.hash.as_bytes(), &signature),
        "audit signature should verify"
    );
}
