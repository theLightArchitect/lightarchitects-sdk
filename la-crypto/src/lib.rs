//! # la-crypto
//!
//! Scripture-forged cryptographic foundation for the Light Architects ecosystem.
//!
//! Every key, token, and encryption operation derives from a single master pepper
//! via HKDF domain separation with KJV Scripture verses as context.
//!
//! ## Quick Start
//!
//! ```rust
//! use la_crypto::derive::derive_key;
//! use la_crypto::verses::find_verse;
//! use secrecy::SecretString;
//!
//! let pepper = SecretString::from("my-secret-pepper");
//! let verse = find_verse("John 1:1").expect("verse exists");
//! let key = derive_key(&pepper, b"input-material", verse, "my-purpose")
//!     .expect("derivation succeeds");
//! assert_eq!(key.as_bytes().len(), 32);
//! ```
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
//!
//! ## Feature Flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `keychain` | Yes | macOS Keychain backend via native Security Framework API |
//! | `env-store` | Yes | Environment variable backend (uses `unsafe` set_var/remove_var) |

// Workspace lints enforce `missing_docs` and `forbid(unsafe_code)`.
// EnvStore's set_var/remove_var have per-function #[allow(unsafe_code)] annotations.

pub mod compare;
pub mod derive;
pub mod encrypt;
/// Error types for la-crypto operations.
pub mod error;
pub mod hash;
pub mod random;
pub mod secrets;
pub mod sign;
pub mod verses;

pub use error::{CryptoError, Result};
