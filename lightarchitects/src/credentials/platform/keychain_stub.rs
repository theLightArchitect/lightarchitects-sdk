//! No-op keychain probe for non-macOS platforms.
//!
//! libsecret (Linux) and Windows Credential Manager can be added behind
//! `cfg(target_os = "linux")` / `cfg(target_os = "windows")` branches
//! without breaking the signature.

pub(crate) async fn probe_keychain(_service: &str, _account: &str) -> std::io::Result<bool> {
    Ok(false)
}
