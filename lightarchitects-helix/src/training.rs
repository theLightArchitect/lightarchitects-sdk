//! Training data collection for L-ARC helix navigation model.
//!
//! When `SOUL_TRAINING_DATA=/path/to/traces.jsonl` is set, every MCP tool
//! call that touches Neo4j emits one JSONL record:
//!
//! ```json
//! {
//!   "request_id": "...", "tool": "helix", "params": {...},
//!   "timestamp": "2026-...",
//!   "cypher_sequence": [
//!     {"op": "ensure_helix", "query": "MERGE (h:Helix {id: $id})...", "ms": 8},
//!     {"op": "fulltext_search", "query": "CALL db.index.fulltext...", "ms": 43}
//!   ],
//!   "result_count": 7
//! }
//! ```
//!
//! Architecture: thread-local accumulator set/cleared at the MCP router
//! boundary. `timed_execute` in `db.rs` pushes into it after every query.
//! Zero overhead when the env var is not set.

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::io::Write as _;
use std::path::PathBuf;

// ── Record Types ───────────────────────────────────────────────────────

/// A single Cypher call captured during a request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CypherCall {
    /// Operation name (e.g. `fulltext_search`, `upsert_step`)
    pub op: String,
    /// Parameterized query template — first 300 chars, `$param` placeholders
    /// preserved. Content values are never included.
    pub query: String,
    /// Elapsed milliseconds
    pub ms: u128,
}

/// One complete MCP tool invocation + its Cypher call sequence.
///
/// This is the unit written to the JSONL training file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingRecord {
    /// Unique identifier for this MCP request.
    pub request_id: String,
    /// MCP tool name that was invoked.
    pub tool: String,
    /// Parameters passed to the MCP tool.
    pub params: serde_json::Value,
    /// ISO-8601 timestamp of the tool invocation.
    pub timestamp: String,
    /// Ordered sequence of Cypher calls made during the tool execution.
    pub cypher_sequence: Vec<CypherCall>,
    /// Result count when extractable from the response (for reward signal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_count: Option<usize>,
}

// ── Thread-Local Session ───────────────────────────────────────────────

thread_local! {
    static ACTIVE_SESSION: RefCell<Option<TrainingRecord>> = const { RefCell::new(None) };
}

/// Push a Cypher call into the active session.
///
/// Called from `timed_execute` after every Neo4j query. No-op when no
/// session is active (env var not set, or non-Neo4j tool call).
pub fn record_cypher_call(op: &str, query: &str, ms: u128) {
    ACTIVE_SESSION.with(|s| {
        if let Ok(mut borrow) = s.try_borrow_mut() {
            if let Some(ref mut record) = *borrow {
                record.cypher_sequence.push(CypherCall {
                    op: op.to_owned(),
                    query: query.chars().take(300).collect(),
                    ms,
                });
            }
        }
    });
}

// ── TrainingRecorder ───────────────────────────────────────────────────

/// JSONL training data recorder — one instance per server lifetime.
///
/// Enable by setting `SOUL_TRAINING_DATA=/path/to/traces.jsonl`.
/// The file is created (with parent dirs) on first write.
#[derive(Debug)]
pub struct TrainingRecorder {
    output_path: PathBuf,
}

impl TrainingRecorder {
    /// Construct from `SOUL_TRAINING_DATA` env var.
    ///
    /// Returns `None` when the variable is unset — training is disabled,
    /// no overhead anywhere in the request path.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        std::env::var("SOUL_TRAINING_DATA").ok().map(|p| Self {
            output_path: PathBuf::from(p),
        })
    }

    /// Begin accumulating Cypher calls for a new MCP request.
    ///
    /// Must be paired with [`end_request`] at the router boundary.
    pub fn begin_request(&self, request_id: &str, tool: &str, params: &serde_json::Value) {
        ACTIVE_SESSION.with(|s| {
            *s.borrow_mut() = Some(TrainingRecord {
                request_id: request_id.to_owned(),
                tool: tool.to_owned(),
                params: params.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                cypher_sequence: Vec::new(),
                result_count: None,
            });
        });
    }

    /// End the request, consuming the accumulated record.
    ///
    /// Returns `None` when no Cypher calls were made (filesystem-only tool
    /// path — no useful training signal).
    #[must_use]
    pub fn end_request(&self, result_count: Option<usize>) -> Option<TrainingRecord> {
        ACTIVE_SESSION.with(|s| {
            s.borrow_mut().take().and_then(|mut record| {
                if record.cypher_sequence.is_empty() {
                    return None;
                }
                record.result_count = result_count;
                Some(record)
            })
        })
    }

    /// Append a training record as a JSONL line to the output file.
    ///
    /// Creates parent directories if missing.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` on file I/O failure or JSON serialization error.
    pub fn flush(&self, record: &TrainingRecord) -> std::io::Result<()> {
        if let Some(parent) = self.output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string(record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.output_path)?;
        writeln!(file, "{json}")
    }

    /// Path where training data is written.
    #[must_use]
    pub fn output_path(&self) -> &std::path::Path {
        &self.output_path
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_record_cypher_call_no_session_is_noop() {
        // No session active — should not panic
        record_cypher_call("fulltext_search", "CALL db.index...", 42);
    }

    #[test]
    fn test_begin_end_captures_calls() {
        // Use a temp recorder — from_env() would need env var set
        let recorder = TrainingRecorder {
            output_path: PathBuf::from("/tmp/test.jsonl"),
        };

        recorder.begin_request("req-1", "helix", &json!({"sibling": "eva"}));
        record_cypher_call("ensure_helix", "MERGE (h:Helix {id: $id})", 5);
        record_cypher_call("fulltext_search", "CALL db.index.fulltext...", 43);
        let record = recorder.end_request(Some(7));

        assert!(record.is_some());
        let r = record.unwrap();
        assert_eq!(r.tool, "helix");
        assert_eq!(r.cypher_sequence.len(), 2);
        assert_eq!(r.cypher_sequence[0].op, "ensure_helix");
        assert_eq!(r.cypher_sequence[1].op, "fulltext_search");
        assert_eq!(r.result_count, Some(7));
    }

    #[test]
    fn test_end_request_returns_none_when_no_cypher_calls() {
        let recorder = TrainingRecorder {
            output_path: PathBuf::from("/tmp/test.jsonl"),
        };
        recorder.begin_request("req-2", "read_note", &json!({"path": "foo"}));
        // No Cypher calls — filesystem-only tool
        let record = recorder.end_request(None);
        assert!(
            record.is_none(),
            "Filesystem-only calls produce no training record"
        );
    }

    #[test]
    fn test_query_truncated_at_300_chars() {
        let recorder = TrainingRecorder {
            output_path: PathBuf::from("/tmp/test.jsonl"),
        };
        recorder.begin_request("req-3", "query", &json!({}));
        let long_query = "X".repeat(500);
        record_cypher_call("vector_search", &long_query, 10);
        let record = recorder.end_request(None).unwrap();
        assert_eq!(record.cypher_sequence[0].query.len(), 300);
    }

    #[test]
    fn test_training_record_serializes_to_jsonl() {
        let recorder = TrainingRecorder {
            output_path: PathBuf::from("/tmp/test.jsonl"),
        };
        recorder.begin_request("req-4", "helix", &json!({"sibling": "corso"}));
        record_cypher_call("ensure_helix", "MERGE (h:Helix {id: $id})", 8);
        let record = recorder.end_request(Some(1)).unwrap();
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"tool\":\"helix\""));
        assert!(json.contains("\"op\":\"ensure_helix\""));
        assert!(json.contains("\"result_count\":1"));
    }
}
