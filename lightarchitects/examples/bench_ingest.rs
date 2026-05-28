//! `LongMemEval` benchmark ingestion to SOUL helix.
//!
//! Ingests 500 benchmark questions into SOUL using `GraphRAG` entity extraction.
//! Creates helix entries at `helix_id` = bench-2026-05-25/{session_id}.
//!
//! Usage:
//! ```bash
//! LA_API_KEY=la_your_key cargo run --example bench_ingest -- \
//!   /path/to/longmemeval_s.json \
//!   [--dry-run] [--limit 50]
//! ```

use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use lightarchitects::soul::{IngestSource, SoulClient, TextFormat};
use serde_json::Value;
use tokio::sync::Semaphore;

// ── Types ──────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct BenchmarkQuestion {
    question_id: String,
    question_type: String,
    question: String,
    answer: String,
    haystack_session_ids: Vec<String>,
}

impl BenchmarkQuestion {
    fn from_value(val: Value) -> Result<Self, String> {
        let obj = val.as_object().ok_or("Expected object")?;

        Ok(BenchmarkQuestion {
            question_id: extract_string(obj, "question_id")?,
            question_type: extract_string(obj, "question_type")?,
            question: extract_string(obj, "question")?,
            answer: extract_string(obj, "answer")?,
            haystack_session_ids: extract_string_array(obj, "haystack_session_ids")?,
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

fn extract_string_array(
    obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, String> {
    obj.get(key)
        .ok_or_else(|| format!("Missing field: {key}"))
        .map(|v| match v {
            Value::Array(arr) => arr
                .iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        })
}

#[derive(Debug, Default)]
struct IngestStats {
    total_entries: u64,
    total_nodes: u64,
    total_edges: u64,
    errors: u64,
}

// ── Main ───────────────────────────────────────────────────────────────────────

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Usage: {} <benchmark_json> [--dry-run] [--limit N]",
            args[0]
        );
        std::process::exit(1);
    }

    let benchmark_path = PathBuf::from(&args[1]);
    let mut dry_run = false;
    let mut limit = None;

    for arg in &args[2..] {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            arg if arg.starts_with("--limit") => {
                if let Some(pos) = args.iter().position(|a| a == arg) {
                    if let Some(val) = args.get(pos + 1) {
                        limit = val.parse().ok();
                    }
                }
            }
            _ => {}
        }
    }

    // Initialize SOUL client (local stdio transport to spawned subprocess)
    let client = Arc::new(SoulClient::local_builder().build().await?);

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
    eprintln!("Ingesting {total} benchmark questions (dry_run={dry_run}) ...");

    // Ingest with parallelism (max 5 concurrent)
    let semaphore = Arc::new(Semaphore::new(5));
    let stats = Arc::new(tokio::sync::Mutex::new(IngestStats::default()));

    let mut handles = Vec::new();

    for (idx, question) in questions.into_iter().take(total).enumerate() {
        let client = client.clone();
        let semaphore = semaphore.clone();
        let stats = stats.clone();
        let dry_run = dry_run;

        let handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.ok()?;

            // Build entry content
            let content = format_question_entry(&question);
            let source_id = format!("bench-{}", question.question_id);

            // Ingest via GraphRAG
            let mut builder = client.graphrag_ingest();
            builder = builder.source(IngestSource::Inline {
                source_id: source_id.clone(),
                text: content,
                format: Some(TextFormat::Plaintext),
            });
            builder = builder.domain("benchmark").sibling("quantum");

            if dry_run {
                builder = builder.dry_run();
            }

            match builder.call().await {
                Ok(result) => {
                    let mut s = stats.lock().await;
                    s.total_entries += 1;
                    s.total_nodes += result.nodes_created;
                    s.total_edges += result.edges_created;

                    if (idx + 1) % 50 == 0 {
                        eprintln!(
                            "  [{:3}/{}] {} nodes, {} edges",
                            idx + 1,
                            total,
                            s.total_nodes,
                            s.total_edges
                        );
                    }

                    Some(())
                }
                Err(e) => {
                    eprintln!("  ERROR [{}]: {}", question.question_id, e);
                    let mut s = stats.lock().await;
                    s.errors += 1;
                    None
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all ingests
    for handle in handles {
        let _ = handle.await;
    }

    // Final stats
    let final_stats = stats.lock().await;
    eprintln!("\n✓ Ingestion Complete");
    eprintln!("  Entries:     {}", final_stats.total_entries);
    eprintln!("  Nodes:       {}", final_stats.total_nodes);
    eprintln!("  Edges:       {}", final_stats.total_edges);
    eprintln!("  Errors:      {}", final_stats.errors);

    if final_stats.errors == 0 {
        eprintln!(
            "\n✓ All {} entries ingested successfully",
            final_stats.total_entries
        );
    } else {
        eprintln!("\n⚠ {} errors during ingestion", final_stats.errors);
    }

    Ok(())
}

// ── Formatting ─────────────────────────────────────────────────────────────────

/// Format a benchmark question as plaintext for `GraphRAG` ingestion.
fn format_question_entry(q: &BenchmarkQuestion) -> String {
    let sessions_str = q.haystack_session_ids.join("\n  ");

    format!(
        "BENCHMARK QUESTION: {}

Type: {}
Question: {}
Expected Answer: {}

Source Sessions:
  {}

---
This entry represents a question from the LongMemEval benchmark with expected answer
and the list of source sessions where the answer can be found. Entity extraction will
identify question/answer entities and create relations to session IDs.",
        q.question_id, q.question_type, q.question, q.answer, sessions_str
    )
}

// ── Clone trait for SoulClient ─────────────────────────────────────────────────

// The SoulClient needs to be cloneable for parallel ingests. If not already
// implemented, uncomment below:
//
// impl Clone for SoulClient<T: Transport + Clone> {
//     fn clone(&self) -> Self { ... }
// }
