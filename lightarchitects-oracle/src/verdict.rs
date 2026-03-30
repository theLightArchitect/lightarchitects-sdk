//! Oracle verdict — the synthesized result of multi-model analysis.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::models::{ModelId, ModelRole};

/// The consensus level across models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Consensus {
    /// All models agree on the conclusion.
    Unanimous,
    /// Majority of models agree, some disagree or have gaps.
    Majority,
    /// Models disagree — the disagreement IS the finding.
    Disagreement,
    /// Not enough models responded to determine consensus.
    Insufficient,
}

/// A single model's finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Which model produced this finding.
    pub model: ModelId,
    /// The model's analytical role.
    pub role: ModelRole,
    /// Human-readable model name.
    pub display: String,
    /// Whether the call succeeded.
    pub status: FindingStatus,
    /// The model's full response content.
    pub content: String,
    /// Time taken for this model's response.
    pub elapsed: Duration,
    /// Token usage (if reported by the API).
    #[serde(default)]
    pub tokens_in: u32,
    /// Token usage (if reported by the API).
    #[serde(default)]
    pub tokens_out: u32,
}

/// Status of a single model's analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    /// Model responded successfully.
    Ok,
    /// Model returned an error.
    Error(String),
    /// Model timed out.
    Timeout,
}

/// The complete oracle verdict — all findings plus consensus analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleVerdict {
    /// The prompt that was analyzed.
    pub prompt: String,
    /// Individual findings from each model, ordered by role priority.
    pub findings: Vec<Finding>,
    /// Consensus level across responding models.
    pub consensus: Consensus,
    /// Total time for the oracle query (wall clock, parallel execution).
    pub total_elapsed: Duration,
    /// How many models responded successfully.
    pub models_ok: usize,
    /// How many models were dispatched.
    pub models_total: usize,
}

impl OracleVerdict {
    /// Compute consensus from findings.
    ///
    /// Consensus is determined by whether the models agree on the core conclusion:
    /// - All formal proofs compile (or have `sorry` in the same places)
    /// - Derivations reach the same bound
    /// - Numerical checks find no counterexamples
    ///
    /// This is a heuristic — true consensus analysis requires the caller
    /// (Claude) to read the content and synthesize.
    pub(crate) fn compute_consensus(findings: &[Finding]) -> Consensus {
        let ok_count = findings
            .iter()
            .filter(|f| f.status == FindingStatus::Ok)
            .count();

        if ok_count == 0 {
            return Consensus::Insufficient;
        }
        if ok_count == 1 {
            return Consensus::Insufficient;
        }

        // Heuristic: check for disagreement signals in content.
        // A proper implementation would use NLI or structured output parsing.
        // For now, we flag potential disagreement if any model mentions
        // "false", "incorrect", "counterexample", or "disprove" while others don't.
        let mut has_negative = false;
        let mut has_positive = false;

        for finding in findings {
            if finding.status != FindingStatus::Ok {
                continue;
            }
            let lower = finding.content.to_lowercase();
            let is_negative = lower.contains("false")
                || lower.contains("incorrect")
                || lower.contains("counterexample")
                || lower.contains("disprove")
                || lower.contains("does not hold");
            let is_positive = lower.contains("proven")
                || lower.contains("verified")
                || lower.contains("qed")
                || lower.contains("holds for all")
                || lower.contains("therefore true");

            if is_negative {
                has_negative = true;
            }
            if is_positive {
                has_positive = true;
            }
        }

        if has_negative && has_positive {
            Consensus::Disagreement
        } else if ok_count == findings.len() {
            Consensus::Unanimous
        } else {
            Consensus::Majority
        }
    }
}

impl std::fmt::Display for OracleVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Oracle Verdict ({}/{} models responded, {:?})",
            self.models_ok, self.models_total, self.consensus
        )?;
        writeln!(f, "Total time: {:.1}s\n", self.total_elapsed.as_secs_f64())?;

        for finding in &self.findings {
            writeln!(f, "═══ {} ({}) ═══", finding.display, finding.model)?;
            writeln!(
                f,
                "Role: {:?} | Status: {:?} | Time: {:.1}s",
                finding.role,
                finding.status,
                finding.elapsed.as_secs_f64()
            )?;
            match &finding.status {
                FindingStatus::Ok => writeln!(f, "{}\n", finding.content)?,
                FindingStatus::Error(e) => writeln!(f, "ERROR: {e}\n")?,
                FindingStatus::Timeout => writeln!(f, "TIMED OUT\n")?,
            }
        }

        Ok(())
    }
}
