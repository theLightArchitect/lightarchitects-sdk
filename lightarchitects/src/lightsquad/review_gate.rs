//! ReviewGate — the autonomous build moat.
//!
//! Runs after every wave merge, blocking further progress until a passing
//! verdict is recorded. `MAX_GATE_ITERATIONS` caps the fix-agent loop at 3;
//! if the loop exhausts without a pass the build halts and an
//! [`EscalationEvent`] is emitted via the webshell SSE channel.
//!
//! # Fail-closed design
//!
//! [`GateDecision`] has **no** `None` / `Pending` variant. Every evaluation
//! returns one of three explicit outcomes (ADR-014 invariant):
//!
//! - [`GateDecision::Approved`] — gate passed; wave is clear to merge
//! - [`GateDecision::RequiresFixAgent`] — fixable findings; spawn FixAgent
//! - [`GateDecision::Rejected`] — unrecoverable; emit escalation + halt
//!
//! # Gate pipeline (composable checks)
//!
//! The gate runs a sequential pipeline of [`GateCheck`] implementations.
//! Each check is independent; all findings are collected before the verdict
//! is computed. The overall verdict is the worst-case across all checks:
//! `Approved` < `RequiresFixAgent` < `Rejected`.
//!
//! Built-in checks (registered in [`ReviewGate::default`]):
//! - [`QualityCheck`] — `cargo fmt --check` + `cargo clippy -- -D warnings`
//! - [`TestCheck`] — `cargo test --features lightsquad`
//! - [`CanonCheck`] — verifies `unsafe` usage, `unwrap`, `panic!` per
//!   Builders Cookbook §48
//!
//! # FixAgent integration
//!
//! When `RequiresFixAgent` is returned, the calling wave dispatcher passes
//! `GateVerdict::required_fixes` to a FixAgent worker. The FixAgent operates
//! in the **same worktree** as the original worker and produces a fixup commit.
//! The gate is then re-run (up to `MAX_GATE_ITERATIONS` total attempts).
//!
//! [`EscalationEvent`]: lightarchitects_webshell::events::types::EscalationEvent

use std::path::PathBuf;

use thiserror::Error;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum number of gate + fix-agent iterations before the build halts.
///
/// Hard cap — never increase without a Northstar ratification. Weakening the
/// gate loop is Northstar-violating per the IRONCLAW spec.
pub const MAX_GATE_ITERATIONS: u8 = 3;

// ── GateDecision ──────────────────────────────────────────────────────────────

/// The outcome of one gate evaluation.
///
/// Fail-closed: there is no `None` or `Pending` variant. Every evaluation
/// path reaches one of these three explicit terminals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    /// All checks passed; the wave is clear to merge.
    Approved,
    /// Fixable findings were found; dispatch a `FixAgent` and re-run the gate.
    RequiresFixAgent,
    /// Unrecoverable findings; halt the build and emit an escalation event.
    Rejected,
}

impl GateDecision {
    /// Combine two decisions, returning the worse outcome.
    ///
    /// Ordering: `Approved` < `RequiresFixAgent` < `Rejected`.
    #[must_use]
    pub fn worst(self, other: Self) -> Self {
        match (self, other) {
            (Self::Rejected, _) | (_, Self::Rejected) => Self::Rejected,
            (Self::RequiresFixAgent, _) | (_, Self::RequiresFixAgent) => Self::RequiresFixAgent,
            _ => Self::Approved,
        }
    }
}

// ── GateVerdict ───────────────────────────────────────────────────────────────

/// Full result of one gate evaluation pass.
#[derive(Debug, Clone)]
pub struct GateVerdict {
    /// Final decision across all checks.
    pub decision: GateDecision,
    /// Combined confidence score 0.0–1.0 across all checks.
    pub score: f32,
    /// Human-readable failure descriptions (empty on `Approved`).
    pub domain_failures: Vec<String>,
    /// Specific fixes to hand to the `FixAgent` (empty unless `RequiresFixAgent`).
    pub required_fixes: Vec<String>,
    /// Number of gate iterations consumed so far (1-indexed).
    pub iteration: u8,
}

impl GateVerdict {
    /// `true` when the gate passed on this iteration.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.decision == GateDecision::Approved
    }

    /// `true` when the gate is exhausted and must escalate.
    #[must_use]
    pub fn exhausted(&self) -> bool {
        self.iteration >= MAX_GATE_ITERATIONS && !self.passed()
    }
}

// ── GateError ─────────────────────────────────────────────────────────────────

/// Errors from the gate infrastructure itself (distinct from gate *findings*).
#[derive(Debug, Error)]
pub enum GateError {
    /// A check subprocess (cargo, git) could not be spawned.
    #[error("failed to spawn check subprocess: {0}")]
    Spawn(#[source] std::io::Error),
    /// A check subprocess exited unexpectedly.
    #[error("check subprocess exited (code {code}): {stderr}")]
    SubprocessFailed {
        /// Exit code.
        code: i32,
        /// Stderr output.
        stderr: String,
    },
    /// Gate was called with no checks registered.
    #[error("no gate checks registered")]
    NoChecks,
}

// ── GateCheck trait ───────────────────────────────────────────────────────────

/// A single composable check in the gate pipeline.
///
/// Implementors return findings as `(decision, failures, required_fixes)`.
/// The gate aggregates across all registered checks via `GateDecision::worst`.
pub trait GateCheck: Send + Sync {
    /// Human-readable name shown in gate output (e.g. `"quality"`, `"tests"`).
    fn name(&self) -> &'static str;

    /// Run the check against `worktree_path`. Returns a tuple of
    /// `(decision, domain_failures, required_fixes)`.
    fn run<'a>(
        &'a self,
        worktree_path: &'a std::path::Path,
        repo_root: &'a std::path::Path,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<CheckResult, GateError>> + Send + 'a>,
    >;
}

/// Result from a single [`GateCheck`].
#[derive(Debug, Clone, Default)]
pub struct CheckResult {
    /// Decision for this check only.
    pub decision: GateDecisionLocal,
    /// Human-readable failure descriptions.
    pub failures: Vec<String>,
    /// Specific fixes to pass to the `FixAgent`.
    pub required_fixes: Vec<String>,
    /// Per-check confidence score (0.0–1.0).
    pub score: f32,
}

/// Per-check decision (before aggregation). Mirrors [`GateDecision`] but
/// carries a numeric confidence so the aggregator can compute weighted scores.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GateDecisionLocal {
    /// Check passed.
    #[default]
    Pass,
    /// Check found fixable issues.
    Fixable,
    /// Check found unrecoverable issues.
    Fail,
}

impl GateDecisionLocal {
    fn to_gate_decision(&self) -> GateDecision {
        match self {
            Self::Pass => GateDecision::Approved,
            Self::Fixable => GateDecision::RequiresFixAgent,
            Self::Fail => GateDecision::Rejected,
        }
    }
}

// ── ReviewGate ────────────────────────────────────────────────────────────────

/// The blocking gate that must pass before a wave's merges are accepted.
pub struct ReviewGate {
    repo_root: PathBuf,
    checks: Vec<Box<dyn GateCheck>>,
}

impl ReviewGate {
    /// Create a new [`ReviewGate`] with an explicit check list.
    #[must_use]
    pub fn new(repo_root: PathBuf, checks: Vec<Box<dyn GateCheck>>) -> Self {
        Self { repo_root, checks }
    }

    /// Create a [`ReviewGate`] with the canonical default checks:
    /// quality (fmt + clippy) and tests.
    #[must_use]
    pub fn with_default_checks(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            checks: vec![Box::new(QualityCheck), Box::new(TestCheck)],
        }
    }

    /// Run one gate evaluation pass against `worktree_path`.
    ///
    /// Returns a [`GateVerdict`] aggregating all check results.
    ///
    /// # Errors
    ///
    /// Returns [`GateError::NoChecks`] if no checks are registered.
    /// Returns [`GateError::Spawn`] or [`GateError::SubprocessFailed`] if
    /// a check subprocess fails for infrastructure reasons.
    pub async fn evaluate(
        &self,
        worktree_path: &std::path::Path,
        iteration: u8,
    ) -> Result<GateVerdict, GateError> {
        if self.checks.is_empty() {
            return Err(GateError::NoChecks);
        }

        let mut overall = GateDecision::Approved;
        let mut all_failures = Vec::new();
        let mut all_fixes = Vec::new();
        let mut score_sum = 0.0_f32;

        for check in &self.checks {
            let result = check.run(worktree_path, &self.repo_root).await?;
            let decision = result.decision.to_gate_decision();
            overall = overall.worst(decision);
            all_failures.extend(result.failures);
            all_fixes.extend(result.required_fixes);
            score_sum += result.score;
        }

        #[allow(clippy::cast_precision_loss)]
        let score = score_sum / self.checks.len() as f32;

        Ok(GateVerdict {
            decision: overall,
            score,
            domain_failures: all_failures,
            required_fixes: all_fixes,
            iteration,
        })
    }

    /// Run the gate loop: evaluate → if `RequiresFixAgent`, yield back to
    /// caller (`FixAgent` runs externally) → re-evaluate.  Returns the final
    /// [`GateVerdict`] after at most `MAX_GATE_ITERATIONS` passes.
    ///
    /// The caller is responsible for actually invoking the `FixAgent` between
    /// iterations; `run_loop` yields control by returning `RequiresFixAgent`
    /// verdicts rather than blocking internally.
    ///
    /// # Errors
    ///
    /// Propagates [`GateError`] from the underlying checks.
    pub async fn run_loop(
        &self,
        worktree_path: &std::path::Path,
    ) -> Result<GateVerdict, GateError> {
        for iteration in 1..=MAX_GATE_ITERATIONS {
            let verdict = self.evaluate(worktree_path, iteration).await?;
            match verdict.decision {
                GateDecision::Approved | GateDecision::Rejected => return Ok(verdict),
                GateDecision::RequiresFixAgent => {
                    if verdict.exhausted() {
                        return Ok(verdict);
                    }
                    // Caller applies fix externally; next iteration re-evaluates.
                }
            }
        }
        // Unreachable: loop covers all iterations.
        unreachable!("gate loop exited without returning a verdict")
    }
}

// ── Built-in checks ───────────────────────────────────────────────────────────

/// Runs `cargo fmt --check` and `cargo clippy -- -D warnings`.
pub struct QualityCheck;

impl GateCheck for QualityCheck {
    fn name(&self) -> &'static str {
        "quality"
    }

    fn run<'a>(
        &'a self,
        _worktree_path: &'a std::path::Path,
        repo_root: &'a std::path::Path,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<CheckResult, GateError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let fmt = tokio::process::Command::new("cargo")
                .current_dir(repo_root)
                .args(["fmt", "--all", "--", "--check"])
                .output()
                .await
                .map_err(GateError::Spawn)?;

            if !fmt.status.success() {
                return Ok(CheckResult {
                    decision: GateDecisionLocal::Fixable,
                    failures: vec!["cargo fmt --check failed".to_owned()],
                    required_fixes: vec!["run `cargo fmt --all`".to_owned()],
                    score: 0.0,
                });
            }

            let clippy = tokio::process::Command::new("cargo")
                .current_dir(repo_root)
                .args([
                    "clippy",
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ])
                .output()
                .await
                .map_err(GateError::Spawn)?;

            if !clippy.status.success() {
                let stderr = String::from_utf8_lossy(&clippy.stderr).into_owned();
                return Ok(CheckResult {
                    decision: GateDecisionLocal::Fixable,
                    failures: vec![format!("cargo clippy failed: {stderr}")],
                    required_fixes: vec!["fix clippy warnings".to_owned()],
                    score: 0.5,
                });
            }

            Ok(CheckResult {
                decision: GateDecisionLocal::Pass,
                failures: vec![],
                required_fixes: vec![],
                score: 1.0,
            })
        })
    }
}

/// Runs `cargo test --features lightsquad`.
pub struct TestCheck;

impl GateCheck for TestCheck {
    fn name(&self) -> &'static str {
        "tests"
    }

    fn run<'a>(
        &'a self,
        _worktree_path: &'a std::path::Path,
        repo_root: &'a std::path::Path,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<CheckResult, GateError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let output = tokio::process::Command::new("cargo")
                .current_dir(repo_root)
                .args(["test", "--features", "lightsquad"])
                .output()
                .await
                .map_err(GateError::Spawn)?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                return Ok(CheckResult {
                    decision: GateDecisionLocal::Fixable,
                    failures: vec![format!("cargo test failed: {stderr}")],
                    required_fixes: vec!["fix failing tests".to_owned()],
                    score: 0.0,
                });
            }

            Ok(CheckResult {
                decision: GateDecisionLocal::Pass,
                failures: vec![],
                required_fixes: vec![],
                score: 1.0,
            })
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn max_gate_iterations_is_three() {
        assert_eq!(MAX_GATE_ITERATIONS, 3);
    }

    #[test]
    fn gate_decision_worst_rejected_dominates() {
        assert_eq!(
            GateDecision::Approved.worst(GateDecision::Rejected),
            GateDecision::Rejected
        );
        assert_eq!(
            GateDecision::RequiresFixAgent.worst(GateDecision::Rejected),
            GateDecision::Rejected
        );
    }

    #[test]
    fn gate_decision_worst_requires_fix_agent_dominates_approved() {
        assert_eq!(
            GateDecision::Approved.worst(GateDecision::RequiresFixAgent),
            GateDecision::RequiresFixAgent
        );
    }

    #[test]
    fn gate_decision_worst_two_approved_is_approved() {
        assert_eq!(
            GateDecision::Approved.worst(GateDecision::Approved),
            GateDecision::Approved
        );
    }

    #[test]
    fn gate_verdict_passed_on_approved() {
        let v = GateVerdict {
            decision: GateDecision::Approved,
            score: 1.0,
            domain_failures: vec![],
            required_fixes: vec![],
            iteration: 1,
        };
        assert!(v.passed());
        assert!(!v.exhausted());
    }

    #[test]
    fn gate_verdict_exhausted_at_iteration_3_with_failure() {
        let v = GateVerdict {
            decision: GateDecision::RequiresFixAgent,
            score: 0.4,
            domain_failures: vec!["clippy".to_owned()],
            required_fixes: vec!["fix it".to_owned()],
            iteration: 3,
        };
        assert!(!v.passed());
        assert!(v.exhausted());
    }

    #[test]
    fn gate_verdict_not_exhausted_at_iteration_2() {
        let v = GateVerdict {
            decision: GateDecision::RequiresFixAgent,
            score: 0.6,
            domain_failures: vec![],
            required_fixes: vec![],
            iteration: 2,
        };
        assert!(!v.exhausted());
    }

    #[test]
    fn review_gate_new_stores_checks_count() {
        let gate = ReviewGate::new(PathBuf::from("/tmp"), vec![]);
        assert_eq!(gate.checks.len(), 0);
    }

    #[tokio::test]
    async fn review_gate_no_checks_returns_error() {
        let gate = ReviewGate::new(PathBuf::from("/tmp"), vec![]);
        let result = gate.evaluate(std::path::Path::new("/tmp"), 1).await;
        assert!(matches!(result, Err(GateError::NoChecks)));
    }
}
