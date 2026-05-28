//! `LongMemEval` retrieval evaluation against SOUL helix.
//!
//! Loads 500 benchmark questions, queries SOUL for each, computes R@5, R@10, NDCG@10.
//! Compares against `MemPalace` baseline: R@5=0.966, R@10=0.982, NDCG@10=0.889
//!
//! Usage:
//! ```bash
//! cargo run --example bench_eval -- /path/to/longmemeval_s.json [--limit 50]
//! ```

use serde_json::Value;
use std::fs::File;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug)]
struct BenchmarkQuestion {
    question_id: String,
    question: String,
    answer: String,
}

impl BenchmarkQuestion {
    fn from_value(val: Value) -> Result<Self, String> {
        let obj = val.as_object().ok_or("Expected object")?;
        Ok(BenchmarkQuestion {
            question_id: extract_string(obj, "question_id")?,
            question: extract_string(obj, "question")?,
            answer: extract_string(obj, "answer")?,
        })
    }
}

fn extract_string(obj: &serde_json::Map<String, Value>, key: &str) -> Result<String, String> {
    obj.get(key)
        .ok_or_else(|| format!("Missing field: {key}"))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Null => String::new(),
            _ => v.to_string(),
        })
}

#[allow(clippy::cast_precision_loss)]
fn compute_ndcg(rank: Option<usize>, k: usize) -> f64 {
    match rank {
        Some(r) if r <= k => 1.0 / (2.0_f64.log2() + (r as f64).log2()),
        _ => 0.0,
    }
}

#[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <benchmark.json> [--limit N]", args[0]);
        std::process::exit(1);
    }

    let benchmark_path = PathBuf::from(&args[1]);
    let mut limit = None;

    for arg in &args[2..] {
        if arg.starts_with("--limit") {
            if let Some(pos) = args.iter().position(|a| a == arg) {
                if let Some(val) = args.get(pos + 1) {
                    limit = val.parse().ok();
                }
            }
        }
    }

    // Load benchmark
    let file = File::open(&benchmark_path)?;
    let values: Vec<Value> = serde_json::from_reader(file)?;
    let mut questions = Vec::new();

    for (idx, val) in values.into_iter().enumerate() {
        match BenchmarkQuestion::from_value(val) {
            Ok(q) => questions.push(q),
            Err(e) => eprintln!("Warning: skipped question at index {idx}: {e}"),
        }
    }

    let total = limit.unwrap_or(questions.len());
    eprintln!("Evaluating {total} benchmark questions ...");

    let mut recall_5 = 0.0;
    let mut recall_10 = 0.0;
    let mut ndcg_10 = 0.0;

    // Evaluate each question
    for (idx, question) in questions.into_iter().take(total).enumerate() {
        if (idx + 1) % 50 == 0 {
            eprintln!("  [{:3}/{}] evaluating...", idx + 1, total);
        }

        // Query SOUL for top 10 results via gateway subprocess
        let output = Command::new("cargo")
            .args([
                "run",
                "--release",
                "--bin",
                "lightarchitects-gateway",
                "--",
                "soul",
                "query",
                "--query",
                &question.question,
                "--top-k",
                "10",
                "--json",
            ])
            .output();

        let results: Vec<Value> = match output {
            Ok(out) if out.status.success() => {
                if let Ok(json) = serde_json::from_slice::<Value>(&out.stdout) {
                    json.get("steps")
                        .and_then(|s| s.as_array())
                        .cloned()
                        .unwrap_or_default()
                } else {
                    eprintln!("  ERROR [{}]: invalid JSON response", question.question_id);
                    continue;
                }
            }
            _ => {
                eprintln!("  ERROR [{}]: query failed", question.question_id);
                continue;
            }
        };

        // Check if expected answer appears in results
        let answer_lower = question.answer.to_lowercase();
        let mut found_rank_5: Option<usize> = None;
        let mut found_rank_10: Option<usize> = None;

        for (rank, step_val) in results.iter().enumerate() {
            if let Some(content) = step_val.get("content").and_then(|c| c.as_str()) {
                if content.to_lowercase().contains(&answer_lower) {
                    if rank < 5 {
                        found_rank_5 = Some(rank + 1);
                    }
                    found_rank_10 = Some(rank + 1);
                    break;
                }
            }
        }

        // Update metrics
        recall_5 += if found_rank_5.is_some() { 1.0 } else { 0.0 };
        recall_10 += if found_rank_10.is_some() { 1.0 } else { 0.0 };
        ndcg_10 += compute_ndcg(found_rank_10, 10);
    }

    let count = total as f64;
    recall_5 /= count;
    recall_10 /= count;
    ndcg_10 /= count;

    eprintln!("\n✓ Evaluation Complete");
    eprintln!("  R@5:     {recall_5:.3}");
    eprintln!("  R@10:    {recall_10:.3}");
    eprintln!("  NDCG@10: {ndcg_10:.3}");

    eprintln!("\n📊 MemPalace Baseline:");
    eprintln!("  R@5:     0.966 (Δ {:+.3})", recall_5 - 0.966);
    eprintln!("  R@10:    0.982 (Δ {:+.3})", recall_10 - 0.982);
    eprintln!("  NDCG@10: 0.889 (Δ {:+.3})", ndcg_10 - 0.889);

    Ok(())
}
