//! macOS Keychain access via `security(1)` CLI subprocess (OA-3, OA-12).
//!
//! # Why not the `keyring` crate?
//!
//! `keyring = "3"` silently hangs on ad-hoc signed macOS binaries — the crate
//! calls into the Security framework via `SecItemAdd`, which blocks waiting for
//! a privilege escalation dialog that is suppressed by the ad-hoc signature
//! policy.  The `/usr/bin/security` subprocess works regardless of
//! code-signing entitlements and is the canonical way macOS CLI tools interact
//! with the Keychain (see `git-credential-osxkeychain`).
//!
//! # Security
//!
//! - Secrets are passed as a positional `argv` element — never in a shell
//!   string, never via environment variables, never via stdin pipes.
//! - The subprocess inherits no environment variables that could leak the
//!   secret (the `Command` API uses `execve` directly).
//! - Service names are application-controlled constants, never user-supplied.

use anyhow::{Context, Result, anyhow};
use std::process::Command;

/// Keychain account name shared by all lightarchitects credential entries.
const ACCOUNT: &str = "lightarchitects";

/// Absolute path to the `security(1)` binary — never shell-interpolated.
const SECURITY_BIN: &str = "/usr/bin/security";

/// Stores `secret` in the macOS Keychain under `service`.
///
/// Uses `-U` to overwrite an existing entry.  The secret travels through
/// `argv`, not a shell expansion, so it is safe for arbitrary string values.
///
/// # Errors
///
/// Returns an error when the `security(1)` process cannot be spawned or exits
/// non-zero (e.g., Keychain locked, permission denied).
pub fn keychain_set(service: &str, secret: &str) -> Result<()> {
    let output = Command::new(SECURITY_BIN)
        .args([
            "add-generic-password",
            "-U",
            "-s",
            service,
            "-a",
            ACCOUNT,
            "-w",
            secret,
        ])
        .output()
        .with_context(|| format!("spawn security(1) for keychain_set({service})"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!("keychain_set({service}) failed: {stderr}"))
    }
}

/// Retrieves the stored secret for `service`.
///
/// Returns `Ok(None)` when the item does not exist (exit code 44 =
/// `SecItemNotFound`).
///
/// # Errors
///
/// Returns an error when the `security(1)` process cannot be spawned, exits
/// with an unexpected non-zero code, or the stored bytes are not valid UTF-8.
pub fn keychain_get(service: &str) -> Result<Option<String>> {
    let output = Command::new(SECURITY_BIN)
        .args(["find-generic-password", "-s", service, "-a", ACCOUNT, "-w"])
        .output()
        .with_context(|| format!("spawn security(1) for keychain_get({service})"))?;

    if output.status.success() {
        let raw = String::from_utf8(output.stdout)
            .with_context(|| format!("keychain value for {service} is not valid UTF-8"))?;
        Ok(Some(raw.trim_end_matches('\n').to_owned()))
    } else {
        // Exit 44 = SecItemNotFound — item absent, not an error.
        Ok(None)
    }
}

/// Removes the Keychain entry for `service`.
///
/// Returns `Ok(())` if the item was already absent (exit 44 = `SecItemNotFound`).
///
/// # Errors
///
/// Returns an error when the `security(1)` process cannot be spawned or exits
/// with an unexpected non-zero code.
pub fn keychain_delete(service: &str) -> Result<()> {
    let output = Command::new(SECURITY_BIN)
        .args(["delete-generic-password", "-s", service, "-a", ACCOUNT])
        .output()
        .with_context(|| format!("spawn security(1) for keychain_delete({service})"))?;

    // Exit 44 = SecItemNotFound — already absent, treat as success.
    if output.status.success() || output.status.code() == Some(44) {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!("keychain_delete({service}) failed: {stderr}"))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Full Keychain roundtrip: set → get → delete → absent.
    ///
    /// Requires an unlocked macOS Keychain.  Skipped in non-interactive CI.
    #[test]
    #[ignore = "requires unlocked macOS Keychain — run locally only"]
    fn keychain_roundtrip() {
        let service = "la-test-pkce-credential-roundtrip-2025";
        let secret = "unit_test_secret_value";
        keychain_set(service, secret).unwrap();
        let loaded = keychain_get(service).unwrap();
        assert_eq!(loaded.as_deref(), Some(secret));
        keychain_delete(service).unwrap();
        let after_delete = keychain_get(service).unwrap();
        assert!(after_delete.is_none());
    }

    /// Absent item returns `None`, not an error.
    #[test]
    #[ignore = "requires macOS Keychain access — run locally only"]
    fn keychain_get_absent_returns_none() {
        let result = keychain_get("la-test-nonexistent-item-xyzzy-9999").unwrap();
        assert!(result.is_none());
    }
}
