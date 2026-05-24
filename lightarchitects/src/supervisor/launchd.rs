//! macOS launchd integration for the lightsquad supervisor service.
//!
//! Provides the plist template and `launchctl` helpers to install, load, and
//! unload the `io.lightarchitects.supervisor` [`LaunchAgent`].
//!
//! # Platform scope
//!
//! This module is compiled **only on macOS** (`target_os = "macos"`).
//! All public items are wrapped in `#[cfg(target_os = "macos")]`.

#[cfg(target_os = "macos")]
mod inner {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
    };
    use thiserror::Error;

    // ── Constants ─────────────────────────────────────────────────────────────

    /// The launchd plist label for the lightsquad supervisor [`LaunchAgent`].
    pub const PLIST_LABEL: &str = "io.lightarchitects.supervisor";

    /// Subdirectory under `~/Library/Logs/` where supervisor output is written.
    const LOG_SUBDIR: &str = "lightarchitects";

    /// Log file name written inside `LOG_SUBDIR`.
    const LOG_FILE: &str = "supervisor.log";

    // ── Error type ────────────────────────────────────────────────────────────

    /// Errors that can occur during launchd plist management or service control.
    #[derive(Debug, Error)]
    pub enum LaunchdError {
        /// The user's home directory could not be resolved.
        #[error("home directory not found")]
        HomeDirNotFound,

        /// A filesystem operation (read, write, create) failed.
        #[error("filesystem error at `{path}`: {source}")]
        Io {
            /// The path involved in the failing operation.
            path: PathBuf,
            /// The underlying I/O error.
            #[source]
            source: std::io::Error,
        },

        /// `launchctl` exited with a non-zero status.
        #[error("launchctl {subcommand} failed (exit {code}): {stderr}")]
        LaunchctlFailed {
            /// The launchctl subcommand that failed (e.g. `"load"`, `"unload"`).
            subcommand: &'static str,
            /// Exit code returned by launchctl.
            code: i32,
            /// Stderr captured from the launchctl invocation.
            stderr: String,
        },

        /// `launchctl` process could not be spawned or waited on.
        #[error("failed to run launchctl {subcommand}: {source}")]
        LaunchctlSpawn {
            /// The launchctl subcommand that could not be spawned.
            subcommand: &'static str,
            /// The underlying I/O error.
            #[source]
            source: std::io::Error,
        },
    }

    // ── Plist template ────────────────────────────────────────────────────────

    /// Returns the launchd plist XML template as a static string.
    ///
    /// The caller **must** substitute the literal `{binary_path}` placeholder
    /// with the absolute path to the supervisor binary before writing the plist
    /// to disk (see [`install_plist`]).
    ///
    /// # Template guarantees
    ///
    /// - Uses `caffeinate -dimsu -- {binary_path}` as `ProgramArguments` to
    ///   prevent macOS sleep during long builds.
    /// - `KeepAlive` is `true` — launchd restarts the service on exit.
    /// - `RunAtLoad` is `false` — the operator must explicitly start the service
    ///   with `launchctl load` or `launchctl start`.
    /// - `StandardOutPath` and `StandardErrorPath` both write to
    ///   `~/Library/Logs/lightarchitects/supervisor.log`. The `~` is not
    ///   expanded by launchd for these keys; callers should substitute the
    ///   literal `$HOME` or expand it before writing (see [`install_plist`]).
    pub fn plist_template() -> &'static str {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>io.lightarchitects.supervisor</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/bin/caffeinate</string>
        <string>-dimsu</string>
        <string>--</string>
        <string>{binary_path}</string>
    </array>

    <key>KeepAlive</key>
    <true/>

    <key>RunAtLoad</key>
    <false/>

    <key>StandardOutPath</key>
    <string>{log_path}</string>

    <key>StandardErrorPath</key>
    <string>{log_path}</string>
</dict>
</plist>
"#
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Returns the resolved path to `~/Library/LaunchAgents/`.
    fn launch_agents_dir(home: &Path) -> PathBuf {
        home.join("Library").join("LaunchAgents")
    }

    /// Returns the resolved path to `~/Library/Logs/lightarchitects/supervisor.log`.
    fn log_path(home: &Path) -> PathBuf {
        home.join("Library")
            .join("Logs")
            .join(LOG_SUBDIR)
            .join(LOG_FILE)
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Write the rendered plist to
    /// `~/Library/LaunchAgents/io.lightarchitects.supervisor.plist`.
    ///
    /// Substitutes `{binary_path}` with `binary_path` and `{log_path}` with
    /// the expanded `~/Library/Logs/lightarchitects/supervisor.log` path.
    /// Parent directories are created if they do not already exist.
    ///
    /// # Errors
    ///
    /// Returns [`LaunchdError::HomeDirNotFound`] when the home directory cannot
    /// be resolved, or [`LaunchdError::Io`] on any filesystem failure.
    pub fn install_plist(binary_path: &Path) -> Result<PathBuf, LaunchdError> {
        let home = dirs::home_dir().ok_or(LaunchdError::HomeDirNotFound)?;

        let agents_dir = launch_agents_dir(&home);
        fs::create_dir_all(&agents_dir).map_err(|source| LaunchdError::Io {
            path: agents_dir.clone(),
            source,
        })?;

        let log = log_path(&home);
        // Ensure the log directory exists so launchd can write on first launch.
        if let Some(log_dir) = log.parent() {
            fs::create_dir_all(log_dir).map_err(|source| LaunchdError::Io {
                path: log_dir.to_owned(),
                source,
            })?;
        }

        let plist_path = agents_dir.join(format!("{PLIST_LABEL}.plist"));
        let binary_str = binary_path.display().to_string();
        let log_str = log.display().to_string();

        let rendered = plist_template()
            .replace("{binary_path}", &binary_str)
            .replace("{log_path}", &log_str);

        fs::write(&plist_path, rendered).map_err(|source| LaunchdError::Io {
            path: plist_path.clone(),
            source,
        })?;

        Ok(plist_path)
    }

    /// Register the supervisor service with launchd via `launchctl load -w`.
    ///
    /// The service is not automatically started (`RunAtLoad = false`); use
    /// `launchctl start io.lightarchitects.supervisor` or restart to activate.
    ///
    /// # Errors
    ///
    /// Returns [`LaunchdError::LaunchctlSpawn`] if the `launchctl` binary
    /// cannot be executed, or [`LaunchdError::LaunchctlFailed`] if it exits
    /// with a non-zero status.
    pub fn load(plist_path: &Path) -> Result<(), LaunchdError> {
        run_launchctl("load", &["-w", &plist_path.display().to_string()])
    }

    /// Deregister the supervisor service from launchd via `launchctl unload -w`.
    ///
    /// If the service is currently running, launchd will stop it before
    /// removing the registration.
    ///
    /// # Errors
    ///
    /// Returns [`LaunchdError::LaunchctlSpawn`] if the `launchctl` binary
    /// cannot be executed, or [`LaunchdError::LaunchctlFailed`] if it exits
    /// with a non-zero status.
    pub fn unload(plist_path: &Path) -> Result<(), LaunchdError> {
        run_launchctl("unload", &["-w", &plist_path.display().to_string()])
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Invoke `launchctl <subcommand> [args...]` synchronously.
    ///
    /// Captures stderr for diagnostics. Stdout is ignored.
    fn run_launchctl(subcommand: &'static str, args: &[&str]) -> Result<(), LaunchdError> {
        let output = Command::new("launchctl")
            .arg(subcommand)
            .args(args)
            .output()
            .map_err(|source| LaunchdError::LaunchctlSpawn { subcommand, source })?;

        if output.status.success() {
            return Ok(());
        }

        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        Err(LaunchdError::LaunchctlFailed {
            subcommand,
            code,
            stderr,
        })
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    mod tests {
        use super::*;
        use std::path::Path;

        // ── plist_template tests ──────────────────────────────────────────────

        #[test]
        fn plist_template_contains_label() {
            let tmpl = plist_template();
            assert!(
                tmpl.contains(PLIST_LABEL),
                "plist must embed the canonical label"
            );
        }

        #[test]
        fn plist_template_contains_binary_placeholder() {
            let tmpl = plist_template();
            assert!(
                tmpl.contains("{binary_path}"),
                "plist template must contain the {{binary_path}} placeholder"
            );
        }

        #[test]
        fn plist_template_contains_log_placeholder() {
            let tmpl = plist_template();
            assert!(
                tmpl.contains("{log_path}"),
                "plist template must contain the {{log_path}} placeholder"
            );
        }

        #[test]
        fn plist_template_contains_caffeinate() {
            let tmpl = plist_template();
            assert!(
                tmpl.contains("caffeinate"),
                "plist must use caffeinate to prevent sleep during builds"
            );
        }

        #[test]
        fn plist_template_caffeinate_flags() {
            let tmpl = plist_template();
            assert!(
                tmpl.contains("-dimsu"),
                "caffeinate flags -dimsu must be present"
            );
        }

        #[test]
        fn plist_template_keep_alive_true() {
            let tmpl = plist_template();
            // <key>KeepAlive</key> followed by <true/>
            let ka_pos = tmpl
                .find("KeepAlive")
                .expect("KeepAlive key must be present");
            let after = &tmpl[ka_pos..];
            assert!(after.contains("<true/>"), "KeepAlive must be set to true");
        }

        #[test]
        fn plist_template_run_at_load_false() {
            let tmpl = plist_template();
            let ral_pos = tmpl
                .find("RunAtLoad")
                .expect("RunAtLoad key must be present");
            let after = &tmpl[ral_pos..];
            assert!(
                after.contains("<false/>"),
                "RunAtLoad must be false — operator starts the service explicitly"
            );
        }

        #[test]
        fn plist_template_is_valid_xml_prolog() {
            let tmpl = plist_template();
            assert!(
                tmpl.starts_with("<?xml"),
                "plist must begin with the XML declaration"
            );
        }

        // ── install_plist rendering test (pure, no FS writes) ─────────────────

        #[test]
        fn rendered_plist_substitutes_binary_path() {
            let fake_binary = Path::new("/usr/local/bin/lightsquad-supervisor");
            let rendered =
                plist_template().replace("{binary_path}", &fake_binary.display().to_string());
            assert!(
                rendered.contains("/usr/local/bin/lightsquad-supervisor"),
                "rendered plist must contain the substituted binary path"
            );
            assert!(
                !rendered.contains("{binary_path}"),
                "rendered plist must not contain the raw placeholder"
            );
        }

        // ── LaunchdError display tests ─────────────────────────────────────────

        #[test]
        fn launchd_error_home_dir_not_found_display() {
            let err = LaunchdError::HomeDirNotFound;
            assert_eq!(err.to_string(), "home directory not found");
        }

        #[test]
        fn launchd_error_io_display() {
            let source = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
            let err = LaunchdError::Io {
                path: PathBuf::from("/tmp/test.plist"),
                source,
            };
            let msg = err.to_string();
            assert!(msg.contains("/tmp/test.plist"), "error must name the path");
            assert!(
                msg.contains("filesystem error"),
                "error kind prefix present"
            );
        }

        #[test]
        fn launchd_error_launchctl_failed_display() {
            let err = LaunchdError::LaunchctlFailed {
                subcommand: "load",
                code: 1,
                stderr: "service already loaded".to_owned(),
            };
            let msg = err.to_string();
            assert!(msg.contains("load"), "subcommand in message");
            assert!(msg.contains('1'), "exit code in message");
            assert!(msg.contains("service already loaded"), "stderr in message");
        }
    }
}

// ── Re-export flat public surface ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub use inner::{LaunchdError, PLIST_LABEL, install_plist, load, unload};

#[cfg(target_os = "macos")]
pub use inner::plist_template;

/// Non-macOS stub — launchd is macOS-only; this module intentionally empty.
#[cfg(not(target_os = "macos"))]
pub mod _non_macos {}
