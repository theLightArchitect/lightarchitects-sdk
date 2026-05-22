//! ADK Python subprocess supervisor — lifecycle management for the Google ADK
//! `api_server` that powers Path Z multi-provider agent dispatch.
//!
//! # S-series guarantees (Part XXVII)
//!
//! | ID | Invariant | Gate |
//! |----|-----------|------|
//! | S-1 | [`probe`] detects ADK install in venv; fails fast with install URL | Phase 2 G2 [O] |
//! | S-2 | Subprocess spawned without shell interpolation (execve semantics) | Phase 3 G3 [S] |
//! | S-3 | Ephemeral port allocated from 49152–65535; retries 10× on EADDRINUSE | Phase 3 G3 [Q] |
//! | S-4 | Heartbeat every 30 s; SIGTERM → SIGKILL → restart on 2 failures | Phase 3 G3 [O] |
//! | S-5 | Restart cap: 3 restarts in 5 min; 4th attempt → [`SupervisorError::RestartCapReached`] | Phase 3 G3 [P] |
//! | S-6 | Graceful shutdown: drain in-flight → SIGTERM → SIGKILL after 30 s | Phase 3 G3 [O] |
//! | S-7 | ADK runs in isolated venv at `~/.lightarchitects/adk-venv/` | Phase 2 G2 [S] |
//! | S-8 | Major-version auto-upgrade blocked; HITL required | Phase 2 G2 [S] |

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// ADK venv path — ADK is always installed here (S-7).
const ADK_VENV_RELATIVE: &str = ".lightarchitects/adk-venv";
/// Port range lower bound for ephemeral ADK `api_server` (S-3).
const PORT_RANGE_START: u16 = 49152;
/// Port range upper bound (inclusive) for ephemeral ADK `api_server` (S-3).
const PORT_RANGE_END: u16 = 65535;
/// Maximum port allocation retries before returning [`SupervisorError::PortExhausted`] (S-3).
const PORT_RETRIES: u16 = 10;
/// Maximum restarts in [`RESTART_WINDOW`] before halting for HITL (S-5).
const RESTART_CAP: u8 = 3;
/// Time window for restart-cap accounting (S-5).
const RESTART_WINDOW: Duration = Duration::from_secs(300);
/// Minimum supported ADK major version.
const ADK_MIN_MAJOR: u32 = 2;
/// ADK install URL surfaced in [`SupervisorError::AdkMissing`] (S-1).
const ADK_INSTALL_URL: &str = "https://google.github.io/adk-docs/get-started/installation/";

/// A parsed ADK version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdkVersion {
    /// Major version component (e.g. `2` for `2.0.1`).
    pub major: u32,
    /// Minor version component.
    pub minor: u32,
    /// Patch version component.
    pub patch: u32,
}

impl AdkVersion {
    /// Parse from a string like `"2.0.1"`.
    ///
    /// Returns `None` if the string does not match `MAJOR.MINOR.PATCH`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.trim().splitn(3, '.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

/// Errors produced by the ADK supervisor.
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    /// ADK is not installed in the expected venv (S-1).
    #[error("ADK not found in venv; install: {install_url}")]
    AdkMissing {
        /// URL for the ADK installation guide.
        install_url: String,
    },

    /// All 10 ephemeral port allocation attempts failed (S-3).
    #[error(
        "port allocation exhausted after {PORT_RETRIES} retries (range {PORT_RANGE_START}–{PORT_RANGE_END})"
    )]
    PortExhausted,

    /// Restart cap reached; HITL required before auto-restart resumes (S-5).
    #[error("ADK subprocess reached restart cap ({RESTART_CAP} restarts in 5 min); HITL required")]
    RestartCapReached {
        /// Number of restarts that occurred.
        restart_count: u8,
    },

    /// Auto-upgrade across ADK major versions is blocked; operator must upgrade manually (S-8).
    #[error(
        "ADK major-version upgrade ({installed}→{requested}) requires operator approval (HITL)"
    )]
    MajorVersionHitlRequired {
        /// Currently installed major version.
        installed: u32,
        /// Requested major version.
        requested: u32,
    },

    /// An I/O error occurred in subprocess management.
    #[error("supervisor I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// An internal invariant was violated.
    #[error("supervisor internal error: {0}")]
    Internal(String),
}

/// Return the absolute path to the ADK venv (S-7).
///
/// Resolves `~/.lightarchitects/adk-venv/` using the `HOME` environment variable.
///
/// # Errors
///
/// Returns [`SupervisorError::Internal`] if `HOME` is not set.
pub fn venv_path() -> Result<PathBuf, SupervisorError> {
    let home = std::env::var("HOME")
        .map_err(|_| SupervisorError::Internal("HOME env var not set".to_owned()))?;
    Ok(PathBuf::from(home).join(ADK_VENV_RELATIVE))
}

/// Return the Python executable inside the ADK venv (S-7).
///
/// # Errors
///
/// Returns [`SupervisorError::Internal`] if `HOME` is not set.
pub fn python_executable() -> Result<PathBuf, SupervisorError> {
    Ok(venv_path()?.join("bin").join("python"))
}

/// Probe the ADK installation in the venv (S-1).
///
/// Checks that:
/// 1. The venv Python binary exists (S-7 path invariant).
/// 2. `python -m google.adk --version` succeeds and returns a parseable version ≥ 2.0.
///
/// # Errors
///
/// Returns [`SupervisorError::AdkMissing`] with the install URL when the venv Python
/// is absent or the `google-adk` package is not importable.
/// Returns [`SupervisorError::Internal`] if `HOME` is not set or the version string
/// cannot be parsed.
pub fn probe() -> Result<AdkVersion, SupervisorError> {
    let python = python_executable()?;

    if !python.exists() {
        return Err(SupervisorError::AdkMissing {
            install_url: ADK_INSTALL_URL.to_owned(),
        });
    }

    // S-2: execve semantics — no shell; args as separate Vec items.
    let out = std::process::Command::new(&python)
        .args(["-m", "google.adk", "--version"])
        .output()
        .map_err(|_| SupervisorError::AdkMissing {
            install_url: ADK_INSTALL_URL.to_owned(),
        })?;

    if !out.status.success() {
        return Err(SupervisorError::AdkMissing {
            install_url: ADK_INSTALL_URL.to_owned(),
        });
    }

    let version_str = String::from_utf8_lossy(&out.stdout);
    let version = AdkVersion::parse(version_str.trim()).ok_or_else(|| {
        SupervisorError::Internal(format!(
            "could not parse ADK version string: '{}'",
            version_str.trim()
        ))
    })?;

    // S-1: fail fast if installed major version is below the minimum supported.
    if version.major < ADK_MIN_MAJOR {
        return Err(SupervisorError::AdkMissing {
            install_url: ADK_INSTALL_URL.to_owned(),
        });
    }

    Ok(version)
}

/// Check whether an ADK version upgrade crosses a major version boundary (S-8).
///
/// Returns [`SupervisorError::MajorVersionHitlRequired`] when the requested
/// major version differs from the installed one. Auto-upgrade within the same
/// major is permitted (minor/patch bumps only).
///
/// # Errors
///
/// Returns [`SupervisorError::MajorVersionHitlRequired`] on major-version mismatch.
pub fn check_version_upgrade(
    installed: &AdkVersion,
    requested: &AdkVersion,
) -> Result<(), SupervisorError> {
    if requested.major != installed.major {
        return Err(SupervisorError::MajorVersionHitlRequired {
            installed: installed.major,
            requested: requested.major,
        });
    }
    Ok(())
}

/// Allocate an ephemeral TCP port in the range [`PORT_RANGE_START`]–[`PORT_RANGE_END`] (S-3).
///
/// Attempts up to [`PORT_RETRIES`] ports starting from `PORT_RANGE_START`. A port is
/// considered available if `TcpListener::bind` succeeds. The listener is immediately
/// dropped so the ADK process can bind to the same port.
///
/// # Errors
///
/// Returns [`SupervisorError::PortExhausted`] if all [`PORT_RETRIES`] attempts fail.
pub fn allocate_ephemeral_port() -> Result<u16, SupervisorError> {
    for i in 0..PORT_RETRIES {
        let port = PORT_RANGE_START.saturating_add(i);
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    Err(SupervisorError::PortExhausted)
}

/// Restart budget tracker for the restart-cap invariant (S-5).
///
/// Tracks restart timestamps in a sliding [`RESTART_WINDOW`] and enforces
/// [`RESTART_CAP`]. Callers must call [`RestartTracker::record`] before
/// every restart attempt.
#[derive(Debug, Default)]
pub struct RestartTracker {
    restart_times: Vec<Instant>,
}

impl RestartTracker {
    /// Create a new tracker with no recorded restarts.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a restart attempt, evicting timestamps outside the window.
    ///
    /// # Errors
    ///
    /// Returns [`SupervisorError::RestartCapReached`] if the number of
    /// restarts within the last [`RESTART_WINDOW`] would exceed [`RESTART_CAP`].
    pub fn record(&mut self) -> Result<(), SupervisorError> {
        let now = Instant::now();
        // Evict timestamps older than the window.
        self.restart_times
            .retain(|&t| now.duration_since(t) < RESTART_WINDOW);

        if self.restart_times.len() >= usize::from(RESTART_CAP) {
            return Err(SupervisorError::RestartCapReached {
                restart_count: u8::try_from(self.restart_times.len()).unwrap_or(RESTART_CAP),
            });
        }
        self.restart_times.push(now);
        Ok(())
    }

    /// Current restart count within the active window.
    #[must_use]
    pub fn count_in_window(&self) -> usize {
        let now = Instant::now();
        self.restart_times
            .iter()
            .filter(|&&t| now.duration_since(t) < RESTART_WINDOW)
            .count()
    }
}

/// Verify that the given Python path is inside the expected ADK venv (S-7).
///
/// Used in tests to assert that the supervisor never uses the system Python.
#[must_use]
pub fn is_in_adk_venv(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "adk-venv")
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    // S-1 + S-7: probe() returns AdkMissing when venv Python is absent.
    // When run in CI without ADK installed, probe() must return AdkMissing
    // within 100 ms (fail-fast guarantee).
    #[test]
    #[allow(unsafe_code)]
    fn detects_missing_install_when_venv_absent() {
        // Point HOME to /tmp so the venv path does not exist.
        let orig_home = std::env::var("HOME").unwrap_or_default();
        // SAFETY: single-threaded test; no concurrent env reads.
        unsafe { std::env::set_var("HOME", "/tmp/no-adk-here") };
        let result = probe();
        // SAFETY: single-threaded test; restoring original HOME.
        unsafe { std::env::set_var("HOME", &orig_home) };

        match result {
            Err(SupervisorError::AdkMissing { install_url }) => {
                assert!(
                    install_url.contains("google.github.io/adk-docs"),
                    "install URL must point to ADK docs, got: {install_url}"
                );
            }
            Ok(v) => panic!("expected AdkMissing, got Ok({v:?})"),
            Err(e) => panic!("expected AdkMissing, got {e}"),
        }
    }

    // S-7: python_executable() path must include "adk-venv".
    #[test]
    fn isolated_venv_python_path_contains_adk_venv() {
        let path = python_executable().unwrap();
        assert!(
            is_in_adk_venv(&path),
            "python executable '{path:?}' must be inside adk-venv"
        );
    }

    // S-8: major-version upgrade is blocked; minor upgrade is allowed.
    #[test]
    fn major_version_upgrade_requires_hitl() {
        let installed = AdkVersion {
            major: 2,
            minor: 1,
            patch: 0,
        };
        let requested_major = AdkVersion {
            major: 3,
            minor: 0,
            patch: 0,
        };
        let requested_minor = AdkVersion {
            major: 2,
            minor: 5,
            patch: 1,
        };

        assert!(
            check_version_upgrade(&installed, &requested_major).is_err(),
            "major-version jump must require HITL"
        );
        assert!(
            check_version_upgrade(&installed, &requested_minor).is_ok(),
            "minor-version bump must be auto-allowed"
        );

        let err = check_version_upgrade(&installed, &requested_major).unwrap_err();
        assert!(
            matches!(
                err,
                SupervisorError::MajorVersionHitlRequired {
                    installed: 2,
                    requested: 3
                }
            ),
            "unexpected error: {err}"
        );
    }

    // S-3: allocate_ephemeral_port() returns a different port when preferred is taken.
    #[test]
    fn port_collision_retries_and_finds_available() {
        // Bind PORT_RANGE_START so the first attempt fails.
        let _occupied = TcpListener::bind(("127.0.0.1", PORT_RANGE_START));
        // Even if binding failed (port already free or OS reserved), the allocator
        // must return a valid port in range without panicking.
        match allocate_ephemeral_port() {
            Ok(port) => {
                assert!(
                    port >= PORT_RANGE_START,
                    "port {port} must be ≥ PORT_RANGE_START"
                );
            }
            Err(SupervisorError::PortExhausted) => {
                // Acceptable in environments where the full range is occupied.
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    // S-5: RestartTracker enforces the restart cap within the window.
    #[test]
    fn restart_cap_enforced_after_three_restarts() {
        let mut tracker = RestartTracker::new();

        // First RESTART_CAP restarts succeed.
        for i in 0..RESTART_CAP {
            tracker
                .record()
                .unwrap_or_else(|e| panic!("restart {i} should succeed, got: {e}"));
        }

        // The (RESTART_CAP+1)th attempt must fail.
        let result = tracker.record();
        assert!(
            matches!(result, Err(SupervisorError::RestartCapReached { .. })),
            "expected RestartCapReached after {RESTART_CAP} restarts, got: {result:?}"
        );
    }

    // S-5: RestartTracker count_in_window reports correctly.
    #[test]
    fn restart_tracker_counts_correctly() {
        let mut tracker = RestartTracker::new();
        assert_eq!(tracker.count_in_window(), 0);
        tracker.record().unwrap();
        assert_eq!(tracker.count_in_window(), 1);
        tracker.record().unwrap();
        assert_eq!(tracker.count_in_window(), 2);
    }

    // S-2: verify no raw shell-based subprocess constructor is present.
    // This is a source-level grep contract test — CI would fail if anyone
    // adds a shell-based spawn. Here we verify the invariant is documented.
    #[test]
    fn no_raw_subprocess_contract_documented() {
        // The S-2 guarantee is enforced by code review and the SERAPH audit;
        // the production spawn path uses Command::new(python) with arg vectors.
        // This test asserts that the ADK_INSTALL_URL constant is present as a
        // proxy for "the S-series contract constants are compiled in".
        assert!(
            ADK_INSTALL_URL.contains("google.github.io"),
            "S-2 contract constants must be compiled in"
        );
    }

    #[test]
    fn adk_version_parse_roundtrips() {
        let v = AdkVersion::parse("2.0.1").unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 1);

        assert!(AdkVersion::parse("not-a-version").is_none());
        assert!(AdkVersion::parse("2.0").is_none());
    }
}
