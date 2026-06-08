//! Per-session HMAC seed bootstrapping.
//!
//! Each session receives a 32-byte CSPRNG seed at creation time.  The seed
//! is used by [`super::persist`] to chain HMAC tags across the NDJSON event
//! log, giving the replay endpoint a tamper-evident integrity check.
//!
//! Seeds are ephemeral — they live in memory for the process lifetime and are
//! NOT persisted to Keychain or disk.  This is intentional: the log integrity
//! guarantee is "no in-process tampering", not "cross-restart continuity".

use rand::RngCore;

/// A 32-byte session-scoped HMAC seed.
pub type HmacSeed = [u8; 32];

/// Mint a fresh [`HmacSeed`] using the OS CSPRNG.
#[must_use]
pub fn new_seed() -> HmacSeed {
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    seed
}
