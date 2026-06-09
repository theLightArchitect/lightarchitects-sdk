//! `lightarchitects serve-webshell` — idempotent, cross-platform webshell launcher.
//!
//! Unlike `webshell start` (which blocks until the process exits), this command
//! spawns the webshell detached and returns immediately.  Running it when the
//! webshell is already listening is a no-op — no duplicate process is created.
//!
//! Port probe uses [`std::net::TcpStream::connect_timeout`] — no `lsof`,
//! no `PowerShell`, no OS-specific tooling.  Works on macOS, Linux, and Windows.

use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::config::GatewayConfig;
use crate::error::GatewayError;

/// Default port the webshell HTTP server listens on.
const DEFAULT_PORT: u16 = 8733;

/// How long to wait when probing the port before concluding nothing is there.
const PROBE_TIMEOUT: Duration = Duration::from_millis(200);

/// Start the webshell HTTP server if it is not already running on `--port`.
///
/// Idempotent: if a process is already accepting connections on the target port
/// this function prints a notice and returns `Ok(())` without spawning anything.
///
/// # Errors
///
/// Returns [`GatewayError::SpawnFailed`] if the binary cannot be found or the
/// OS rejects the `spawn()` call.
pub fn execute(config: &GatewayConfig, args: &[String]) -> Result<(), GatewayError> {
    let port = parse_flag(args, "--port")
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);

    // Cross-platform port probe.
    // Success  → something is already listening → nothing to do.
    // Err      → port is free → fall through to spawn.
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    if TcpStream::connect_timeout(&addr, PROBE_TIMEOUT).is_ok() {
        println!("webshell already running on :{port}");
        return Ok(());
    }

    let mut cmd = Command::new(resolve_binary(config));
    cmd.arg("--port").arg(port.to_string());

    // WHY: on Windows, Command::spawn() inherits the parent's console handle.
    // The child process is then killed when the parent terminal closes.
    // DETACHED_PROCESS severs that link so the webshell outlives the shell.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(DETACHED_PROCESS);
    }

    cmd.spawn().map_err(|e| GatewayError::SpawnFailed {
        agent: "webshell".to_owned(),
        reason: format!("failed to spawn webshell: {e}"),
    })?;

    println!("webshell started on :{port}");
    Ok(())
}

/// Resolve the webshell binary path.
///
/// Checks agent config first, then falls back to the platform deploy path:
/// `~/.lightarchitects/bin/lightspace[.exe]`.
fn resolve_binary(config: &GatewayConfig) -> PathBuf {
    config.agents.get("webshell").map_or_else(
        || {
            // HOME on Unix; USERPROFILE on Windows (HOME may be absent there).
            let home = std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .unwrap_or_default();
            let bin_name = if cfg!(windows) {
                "lightspace.exe"
            } else {
                "lightspace"
            };
            PathBuf::from(home)
                .join(".lightarchitects")
                .join("bin")
                .join(bin_name)
        },
        super::super::config::AgentConfig::binary_path,
    )
}

/// Parse a `--flag <value>` pair from a raw argument slice.
fn parse_flag(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}
