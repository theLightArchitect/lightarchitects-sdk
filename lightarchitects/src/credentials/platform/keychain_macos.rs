//! macOS keychain presence probe via `security(1)` subprocess.
//!
//! Exit-code-only: stdin/stdout/stderr are all redirected to `/dev/null`.
//! The keychain's data never enters our process; we only learn whether the
//! entry exists. Keychain-unlock prompt behavior is identical to a direct
//! CLI probe — if the keychain is locked, macOS surfaces its own dialog.

use std::process::Stdio;
use tokio::process::Command;

/// Probe the keychain for a generic-password entry with `service` and
/// `account`. Returns `Ok(true)` on exit 0, `Ok(false)` on any non-zero
/// exit (including the `errSecItemNotFound = 44` path), and propagates the
/// spawn error only if `security(1)` itself cannot be launched.
// Used by providers-anthropic and providers-openai callers; dead_code fires
// when those features are inactive under the `lightsquad`-only feature set.
#[allow(dead_code)]
pub(crate) async fn probe_keychain(service: &str, account: &str) -> std::io::Result<bool> {
    let status = Command::new("security")
        .args(["find-generic-password", "-s", service, "-a", account])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;
    Ok(status.success())
}
