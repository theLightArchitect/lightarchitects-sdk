//! # la-crypto
//!
//! Scripture-forged cryptographic foundation for the Light Architects ecosystem.
//!
//! Every key, token, and encryption operation derives from a single master pepper
//! via HKDF domain separation with KJV Scripture verses as context.
//!
//! ## Module Overview
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`derive`] | HKDF key derivation with verse-based domain separation |
//! | [`hash`] | HMAC-SHA256 hashing and webhook signatures |
//! | [`encrypt`] | AES-256-GCM authenticated encryption |
//! | [`sign`] | Ed25519 digital signatures |
//! | [`secrets`] | Secret storage trait with Keychain/File/Env backends |
//! | [`verses`] | 147 curated 1611 KJV verses for cryptographic context |
//! | [`random`] | CSPRNG wrappers (key gen, nonce gen, salt gen) |
//! | [`compare`] | Constant-time comparison utilities |

pub mod compare;
pub mod derive;
pub mod encrypt;
pub mod error;
pub mod hash;
pub mod random;
pub mod secrets;
pub mod sign;
pub mod verses;

pub use error::{CryptoError, Result};
