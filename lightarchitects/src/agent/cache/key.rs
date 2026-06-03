//! [`CacheKey`] trait — canonical, deterministic hash input for [`SoulCache`].
//!
//! Mirrors the interface of `soul-cache-substrate::agent::cache::key` so the
//! combinator surface compiles and tests pass without the real SOUL store.
//!
//! [`SoulCache`]: super::SoulCache

/// A cache key — produces canonical, deterministic bytes for hashing.
///
/// The default blanket impl via `serde_json` covers all `Serialize + Sync`
/// types. Override `canonical_bytes` when JSON is not the natural wire form.
///
/// # Contract
///
/// - `canonical_bytes()` MUST be **deterministic**: same logical key → same bytes.
/// - `canonical_bytes()` MUST be **unique within namespace**: different logical
///   keys should produce different bytes.
pub trait CacheKey: Send + Sync {
    /// Canonical byte form for hashing.
    fn canonical_bytes(&self) -> Vec<u8>;
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

impl CacheKey for u32 {
    fn canonical_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

impl CacheKey for u64 {
    fn canonical_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

impl CacheKey for Vec<u8> {
    fn canonical_bytes(&self) -> Vec<u8> {
        self.clone()
    }
}

impl<T: CacheKey + ?Sized> CacheKey for &T {
    fn canonical_bytes(&self) -> Vec<u8> {
        (**self).canonical_bytes()
    }
}

/// SHA-256 hash of `bytes`, returning a 32-byte digest.
///
/// Converts [`CacheKey::canonical_bytes`] output into the fixed-width key
/// used by `SoulCache`'s moka L1 store and `HelixSoulCacheStore` L2 paths.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().into()
}
