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

// ── GRPO Export ─────────────────────────────────────────────────────────────

/// A single message in the GRPO prompt list.
///
/// Unsloth `GRPOTrainer` expects `prompt` as a list of `{role, content}` dicts
/// so the model's chat template can be applied before generation begins.
#[derive(Debug, Serialize)]
struct GrpoMessage {
    role: String,
    content: String,
}

/// GRPO training example in Unsloth-compatible format.
///
/// `prompt` is a `ChatML` message list (system + user).
/// `answer`  is the reference completion — the model output from this trace.
///           High-reward traces serve as strong positive references; low-reward
///           traces are included so GRPO can learn the reward gradient.
/// `reward`  is the scalar total reward for this completion.
/// `reward_components` exposes all 8 Arena dimensions so callers can build
///           multi-signal reward functions (e.g. separate safety vs quality heads).
#[derive(Debug, Serialize)]
struct GrpoExample {
    /// Conversation context: `[{role:"system",...}, {role:"user",...}]`.
    prompt: Vec<GrpoMessage>,
    /// The model's actual completion for this trace (tool call + response text).
    answer: String,
    /// Scalar total reward in [0.0, 1.0].
    reward: f64,
    /// Per-dimension reward breakdown for multi-signal GRPO heads.
    reward_components: GrpoRewardComponents,
    /// Exercise ID — useful for grouping completions during online GRPO sampling.
    #[serde(skip_serializing_if = "str::is_empty")]
    exercise_id: String,
}

/// The 8 Arena reward dimensions, serialised as flat fields for readability.
#[derive(Debug, Serialize)]
struct GrpoRewardComponents {
    judgment: f64,
    parameter_accuracy: f64,
    timing: f64,
    result_usage: f64,
    safety: f64,
    efficiency: f64,
    escalation: f64,
    hallucination: f64,
}

/// Export scored traces as GRPO training data (JSONL).
///
/// Emits every trace (no threshold — GRPO needs the full reward gradient,
/// including low-scoring examples). Each line is a JSON object with:
///
/// - `prompt`: `[{role, content}]` message list (Unsloth chat-template aware)
/// - `answer`: raw model output for this trace
/// - `reward`: scalar total reward
/// - `reward_components`: 8-dimensional Arena breakdown
/// - `exercise_id`: for grouping in online GRPO sampling loops
///
/// Downstream usage with Unsloth:
///
/// ```python
/// from trl import GRPOConfig, GRPOTrainer
/// trainer = GRPOTrainer(
///     model=model,
///     args=GRPOConfig(
///         loss_type="dr_grpo",
///         importance_sampling_level="sequence",  # GSPO
///         num_generations=8,
///         mask_truncated_completions=True,
///     ),
///     train_dataset=load_dataset("json", data_files="arena_grpo.jsonl")["train"],
///     reward_funcs=[arena_reward_fn],  # scores live completions; `answer` is reference
/// )
/// ```
///
/// # Errors
///
/// Returns [`ExportError`] if file I/O or serialization fails.
pub fn export_grpo_jsonl(traces: &[ScoredTrace], output_path: &Path) -> Result<usize, ExportError> {
    let file = std::fs::File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut count = 0;

    for scored in traces {
        let r = &scored.reward;
        let t = &scored.trace;

        let example = GrpoExample {
            prompt: vec![
                GrpoMessage {
                    role: "system".into(),
                    content: t.prompt.system.clone(),
                },
                GrpoMessage {
                    role: "user".into(),
                    content: t.prompt.user.clone(),
                },
            ],
            answer: t.model_output.clone(),
            reward: r.total,
            reward_components: GrpoRewardComponents {
                judgment: r.judgment,
                parameter_accuracy: r.parameter_accuracy,
                timing: r.timing,
                result_usage: r.result_usage,
                safety: r.safety,
                efficiency: r.efficiency,
                escalation: r.escalation,
                hallucination: r.hallucination,
            },
            exercise_id: t.exercise_id.clone(),
        };

        serde_json::to_writer(&mut writer, &example)?;
        writer.write_all(b"\n")?;
        count += 1;
    }

    writer.flush()?;
    Ok(count)
}

/// Export scored traces grouped by exercise as a best-of-N GRPO dataset (JSONL).
///
/// Groups traces by `exercise_id` and emits up to `best_n` highest-scoring
/// completions per exercise alongside up to `worst_n` lowest-scoring ones.
/// This gives GRPO a balanced view of the reward landscape for each task without
/// including every trace from large runs.
///
/// When `best_n` or `worst_n` is 0, that tier is omitted entirely.
///
/// # Errors
///
/// Returns [`ExportError`] if file I/O or serialization fails.
pub fn export_grpo_best_of_n(
    traces: &[ScoredTrace],
    best_n: usize,
    worst_n: usize,
    output_path: &Path,
) -> Result<usize, ExportError> {
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

    let mut exercises: Vec<&str> = by_exercise.keys().copied().collect();
    exercises.sort_unstable(); // deterministic output order

    for exercise_id in exercises {
        let Some(group) = by_exercise.get(exercise_id) else {
            continue; // key came from the same map; unreachable in practice
        };

        // Sort descending by reward.
        let mut sorted: Vec<&&ScoredTrace> = group.iter().collect();
        sorted.sort_by(|a, b| {
            b.reward
                .total
                .partial_cmp(&a.reward.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Select best_n from the top, worst_n from the bottom (no overlap).
        let take_best = best_n.min(sorted.len());
        let take_worst = worst_n.min(sorted.len().saturating_sub(take_best));

        let selected = sorted[..take_best]
            .iter()
            .chain(sorted[sorted.len() - take_worst..].iter());

        for scored in selected {
            let r = &scored.reward;
            let t = &scored.trace;

            let example = GrpoExample {
                prompt: vec![
                    GrpoMessage {
                        role: "system".into(),
                        content: t.prompt.system.clone(),
                    },
                    GrpoMessage {
                        role: "user".into(),
                        content: t.prompt.user.clone(),
                    },
                ],
                answer: t.model_output.clone(),
                reward: r.total,
                reward_components: GrpoRewardComponents {
                    judgment: r.judgment,
                    parameter_accuracy: r.parameter_accuracy,
                    timing: r.timing,
                    result_usage: r.result_usage,
                    safety: r.safety,
                    efficiency: r.efficiency,
                    escalation: r.escalation,
                    hallucination: r.hallucination,
                },
                exercise_id: t.exercise_id.clone(),
            };

            serde_json::to_writer(&mut writer, &example)?;
            writer.write_all(b"\n")?;
            count += 1;
        }
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

    #[test]
    fn grpo_export_includes_all_traces() {
        let traces = vec![
            make_scored_trace("a", 0.9),
            make_scored_trace("b", 0.1), // low reward — still included for GRPO gradient
        ];

        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("grpo.jsonl");

        let count = export_grpo_jsonl(&traces, &path).expect("grpo export");
        assert_eq!(count, 2);
    }

    #[test]
    fn grpo_export_emits_message_list_format() {
        let traces = vec![make_scored_trace("ex1", 0.85)];

        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("grpo.jsonl");
        export_grpo_jsonl(&traces, &path).expect("grpo export");

        let raw = std::fs::read_to_string(&path).expect("read");
        let parsed: serde_json::Value = serde_json::from_str(raw.trim()).expect("parse");

        // `prompt` must be a list, not a string.
        assert!(
            parsed["prompt"].is_array(),
            "prompt should be a message list"
        );
        assert_eq!(parsed["prompt"][0]["role"], "system");
        assert_eq!(parsed["prompt"][1]["role"], "user");

        // `answer` must be the model output string.
        assert!(parsed["answer"].is_string());

        // `reward` must be a number in [0, 1].
        let reward = parsed["reward"].as_f64().expect("reward is f64");
        assert!((0.0..=1.0).contains(&reward));

        // `reward_components` must expose all 8 dimensions.
        for dim in &[
            "judgment",
            "parameter_accuracy",
            "timing",
            "result_usage",
            "safety",
            "efficiency",
            "escalation",
            "hallucination",
        ] {
            assert!(
                parsed["reward_components"][dim].is_number(),
                "missing reward component: {dim}"
            );
        }
    }

    #[test]
    fn grpo_best_of_n_selects_top_and_bottom() {
        // 4 traces for exercise "ex", reward 0.9, 0.7, 0.5, 0.2
        let traces = vec![
            make_scored_trace("ex", 0.9),
            make_scored_trace("ex", 0.7),
            make_scored_trace("ex", 0.5),
            make_scored_trace("ex", 0.2),
        ];

        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("bon.jsonl");

        // best_n=1, worst_n=1 → should emit exactly 2 rows
        let count = export_grpo_best_of_n(&traces, 1, 1, &path).expect("bon export");
        assert_eq!(count, 2);
    }

    #[test]
    fn grpo_best_of_n_no_overlap() {
        // 3 traces — best_n=2, worst_n=2 but only 3 available; no overlap allowed
        let traces = vec![
            make_scored_trace("ex", 0.9),
            make_scored_trace("ex", 0.5),
            make_scored_trace("ex", 0.1),
        ];

        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("bon_nooverlap.jsonl");

        // best_n=2 takes indices [0,1]; worst_n=2 can only take index [2] without overlap
        let count = export_grpo_best_of_n(&traces, 2, 2, &path).expect("bon export");
        assert_eq!(count, 3); // 2 best + 1 remaining worst (no overlap)
    }
}
