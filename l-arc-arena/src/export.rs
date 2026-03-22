//! Training data export in SFT, DPO, and RL formats.
//!
//! Transforms scored execution traces into JSONL files that downstream
//! training frameworks (TRL, transformers, axolotl) can directly consume.
//! Includes evaluation report generation with aggregate scorecards.
#![allow(clippy::cast_precision_loss)] // Export counts are small; f64 precision is fine.

use std::io::{BufWriter, Write};
use std::path::Path;

use crate::engine::Trace;
use crate::scoring::RewardBreakdown;
use serde::Serialize;

/// Errors during export operations.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// I/O error writing files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// A scored trace ready for export.
pub struct ScoredTrace {
    /// The execution trace.
    pub trace: Trace,
    /// The 8-dimensional reward breakdown.
    pub reward: RewardBreakdown,
}

// ── SFT Export ──────────────────────────────────────────────────────────────

/// SFT example in ChatML/ShareGPT conversation format.
#[derive(Debug, Serialize)]
struct SftExample {
    conversations: Vec<SftMessage>,
}

/// A single message in an SFT conversation.
#[derive(Debug, Serialize)]
struct SftMessage {
    role: String,
    content: String,
}

/// Export scored traces as SFT training data (JSONL).
///
/// Only includes traces with total reward >= `threshold`. Each line is a
/// JSON object with `conversations` array in `ChatML` format.
///
/// # Errors
///
/// Returns [`ExportError`] if file I/O or serialization fails.
pub fn export_sft(
    traces: &[ScoredTrace],
    threshold: f64,
    output_path: &Path,
) -> Result<usize, ExportError> {
    let file = std::fs::File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut count = 0;

    for scored in traces {
        if scored.reward.total < threshold {
            continue;
        }

        let example = SftExample {
            conversations: vec![
                SftMessage {
                    role: "system".into(),
                    content: scored.trace.prompt.system.clone(),
                },
                SftMessage {
                    role: "user".into(),
                    content: scored.trace.prompt.user.clone(),
                },
                SftMessage {
                    role: "assistant".into(),
                    content: scored.trace.model_output.clone(),
                },
            ],
        };

        serde_json::to_writer(&mut writer, &example)?;
        writer.write_all(b"\n")?;
        count += 1;
    }

    writer.flush()?;
    Ok(count)
}

// ── DPO Export ──────────────────────────────────────────────────────────────

/// DPO example with chosen (high reward) and rejected (low reward) completions.
#[derive(Debug, Serialize)]
struct DpoExample {
    prompt: String,
    chosen: String,
    rejected: String,
    chosen_score: f64,
    rejected_score: f64,
}

/// Export scored traces as DPO training data (JSONL).
///
/// Pairs high-reward and low-reward traces for the same exercise. Traces
/// are sorted by score and paired: best with worst, second-best with
/// second-worst, etc.
///
/// # Errors
///
/// Returns [`ExportError`] if file I/O or serialization fails.
pub fn export_dpo(traces: &[ScoredTrace], output_path: &Path) -> Result<usize, ExportError> {
    // Group traces by exercise ID.
    let mut by_exercise: std::collections::HashMap<&str, Vec<&ScoredTrace>> =
        std::collections::HashMap::new();
    for scored in traces {
        by_exercise
            .entry(scored.trace.exercise_id.as_str())
            .or_default()
            .push(scored);
    }

    let file = std::fs::File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut count = 0;

    for (_exercise_id, mut group) in by_exercise {
        if group.len() < 2 {
            continue;
        }

        // Sort by reward descending.
        group.sort_by(|a, b| {
            b.reward
                .total
                .partial_cmp(&a.reward.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Pair best with worst.
        let pairs = group.len() / 2;
        for i in 0..pairs {
            let chosen = &group[i];
            let rejected = &group[group.len() - 1 - i];

            let example = DpoExample {
                prompt: format!(
                    "{}\n\n{}",
                    chosen.trace.prompt.system, chosen.trace.prompt.user
                ),
                chosen: chosen.trace.model_output.clone(),
                rejected: rejected.trace.model_output.clone(),
                chosen_score: chosen.reward.total,
                rejected_score: rejected.reward.total,
            };

            serde_json::to_writer(&mut writer, &example)?;
            writer.write_all(b"\n")?;
            count += 1;
        }
    }

    writer.flush()?;
    Ok(count)
}

// ── RL Export ────────────────────────────────────────────────────────────────

/// RL example with full trace and 8-dimensional rewards.
#[derive(Debug, Serialize)]
struct RlExample {
    prompt: String,
    completion: String,
    reward: RewardBreakdown,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<String>,
}

/// Export scored traces as RL training data (JSONL).
///
/// Includes all traces (no threshold filtering) with full 8-dimensional
/// reward breakdowns. Downstream GRPO/REINFORCE+ReBN uses the per-dimension
/// rewards for policy gradient computation.
///
/// # Errors
///
/// Returns [`ExportError`] if file I/O or serialization fails.
pub fn export_rl(traces: &[ScoredTrace], output_path: &Path) -> Result<usize, ExportError> {
    let file = std::fs::File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut count = 0;

    for scored in traces {
        let example = RlExample {
            prompt: format!(
                "{}\n\n{}",
                scored.trace.prompt.system, scored.trace.prompt.user
            ),
            completion: scored.trace.model_output.clone(),
            reward: scored.reward.clone(),
            reasoning: scored.trace.reasoning_content.clone(),
        };

        serde_json::to_writer(&mut writer, &example)?;
        writer.write_all(b"\n")?;
        count += 1;
    }

    writer.flush()?;
    Ok(count)
}

// ── Eval Report ─────────────────────────────────────────────────────────────

/// Aggregate evaluation report.
#[derive(Debug, Serialize)]
pub struct EvalReport {
    /// Total traces scored.
    pub total_traces: usize,
    /// Traces above SFT threshold.
    pub sft_eligible: usize,
    /// Average total reward.
    pub avg_total_reward: f64,
    /// Per-dimension average scores.
    pub dimension_averages: DimensionAverages,
    /// Pass rate (% of traces above 0.7 total).
    pub pass_rate: f64,
}

/// Average scores per reward dimension.
#[derive(Debug, Serialize)]
pub struct DimensionAverages {
    /// Average judgment score.
    pub judgment: f64,
    /// Average parameter accuracy score.
    pub parameter_accuracy: f64,
    /// Average timing score.
    pub timing: f64,
    /// Average result usage score.
    pub result_usage: f64,
    /// Average safety score.
    pub safety: f64,
    /// Average efficiency score.
    pub efficiency: f64,
    /// Average escalation score.
    pub escalation: f64,
    /// Average hallucination score.
    pub hallucination: f64,
}

/// Generate an evaluation report from scored traces.
#[must_use]
pub fn generate_report(traces: &[ScoredTrace], sft_threshold: f64) -> EvalReport {
    let n = traces.len();
    if n == 0 {
        return EvalReport {
            total_traces: 0,
            sft_eligible: 0,
            avg_total_reward: 0.0,
            dimension_averages: DimensionAverages {
                judgment: 0.0,
                parameter_accuracy: 0.0,
                timing: 0.0,
                result_usage: 0.0,
                safety: 0.0,
                efficiency: 0.0,
                escalation: 0.0,
                hallucination: 0.0,
            },
            pass_rate: 0.0,
        };
    }

    let mut sum_total = 0.0;
    let mut sum_j = 0.0;
    let mut sum_p = 0.0;
    let mut sum_t = 0.0;
    let mut sum_r = 0.0;
    let mut sum_s = 0.0;
    let mut sum_e = 0.0;
    let mut sum_esc = 0.0;
    let mut sum_h = 0.0;
    let mut sft_eligible = 0usize;
    let mut pass_count = 0usize;

    for scored in traces {
        let r = &scored.reward;
        sum_total += r.total;
        sum_j += r.judgment;
        sum_p += r.parameter_accuracy;
        sum_t += r.timing;
        sum_r += r.result_usage;
        sum_s += r.safety;
        sum_e += r.efficiency;
        sum_esc += r.escalation;
        sum_h += r.hallucination;

        if r.total >= sft_threshold {
            sft_eligible += 1;
        }
        if r.total >= 0.7 {
            pass_count += 1;
        }
    }

    let nf = n as f64;

    EvalReport {
        total_traces: n,
        sft_eligible,
        avg_total_reward: sum_total / nf,
        dimension_averages: DimensionAverages {
            judgment: sum_j / nf,
            parameter_accuracy: sum_p / nf,
            timing: sum_t / nf,
            result_usage: sum_r / nf,
            safety: sum_s / nf,
            efficiency: sum_e / nf,
            escalation: sum_esc / nf,
            hallucination: sum_h / nf,
        },
        pass_rate: pass_count as f64 / nf,
    }
}

/// Write an eval report as JSON.
///
/// # Errors
///
/// Returns [`ExportError`] if file I/O or serialization fails.
pub fn write_report(report: &EvalReport, output_path: &Path) -> Result<(), ExportError> {
    let json = serde_json::to_string_pretty(report)?;
    std::fs::write(output_path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OutputFormat;
    use crate::engine::Trace;
    use crate::prompts::AssembledPrompt;
    use crate::scoring::RewardBreakdown;

    fn make_scored_trace(exercise_id: &str, total: f64) -> ScoredTrace {
        ScoredTrace {
            trace: Trace {
                exercise_id: exercise_id.into(),
                prompt: AssembledPrompt {
                    system: "sys".into(),
                    user: "usr".into(),
                    objective: OutputFormat::Sft,
                },
                model_output: "I'll use the tool".into(),
                reasoning_content: None,
                tool_calls: vec![],
                duration: std::time::Duration::from_millis(100),
                success: true,
                error: None,
            },
            reward: RewardBreakdown {
                judgment: total,
                parameter_accuracy: total,
                timing: total,
                result_usage: total,
                safety: 1.0,
                efficiency: total,
                escalation: 0.7,
                hallucination: 1.0,
                total,
                details: std::collections::HashMap::new(),
            },
        }
    }

    #[test]
    fn sft_export_filters_by_threshold() {
        let traces = vec![
            make_scored_trace("a", 0.9),
            make_scored_trace("b", 0.3),
            make_scored_trace("c", 0.8),
        ];

        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("sft.jsonl");

        let count = export_sft(&traces, 0.7, &path).expect("export");
        assert_eq!(count, 2); // only a and c above 0.7
    }

    #[test]
    fn rl_export_includes_all() {
        let traces = vec![make_scored_trace("a", 0.9), make_scored_trace("b", 0.1)];

        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("rl.jsonl");

        let count = export_rl(&traces, &path).expect("export");
        assert_eq!(count, 2); // all included
    }

    #[test]
    fn report_averages() {
        let traces = vec![make_scored_trace("a", 0.8), make_scored_trace("b", 0.6)];
        let report = generate_report(&traces, 0.7);
        assert_eq!(report.total_traces, 2);
        assert_eq!(report.sft_eligible, 1); // only 0.8 >= 0.7
        assert!((report.avg_total_reward - 0.7).abs() < 0.01);
        assert!((report.pass_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn empty_report() {
        let report = generate_report(&[], 0.7);
        assert_eq!(report.total_traces, 0);
        assert!((report.avg_total_reward - 0.0).abs() < f64::EPSILON);
    }
}
