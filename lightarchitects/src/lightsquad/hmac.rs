//! Per-wave HKDF subkey derivation.
//!
//! Derives a unique 32-byte secret key for each wave of a lightsquad build
//! from a build-level master key. Domain-separation uses the `build_id` and
//! `wave_index` so that compromise of one wave key does not reveal the master
//! key or any sibling wave key.
//!
//! # Key derivation scheme
//!
//! ```text
//! wave_key = HKDF-SHA256(
//!     salt   = <none>   (HKDF extract with empty salt uses SHA-256 hash of zero block)
//!     ikm    = master_key,
//!     info   = "lightsquad:wave:{build_id}:{wave_index}",
//!     L      = 32,
//! )
//! ```
//!
//! The `info` string provides domain separation: the same master key with
//! different `build_id` or `wave_index` values produces completely independent
//! key material (HKDF one-way property).
//!
//! # Zeroisation
//!
//! [`WaveKeyMaterial`] wraps the derived bytes in [`zeroize::Zeroizing`]; the
//! secret is erased from memory when the struct is dropped.

use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

// ─── WaveKeyMaterial ──────────────────────────────────────────────────────────

/// Per-wave key material derived from the build master key.
///
/// The secret bytes are automatically zeroed on drop via [`Zeroizing`].
/// Debug output is redacted to prevent key material from appearing in logs.
pub struct WaveKeyMaterial {
    /// Wave index this key was derived for.
    pub wave_index: u32,
    /// 32-byte derived secret key — zeroed on drop.
    pub key: Zeroizing<[u8; 32]>,
}

impl std::fmt::Debug for WaveKeyMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WaveKeyMaterial")
            .field("wave_index", &self.wave_index)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl WaveKeyMaterial {
    /// Access the raw 32-byte derived key.
    ///
    /// The returned reference is valid for the lifetime of this struct.
    /// Do not copy or log the key bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }
}

// ─── derive_wave_key ─────────────────────────────────────────────────────────

/// Derive a per-wave secret key from a build-level master key.
///
/// The `info` parameter binds the derived key to a specific `build_id` and
/// `wave_index`, ensuring keys are domain-separated across builds and waves.
///
/// # Arguments
///
/// - `master_key` — 32-byte build-level master secret.
/// - `build_id` — unique identifier for the build (e.g., a codename or UUID).
/// - `wave_index` — zero-based index of the wave within the build.
///
/// # Examples
///
/// ```rust,no_run
/// use lightarchitects::lightsquad::hmac::derive_wave_key;
///
/// let master = [0x42u8; 32];
/// let wkm = derive_wave_key(&master, "ironclaw-spine", 0);
/// assert_eq!(wkm.wave_index, 0);
/// assert_ne!(wkm.as_bytes(), &[0u8; 32]);
/// ```
#[must_use]
pub fn derive_wave_key(master_key: &[u8; 32], build_id: &str, wave_index: u32) -> WaveKeyMaterial {
    let info = format!("lightsquad:wave:{build_id}:{wave_index}");
    let hk = Hkdf::<Sha256>::new(None, master_key);
    let mut okm = Zeroizing::new([0u8; 32]);
    // HKDF expand with 32-byte output cannot fail: 32 <= 255 * 32 (SHA-256 max).
    // The `expand` error type is `InvalidLength`, only triggered when L > 255 * HashLen.
    // We use a fixed L=32, so this is statically safe. Document for auditors.
    let _ = hk.expand(info.as_bytes(), okm.as_mut());
    WaveKeyMaterial {
        wave_index,
        key: okm,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn master() -> [u8; 32] {
        let mut k = [0u8; 32];
        k[0] = 0xAB;
        k[31] = 0xCD;
        k
    }

    #[test]
    fn derive_wave_key_produces_32_bytes() {
        let wkm = derive_wave_key(&master(), "test-build", 0);
        assert_eq!(wkm.as_bytes().len(), 32);
    }

    #[test]
    fn derive_wave_key_deterministic() {
        let a = derive_wave_key(&master(), "test-build", 0);
        let b = derive_wave_key(&master(), "test-build", 0);
        assert_eq!(
            a.as_bytes(),
            b.as_bytes(),
            "same inputs must yield same key"
        );
    }

    #[test]
    fn different_wave_index_different_key() {
        let a = derive_wave_key(&master(), "test-build", 0);
        let b = derive_wave_key(&master(), "test-build", 1);
        assert_ne!(
            a.as_bytes(),
            b.as_bytes(),
            "different wave_index must yield different key"
        );
    }

    #[test]
    fn different_build_id_different_key() {
        let a = derive_wave_key(&master(), "build-alpha", 0);
        let b = derive_wave_key(&master(), "build-beta", 0);
        assert_ne!(
            a.as_bytes(),
            b.as_bytes(),
            "different build_id must yield different key"
        );
    }

    #[test]
    fn different_master_different_key() {
        let mut master_b = master();
        master_b[0] ^= 0xFF;
        let a = derive_wave_key(&master(), "same-build", 0);
        let b = derive_wave_key(&master_b, "same-build", 0);
        assert_ne!(
            a.as_bytes(),
            b.as_bytes(),
            "different master key must yield different wave key"
        );
    }

    #[test]
    fn wave_index_field_preserved() {
        let wkm = derive_wave_key(&master(), "build", 7);
        assert_eq!(wkm.wave_index, 7);
    }

    #[test]
    fn derived_key_not_all_zeros() {
        let wkm = derive_wave_key(&master(), "test-build", 0);
        assert!(
            wkm.as_bytes().iter().any(|&b| b != 0),
            "derived key should have entropy"
        );
    }

    #[test]
    fn debug_redacts_key() {
        let wkm = derive_wave_key(&master(), "test", 0);
        let debug_str = format!("{wkm:?}");
        assert!(
            debug_str.contains("REDACTED"),
            "Debug must not expose key bytes"
        );
        assert!(
            debug_str.contains("wave_index"),
            "Debug should include wave_index"
        );
    }
}
