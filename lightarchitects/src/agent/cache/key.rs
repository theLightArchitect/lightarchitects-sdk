//! Cache key trait — canonical, deterministic hash input for [`SoulCache`].
//!
//! Any type implementing [`CacheKey`] can be stored in a [`SoulCache`].
//! The default implementation serialises via `serde_json` (stable, sorted-key
//! canonical form) and hashes with SHA-256.
//!
//! # Custom implementations
//!
//! Override `canonical_bytes` when JSON is not the natural wire form:
//!
//! ```rust
//! use lightarchitects::agent::cache::CacheKey;
//!
//! struct MyKey(u64);
//! impl CacheKey for MyKey {
//!     fn canonical_bytes(&self) -> Vec<u8> {
//!         self.0.to_le_bytes().to_vec()
//!     }
//! }
//! ```
//!
//! [`SoulCache`]: super::SoulCache

use sha2::{Digest, Sha256};

/// A cache key — produces canonical, deterministic bytes for SHA-256 hashing.
///
/// The default implementation serialises `self` as compact JSON (`serde_json` —
/// sorted-key, no whitespace) and is suitable for any `Serialize + Sync` type.
///
/// # Contract
///
/// - `canonical_bytes()` MUST be **deterministic**: same logical key → same bytes.
/// - `canonical_bytes()` MUST be **unique within namespace**: different logical
///   keys should produce different bytes (collision resistance is SHA-256's job
///   once bytes are fixed).
pub trait CacheKey: Send + Sync {
    /// Canonical byte form for SHA-256 hashing.
    ///
    /// Default: `serde_json::to_vec(self)` — compact, sorted-key JSON.
    fn canonical_bytes(&self) -> Vec<u8>;
}

/// Compute `SHA-256(bytes)` → 32-byte array.
///
/// Pure function — no allocations beyond the digest output.
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().into()
}

impl CacheKey for String {
    fn canonical_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl CacheKey for str {
    fn canonical_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl CacheKey for Vec<u8> {
    fn canonical_bytes(&self) -> Vec<u8> {
        self.clone()
    }
}

impl CacheKey for [u8] {
    fn canonical_bytes(&self) -> Vec<u8> {
        self.to_vec()
    }
}

/// Blanket: `&T` is a `CacheKey` when `T: CacheKey + ?Sized`.
impl<T: CacheKey + ?Sized> CacheKey for &T {
    fn canonical_bytes(&self) -> Vec<u8> {
        (**self).canonical_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheKey, sha256};

    #[test]
    fn string_key_deterministic() {
        let k = "hello".to_owned();
        assert_eq!(k.canonical_bytes(), k.canonical_bytes());
    }

    #[test]
    fn different_strings_different_hashes() {
        let h1 = sha256(&"a".to_owned().canonical_bytes());
        let h2 = sha256(&"b".to_owned().canonical_bytes());
        assert_ne!(h1, h2);
    }

    #[test]
    fn sha256_fixed_vector() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let digest = sha256(b"");
        assert_eq!(
            hex::encode(digest),
            "e3b0c44298fc1c149afbf4c8996fb924\
             27ae41e4649b934ca495991b7852b855"
        );
    }
}
