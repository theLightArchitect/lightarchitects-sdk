//! Resolves the embedded seccomp profile to a readable temp file.
//!
//! Docker requires a filesystem path for `--security-opt seccomp=<path>`.
//! This module writes the SDK's embedded profile to a `NamedTempFile` with
//! 0o400 permissions (owner-read only, world-unreadable) so no other process
//! can tamper with the profile between write and docker read.
//!
//! The returned `NamedTempFile` must be held alive until `docker run` returns.
//! It is deleted automatically via `Drop` — no signal-handler cleanup needed.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;

use lightarchitects::container_spawn::seccomp::SECCOMP_PROFILE_JSON;
use tempfile::NamedTempFile;

/// Writes the embedded seccomp profile JSON to a private temp file.
///
/// Returns a [`NamedTempFile`] that owns the file on disk.  The caller MUST
/// hold this handle until `docker run` has been called (Docker reads the file
/// when processing the `--security-opt` flag).  Dropping the handle removes
/// the file.
///
/// # Directory
///
/// Files are created in `~/.lightarchitects/seccomp/`, which is created if
/// absent.  Falling back to `std::env::temp_dir()` if `$HOME` is unavailable.
///
/// # Errors
///
/// Returns [`std::io::Error`] if the directory cannot be created, the file
/// cannot be written, or permissions cannot be set.
pub fn write_seccomp_profile() -> std::io::Result<NamedTempFile> {
    let seccomp_dir = dirs_next::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".lightarchitects")
        .join("seccomp");

    std::fs::create_dir_all(&seccomp_dir)?;

    let mut builder = tempfile::Builder::new();
    builder.prefix("la-seccomp-").suffix(".json").rand_bytes(16);

    let named = builder.tempfile_in(&seccomp_dir)?;

    // Set 0o400 — owner read-only, world-unreadable (H2 fix per SERAPH Round 2).
    named
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o400))?;

    // Write the embedded profile; the file's position is at zero after open.
    named.as_file().set_len(0)?;
    let mut file = named.as_file().try_clone()?;
    file.write_all(SECCOMP_PROFILE_JSON.as_bytes())?;
    file.flush()?;

    Ok(named)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn seccomp_profile_is_valid_json() {
        let tmp = write_seccomp_profile().unwrap();
        let content = std::fs::read_to_string(tmp.path()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(value.is_object(), "seccomp profile must be a JSON object");
    }

    #[test]
    fn seccomp_profile_has_restricted_permissions() {
        let tmp = write_seccomp_profile().unwrap();
        let meta = std::fs::metadata(tmp.path()).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o400,
            "seccomp temp file must be 0o400 (owner read-only)"
        );
    }
}
