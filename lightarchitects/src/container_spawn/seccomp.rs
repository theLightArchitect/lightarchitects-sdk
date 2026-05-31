//! Embedded seccomp profile for Hardened and Airgapped containers.

/// Seccomp profile JSON embedded at compile time.
///
/// The profile uses `SCMP_ACT_ALLOW` as the default action and explicitly
/// blocks namespace-manipulation syscalls (`unshare`, `clone`, `setns`,
/// `mount`, `umount`, `umount2`, `pivot_root`) that would let a process
/// escape its container.
///
/// Write this string to a temporary file and pass the path via
/// `--security-opt seccomp=<path>` when spawning Hardened or Airgapped
/// containers.
pub const SECCOMP_PROFILE_JSON: &str = include_str!("seccomp-profile.json");
