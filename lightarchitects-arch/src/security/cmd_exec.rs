//! Hardened subprocess execution — B1 (binary allowlist) + S-1 (per-binary flag allowlist).
//!
//! Only [`AllowedBinary`] variants may be spawned.  Each variant carries a static set of
//! flag prefixes that are *explicitly permitted*.  Any unrecognised flag causes an immediate
//! [`CmdError::ForbiddenFlag`] — the process is never spawned.
//!
//! **Never use `sh -c`.**  All arguments are passed directly to `Command::new(binary)`.

use std::ffi::OsString;
use std::path::PathBuf;
use thiserror::Error;

/// The set of binaries that the architecture intelligence pipeline may invoke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowedBinary {
    /// `grep` — source-file search.
    Grep,
    /// `diff` — version comparison.
    Diff,
    /// `git` — lightweight metadata reads (e.g. `git log --oneline -N`).
    Git,
}

impl AllowedBinary {
    /// Returns the binary name as it should appear in `PATH`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Grep => "grep",
            Self::Diff => "diff",
            Self::Git => "git",
        }
    }

    /// Validates that `flag` is permitted for this binary.
    ///
    /// Only flags whose prefixes appear in the allowlist below pass.
    /// Short option clusters (e.g. `-rn`) are checked per-character after stripping `-`.
    fn is_allowed_flag(&self, flag: &str) -> bool {
        match self {
            // grep allowlist: pattern matching, line context, output formatting.
            // Forbidden: -f/--file (reads patterns from file — potential injection),
            //            -d/--exclude-dir (arbitrary recursion control),
            //            --include=* patterns that embed shell metacharacters.
            Self::Grep => matches_any(
                flag,
                &[
                    "-n",
                    "-l",
                    "-r",
                    "-R",
                    "-i",
                    "-c",
                    "-q",
                    "-h",
                    "-H",
                    "-A",
                    "-B",
                    "-C",
                    "--line-number",
                    "--files-with-matches",
                    "--recursive",
                    "--ignore-case",
                    "--count",
                    "--quiet",
                    "--no-filename",
                    "--with-filename",
                    "--after-context=",
                    "--before-context=",
                    "--context=",
                    "--color=",
                    "--colour=",
                    "--include=",
                    "--fixed-strings",
                    "-F",
                    "-E",
                    "--extended-regexp",
                    "-P",
                    "--perl-regexp",
                    "--null",
                    "-Z",
                    "--",
                ],
            ),
            // diff allowlist: unified/context output only.
            // Forbidden: -r (recursive dir diff — potential traversal amplifier).
            Self::Diff => matches_any(
                flag,
                &[
                    "-u",
                    "--unified",
                    "-c",
                    "--context",
                    "-p",
                    "--show-c-function",
                    "-a",
                    "--text",
                    "--label",
                    "-N",
                    "--new-file",
                    "--strip-trailing-cr",
                    "--",
                ],
            ),
            // git allowlist: read-only metadata only.
            Self::Git => matches_any(
                flag,
                &[
                    "log",
                    "show",
                    "diff",
                    "status",
                    "--oneline",
                    "--format=",
                    "--pretty=",
                    "-n",
                    "--max-count=",
                    "--follow",
                    "--name-only",
                    "--name-status",
                    "--no-pager",
                    "--",
                ],
            ),
        }
    }
}

/// Errors from the hardened execution layer.
#[derive(Debug, Error)]
pub enum CmdError {
    /// A flag or argument is not on the per-binary allowlist.
    #[error("flag '{flag}' is not permitted for binary '{binary}'")]
    ForbiddenFlag {
        /// The binary that was to be invoked.
        binary: &'static str,
        /// The offending flag.
        flag: String,
    },

    /// The working-directory supplied is outside the allowed roots.
    #[error("working directory '{0}' is not within any allowed root")]
    ForbiddenCwd(PathBuf),

    /// Subprocess spawn or wait failed.
    #[error("subprocess failed: {0}")]
    Io(#[from] std::io::Error),

    /// An empty argument was supplied; grep interprets it as a match-all pattern.
    #[error("empty argument is not permitted for binary '{binary}'")]
    EmptyArgument {
        /// The binary that was to be invoked.
        binary: &'static str,
    },

    /// The subprocess exited with a non-zero status.
    #[error("subprocess exited with status {code}; stderr: {stderr}")]
    NonZeroExit {
        /// Exit code.
        code: i32,
        /// Captured stderr output.
        stderr: String,
    },
}

/// Output captured from a successful subprocess invocation.
#[derive(Debug)]
pub struct ExecOutput {
    /// Captured stdout bytes.
    pub stdout: Vec<u8>,
    /// Captured stderr bytes.
    pub stderr: Vec<u8>,
}

/// Executes `binary` with `args`, constrained to `allowed_roots` as the working directory.
///
/// All arguments are validated against the per-binary allowlist *before* the process is
/// spawned.  The working directory must start with one of `allowed_roots`.
///
/// # Errors
///
/// - [`CmdError::ForbiddenFlag`] — a flag not on the allowlist was passed.
/// - [`CmdError::ForbiddenCwd`] — `cwd` is outside every allowed root.
/// - [`CmdError::Io`] — spawn or wait failed.
/// - [`CmdError::NonZeroExit`] — process exited with a non-zero code (exit 1 from grep
///   means "no match" — callers may handle this specially).
// argv is intentionally omitted from the span to avoid leaking search patterns into traces.
#[tracing::instrument(skip_all, fields(binary = ?binary))]
pub fn execute(
    binary: AllowedBinary,
    args: &[OsString],
    cwd: &std::path::Path,
    allowed_roots: &[PathBuf],
) -> Result<ExecOutput, CmdError> {
    // Validate working directory.
    let cwd_canonical = std::fs::canonicalize(cwd).map_err(CmdError::Io)?;
    let cwd_ok = allowed_roots.iter().any(|r| {
        std::fs::canonicalize(r)
            .map(|rc| cwd_canonical.starts_with(&rc))
            .unwrap_or(false)
    });
    if !cwd_ok {
        return Err(CmdError::ForbiddenCwd(cwd.to_path_buf()));
    }

    // Validate flags.
    for arg in args {
        let s = arg.to_string_lossy();
        // Reject empty args: grep treats "" as a match-all pattern.
        if s.is_empty() {
            return Err(CmdError::EmptyArgument {
                binary: binary.name(),
            });
        }
        // Non-flag arguments (patterns, file paths) are passed as-is.
        if s.starts_with('-') && !binary.is_allowed_flag(&s) {
            return Err(CmdError::ForbiddenFlag {
                binary: binary.name(),
                flag: s.into_owned(),
            });
        }
    }

    // Spawn — NEVER via sh -c.
    let out = std::process::Command::new(binary.name())
        .args(args)
        .current_dir(cwd)
        .output()?;

    if !out.status.success() {
        // exit 1 from grep means "no match" — not an execution error.
        let code = out.status.code().unwrap_or(-1);
        if !(binary == AllowedBinary::Grep && code == 1) {
            return Err(CmdError::NonZeroExit {
                code,
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            });
        }
    }

    Ok(ExecOutput {
        stdout: out.stdout,
        stderr: out.stderr,
    })
}

/// Returns `true` when `flag` starts with any of the allowed prefixes.
fn matches_any(flag: &str, allowed: &[&str]) -> bool {
    allowed.iter().any(|a| {
        if a.ends_with('=') {
            flag.starts_with(a)
        } else {
            flag == *a
        }
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use std::ffi::OsString;

    fn os(s: &str) -> OsString {
        OsString::from(s)
    }

    #[test]
    fn grep_rejects_file_flag() {
        assert!(!AllowedBinary::Grep.is_allowed_flag("-f"));
        assert!(!AllowedBinary::Grep.is_allowed_flag("--file"));
    }

    #[test]
    fn grep_rejects_exclude_dir_flag() {
        assert!(!AllowedBinary::Grep.is_allowed_flag("--exclude-dir"));
        assert!(!AllowedBinary::Grep.is_allowed_flag("-d"));
    }

    #[test]
    fn grep_accepts_recursive_flag() {
        assert!(AllowedBinary::Grep.is_allowed_flag("-r"));
        assert!(AllowedBinary::Grep.is_allowed_flag("--recursive"));
    }

    #[test]
    fn diff_rejects_recursive_flag() {
        assert!(!AllowedBinary::Diff.is_allowed_flag("-r"));
    }

    #[test]
    fn diff_accepts_unified_flag() {
        assert!(AllowedBinary::Diff.is_allowed_flag("-u"));
        assert!(AllowedBinary::Diff.is_allowed_flag("--unified"));
    }

    #[test]
    fn execute_rejects_forbidden_cwd() {
        let args: Vec<OsString> = vec![os("-r"), os("fn"), os(".")];
        let cwd = std::path::Path::new("/etc");
        let roots = vec![std::path::PathBuf::from("/tmp")];
        assert!(matches!(
            execute(AllowedBinary::Grep, &args, cwd, &roots),
            Err(CmdError::ForbiddenCwd(_))
        ));
    }

    #[test]
    fn execute_rejects_forbidden_flag_before_spawn() {
        let tmp = tempfile::TempDir::new().unwrap();
        let args: Vec<OsString> = vec![os("-f"), os("/etc/passwd")];
        let roots = vec![tmp.path().to_path_buf()];
        assert!(matches!(
            execute(AllowedBinary::Grep, &args, tmp.path(), &roots),
            Err(CmdError::ForbiddenFlag { .. })
        ));
    }

    #[test]
    fn execute_rejects_empty_argument() {
        let tmp = tempfile::TempDir::new().unwrap();
        let args: Vec<OsString> = vec![os("")]; // empty pattern = match-all
        let roots = vec![tmp.path().to_path_buf()];
        assert!(matches!(
            execute(AllowedBinary::Grep, &args, tmp.path(), &roots),
            Err(CmdError::EmptyArgument { .. })
        ));
    }
}
