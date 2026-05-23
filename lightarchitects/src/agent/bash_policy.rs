//! Bash command-pattern policy for LLM-driven tool calls.
//!
//! Implements the B3 security fold: allowlist + denylist enforcement for
//! every `bash` tool invocation from the agentic loop.
//!
//! # Design
//!
//! - **Allowlist mode** (default for Restricted classification): only binaries
//!   explicitly listed in [`BashPolicy::ALLOWED_BINARIES`] may run.
//! - **Denylist patterns**: always blocked, even for listed binaries — covers
//!   destructive operations, command substitution, outbound network, and
//!   credential-access patterns.
//! - **Session promotion**: operator may add binaries at runtime via
//!   `BashPolicy::promote_binary()`.
//!
//! Maps to: Cookbook §63 (target-code-exec CRITICAL), OWASP-LLM02.

use std::collections::HashSet;

// ── Decision ────────────────────────────────────────────────────────────────

/// Outcome of a `BashPolicy` check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BashPolicyDecision {
    /// Command is permitted to execute.
    Allow,
    /// Command is blocked; `reason` is operator-visible.
    Deny {
        /// Human-readable explanation.
        reason: String,
    },
}

// ── Denylist entry ───────────────────────────────────────────────────────────

/// A static denylist entry: `(pattern_substring, human_readable_reason)`.
type DenyEntry = (&'static str, &'static str);

/// Patterns that are ALWAYS blocked, regardless of the binary allowlist.
///
/// Checked against the full command string (case-sensitive for path patterns;
/// lower-cased for keyword patterns).
static DENYLIST_PATTERNS: &[DenyEntry] = &[
    // Destructive filesystem operations on protected paths
    (
        "rm -rf ~/lightarchitects",
        "destructive rm on ~/lightarchitects",
    ),
    ("rm -rf ~/Projects", "destructive rm on ~/Projects"),
    ("rm -rf /", "destructive rm on root"),
    ("rm -rf ~", "destructive rm on home"),
    ("rm -rf ..", "destructive rm on parent"),
    ("rm -rf .", "destructive rm on cwd"),
    // Shell injection via piping to interpreters
    (
        "curl",
        "curl|sh pipe not allowed (use explicit download + verify)",
    ),
    ("wget|sh", "wget|sh pipe not allowed"),
    ("wget|bash", "wget|bash pipe not allowed"),
    // Command substitution and eval
    ("`", "backtick command substitution not allowed"),
    ("$(", "command substitution not allowed"),
    ("eval ", "eval not allowed"),
    ("eval\t", "eval not allowed"),
    // Outbound raw network tools
    ("nc -", "netcat outbound network not allowed"),
    ("ncat ", "ncat not allowed"),
    ("socat ", "socat not allowed"),
    // Credential access patterns (env var exfiltration)
    ("_KEY", "environment credential access blocked"),
    ("_TOKEN", "environment credential access blocked"),
    ("_SECRET", "environment credential access blocked"),
    ("_PASSWORD", "environment credential access blocked"),
    ("_PASS", "environment credential access blocked"),
];

// ── Policy ───────────────────────────────────────────────────────────────────

/// Bash command-pattern policy enforced on every LLM `bash` tool call.
///
/// Create with [`BashPolicy::default()`] for the standard Restricted-tier
/// allowlist, or [`BashPolicy::permissive()`] for development environments.
#[derive(Debug, Clone)]
pub struct BashPolicy {
    /// Session-promoted binaries added at runtime via [`BashPolicy::promote_binary`].
    promoted: HashSet<String>,
    /// When `true`, the binary allowlist is bypassed (development mode only).
    permissive: bool,
}

impl BashPolicy {
    /// Binaries allowed by default in Restricted classification.
    pub const ALLOWED_BINARIES: &'static [&'static str] = &[
        "cargo",
        "git",
        "ls",
        "cat",
        "head",
        "tail",
        "grep",
        "rg",
        "find",
        "jq",
        "make",
        "pnpm",
        "npm",
        "pwd",
        "echo",
        "which",
        "mkdir",
        "cd",
        "rustfmt",
        "clippy",
        "rustup",
        "cargo-make",
    ];

    /// Create a permissive policy that skips the binary allowlist.
    ///
    /// Denylist patterns are still enforced. Use only in controlled dev
    /// environments; never in production or Restricted classification.
    #[must_use]
    pub fn permissive() -> Self {
        Self {
            promoted: HashSet::new(),
            permissive: true,
        }
    }

    /// Promote a binary into the session allowlist at operator request.
    ///
    /// `binary` should be the base name (e.g. `"python3"`), not a full path.
    pub fn promote_binary(&mut self, binary: impl Into<String>) {
        self.promoted.insert(binary.into());
    }

    /// Check whether `command` is permitted to execute.
    ///
    /// 1. Denylist patterns are checked first (always blocking).
    /// 2. Binary allowlist is checked next (skipped in permissive mode).
    #[must_use]
    pub fn check(&self, command: &str) -> BashPolicyDecision {
        // Step 1: denylist (always enforced).
        // Use the original command for path-sensitive patterns; lowercase for keywords.
        let lower = command.to_lowercase();
        for (pattern, reason) in DENYLIST_PATTERNS {
            // Credential patterns match upper-case env var names.
            let haystack = if pattern.contains('_') {
                command
            } else {
                &lower
            };
            if haystack.contains(pattern) {
                return BashPolicyDecision::Deny {
                    reason: format!("denylist: {reason} (pattern: `{pattern}`)"),
                };
            }
        }

        // Step 2: binary allowlist (skip in permissive mode).
        if self.permissive {
            return BashPolicyDecision::Allow;
        }

        let binary = extract_binary(command);
        if Self::ALLOWED_BINARIES.contains(&binary.as_str()) || self.promoted.contains(&binary) {
            BashPolicyDecision::Allow
        } else {
            BashPolicyDecision::Deny {
                reason: format!(
                    "binary `{binary}` not in allowlist; promote with `lightarchitects bash allow {binary}`"
                ),
            }
        }
    }
}

impl Default for BashPolicy {
    /// Default policy: Restricted-tier allowlist, no promoted binaries.
    fn default() -> Self {
        Self {
            promoted: HashSet::new(),
            permissive: false,
        }
    }
}

/// Extract the leading binary name from a shell command string.
///
/// Handles simple leading paths (`/usr/bin/cargo` → `cargo`) and ignores
/// leading env-var assignments of the form `UPPER_KEY=value` — specifically,
/// tokens where the text before `=` is composed only of uppercase ASCII
/// letters, digits, and underscores (POSIX env var naming convention).
fn extract_binary(command: &str) -> String {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    // Skip leading `UPPER_KEY=value` env assignments.
    let binary_token = tokens
        .iter()
        .find(|t| {
            if let Some((key, _)) = t.split_once('=') {
                // Not an env-var assignment unless the key is POSIX-upper-case.
                !key.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            } else {
                true
            }
        })
        .copied()
        .unwrap_or("");
    // Strip any path prefix.
    binary_token
        .rsplit('/')
        .next()
        .unwrap_or(binary_token)
        .to_owned()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn policy() -> BashPolicy {
        BashPolicy::default()
    }

    #[test]
    fn allowed_binary_passes() {
        assert_eq!(policy().check("cargo check"), BashPolicyDecision::Allow);
        assert_eq!(policy().check("git status"), BashPolicyDecision::Allow);
        assert_eq!(policy().check("ls -la"), BashPolicyDecision::Allow);
    }

    #[test]
    fn unlisted_binary_denied() {
        let d = policy().check("python3 malicious.py");
        assert!(matches!(d, BashPolicyDecision::Deny { .. }));
    }

    #[test]
    fn denylist_destructive_rm_blocked() {
        let d = policy().check("rm -rf ~/lightarchitects/soul");
        assert!(matches!(d, BashPolicyDecision::Deny { .. }));
    }

    #[test]
    fn denylist_command_substitution_blocked() {
        let d = policy().check("echo $(cat ~/.ssh/id_rsa)");
        assert!(matches!(d, BashPolicyDecision::Deny { .. }));
    }

    #[test]
    fn denylist_backtick_blocked() {
        let d = policy().check("echo `id`");
        assert!(matches!(d, BashPolicyDecision::Deny { .. }));
    }

    #[test]
    fn denylist_credential_env_blocked() {
        let d = policy().check("echo $ANTHROPIC_API_KEY");
        assert!(matches!(d, BashPolicyDecision::Deny { .. }));
    }

    #[test]
    fn denylist_eval_blocked() {
        let d = policy().check("eval malicious_code");
        assert!(matches!(d, BashPolicyDecision::Deny { .. }));
    }

    #[test]
    fn promoted_binary_allowed() {
        let mut p = policy();
        p.promote_binary("python3");
        assert_eq!(p.check("python3 setup.py"), BashPolicyDecision::Allow);
    }

    #[test]
    fn permissive_allows_unlisted() {
        let p = BashPolicy::permissive();
        assert_eq!(p.check("ruby script.rb"), BashPolicyDecision::Allow);
    }

    #[test]
    fn permissive_still_blocks_denylist() {
        let p = BashPolicy::permissive();
        let d = p.check("rm -rf ~/Projects");
        assert!(matches!(d, BashPolicyDecision::Deny { .. }));
    }

    #[test]
    fn path_prefix_stripped_for_binary_check() {
        // /usr/bin/cargo should resolve to `cargo` and be allowed
        assert_eq!(
            policy().check("/usr/bin/cargo test"),
            BashPolicyDecision::Allow
        );
    }

    #[test]
    fn env_assignment_skipped_for_binary() {
        // POSIX env-var assignment (no space in value) before binary.
        assert_eq!(
            policy().check("RUSTFLAGS=--edition=2021 cargo clippy"),
            BashPolicyDecision::Allow
        );
    }
}
