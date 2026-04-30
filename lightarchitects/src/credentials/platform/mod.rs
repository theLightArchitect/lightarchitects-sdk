//! Platform-specific credential probe primitives.
//!
//! macOS uses a `security(1)` subprocess for keychain probes. Other
//! platforms return a no-op stub; libsecret / Credential Manager support
//! can be added behind additional `cfg` branches without breaking callers.

#[cfg(target_os = "macos")]
mod keychain_macos;
#[cfg(target_os = "macos")]
pub(crate) use keychain_macos::probe_keychain;

#[cfg(not(target_os = "macos"))]
mod keychain_stub;
#[cfg(not(target_os = "macos"))]
pub(crate) use keychain_stub::probe_keychain;
