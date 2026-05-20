//! Program.toml integrity lock.
//!
//! Provides a [`ManifestLock`] that captures the SHA-256 digest of a
//! `program.toml` byte slice and signs it with Ed25519, preventing both
//! accidental corruption and tampering.
//!
//! # Why sign the digest, not the raw bytes?
//!
//! Signing the raw SHA-256 digest (32 bytes) instead of the arbitrarily-long
//! TOML text prevents length-extension attacks and keeps the signing surface
//! minimal and constant-size.
//!
//! # Usage
//!
//! ```rust,no_run
//! use lightarchitects::lightsquad::manifest::ManifestLock;
//! use lightarchitects::crypto::sign::keypair_from_seed;
//!
//! let toml = b"[program]\nname = \"my-build\"";
//! let (signing_key, _vk) = keypair_from_seed(&[0x42u8; 32]);
//! let lock = ManifestLock::sign(toml, &signing_key);
//! assert!(lock.verify(toml));
//! ```

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::crypto::sign;

// ─── Error ───────────────────────────────────────────────────────────────────

/// Errors produced by [`ManifestLock`] operations.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// JSON serialisation of the lock failed.
    #[error("serialisation error: {0}")]
    Serialise(String),
}

/// Convenience result alias for manifest operations.
pub type Result<T> = std::result::Result<T, ManifestError>;

// ─── ManifestLock ─────────────────────────────────────────────────────────────

/// Integrity lock for a `program.toml` file.
///
/// Records the SHA-256 digest of the TOML bytes, an Ed25519 signature over
/// that digest, the UTC timestamp of signing, and the public verifying key
/// required for later verification.
///
/// Serialise to JSON with [`ManifestLock::to_json`] for storage alongside the
/// plan artefacts.
#[derive(Debug, Clone)]
pub struct ManifestLock {
    /// SHA-256 of the `program.toml` bytes at signing time.
    pub sha256: [u8; 32],
    /// Ed25519 signature over `sha256` (64 bytes).
    pub signature: Signature,
    /// UTC timestamp of signing.
    pub signed_at: DateTime<Utc>,
    /// The public verifying key corresponding to the signing key used.
    pub signer_pubkey: VerifyingKey,
}

impl ManifestLock {
    /// Create a new [`ManifestLock`] by hashing `toml_bytes` and signing the
    /// resulting SHA-256 digest with `signing_key`.
    ///
    /// The digest (not the raw TOML) is signed to keep the signing input
    /// constant-size and prevent length-extension attacks.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use lightarchitects::lightsquad::manifest::ManifestLock;
    /// use lightarchitects::crypto::sign::keypair_from_seed;
    ///
    /// let (sk, _vk) = keypair_from_seed(&[0x01u8; 32]);
    /// let lock = ManifestLock::sign(b"[program]", &sk);
    /// assert!(lock.verify(b"[program]"));
    /// ```
    #[must_use]
    pub fn sign(toml_bytes: &[u8], signing_key: &SigningKey) -> Self {
        let sha256 = sha256_digest(toml_bytes);
        let signature = sign::sign(signing_key, &sha256);
        let signer_pubkey = signing_key.verifying_key();
        Self {
            sha256,
            signature,
            signed_at: Utc::now(),
            signer_pubkey,
        }
    }

    /// Verify that `toml_bytes` match the recorded SHA-256 digest AND that the
    /// stored signature is valid over that digest.
    ///
    /// Returns `false` if either the digest or the signature does not match.
    /// Verification failure is a boolean result rather than an error because
    /// it is an expected condition (e.g., tampered or stale plan).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use lightarchitects::lightsquad::manifest::ManifestLock;
    /// use lightarchitects::crypto::sign::keypair_from_seed;
    ///
    /// let (sk, _vk) = keypair_from_seed(&[0x02u8; 32]);
    /// let lock = ManifestLock::sign(b"original", &sk);
    /// assert!(!lock.verify(b"tampered"));
    /// ```
    #[must_use]
    pub fn verify(&self, toml_bytes: &[u8]) -> bool {
        let digest = sha256_digest(toml_bytes);
        // Digest must match the recorded sha256 AND the signature must be valid.
        digest == self.sha256 && sign::verify(&self.signer_pubkey, &digest, &self.signature)
    }

    /// Serialise the lock to a [`serde_json::Value`].
    ///
    /// All binary fields are hex-encoded. The `signed_at` timestamp is RFC 3339.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError::Serialise`] if JSON construction fails (which
    /// should not happen in practice — all types are serialisable).
    pub fn to_json(&self) -> Result<Value> {
        let json = serde_json::json!({
            "sha256": hex_encode(&self.sha256),
            "signature": hex_encode(&self.signature.to_bytes()),
            "signed_at": self.signed_at.to_rfc3339(),
            "signer_pubkey": hex_encode(self.signer_pubkey.as_bytes()),
        });
        Ok(json)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Compute the SHA-256 digest of `data`, returning the raw 32-byte array.
fn sha256_digest(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Lowercase hex-encode `bytes`.
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from(HEX[usize::from(b >> 4)]));
        s.push(char::from(HEX[usize::from(b & 0x0f)]));
    }
    s
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::crypto::sign::keypair_from_seed;

    fn test_key() -> SigningKey {
        keypair_from_seed(&[0x42u8; 32]).0
    }

    #[test]
    fn sign_verify_roundtrip() {
        let sk = test_key();
        let toml = b"[program]\nname = \"test\"";
        let lock = ManifestLock::sign(toml, &sk);
        assert!(lock.verify(toml));
    }

    #[test]
    fn tampered_bytes_fail_verify() {
        let sk = test_key();
        let lock = ManifestLock::sign(b"original content", &sk);
        assert!(!lock.verify(b"tampered content"));
    }

    #[test]
    fn empty_toml_roundtrips() {
        let sk = test_key();
        let lock = ManifestLock::sign(b"", &sk);
        assert!(lock.verify(b""));
        assert!(!lock.verify(b"non-empty"));
    }

    #[test]
    fn sha256_field_matches_digest_of_input() {
        let sk = test_key();
        let toml = b"[program]\nname = \"digest-check\"";
        let lock = ManifestLock::sign(toml, &sk);
        let expected = sha256_digest(toml);
        assert_eq!(lock.sha256, expected);
    }

    #[test]
    fn to_json_fields_present() {
        let sk = test_key();
        let lock = ManifestLock::sign(b"[program]", &sk);
        let json = lock.to_json().expect("serialisation");
        assert!(json["sha256"].is_string(), "sha256 field missing");
        assert!(json["signature"].is_string(), "signature field missing");
        assert!(json["signed_at"].is_string(), "signed_at field missing");
        assert!(
            json["signer_pubkey"].is_string(),
            "signer_pubkey field missing"
        );
    }

    #[test]
    fn to_json_sha256_is_64_hex_chars() {
        let sk = test_key();
        let lock = ManifestLock::sign(b"[program]", &sk);
        let json = lock.to_json().expect("serialisation");
        let sha = json["sha256"].as_str().expect("sha256 string");
        assert_eq!(sha.len(), 64, "SHA-256 hex is 64 chars");
    }

    #[test]
    fn to_json_signature_is_128_hex_chars() {
        let sk = test_key();
        let lock = ManifestLock::sign(b"[program]", &sk);
        let json = lock.to_json().expect("serialisation");
        let sig = json["signature"].as_str().expect("signature string");
        assert_eq!(sig.len(), 128, "Ed25519 signature hex is 128 chars");
    }

    #[test]
    fn different_keys_produce_different_signatures() {
        let (sk_a, _) = keypair_from_seed(&[0x01u8; 32]);
        let (sk_b, _) = keypair_from_seed(&[0x02u8; 32]);
        let toml = b"[program]";
        let lock_a = ManifestLock::sign(toml, &sk_a);
        let lock_b = ManifestLock::sign(toml, &sk_b);
        assert_ne!(
            lock_a.signature.to_bytes(),
            lock_b.signature.to_bytes(),
            "different keys must produce different signatures"
        );
    }

    #[test]
    fn wrong_key_fails_verify() {
        let (sk_a, _) = keypair_from_seed(&[0x01u8; 32]);
        let (sk_b, vk_b) = keypair_from_seed(&[0x02u8; 32]);
        let toml = b"[program]";
        let mut lock = ManifestLock::sign(toml, &sk_a);
        // Swap the pubkey so verification uses the wrong key.
        lock.signer_pubkey = vk_b;
        // The signature was produced by sk_a but we're verifying with vk_b.
        assert!(!lock.verify(toml), "wrong pubkey should fail verify");
        // Keep the compiler happy about sk_b.
        drop(sk_b);
    }
}
