//! Conversation-level JSONL tracing with automatic pivot detection.
//!
//! This module mirrors the behaviour of the `trace-conversation.sh` PostToolUse
//! hook but as a native Rust API, enabling lÆx0-cli (and any other application)
//! to emit the same AYIN conversation JSONL format without a shell hook.
//!
//! # Feature flag
//!
//! All types in this module are **compile-time opt-in** via the `conversations`
//! Cargo feature. When the feature is disabled every public type degrades to a
//! zero-cost noop — the same pattern used by [`lightarchitects::ayin::ObservableTransport`].
//!
//! ```toml
//! # No tracing (default — zero cost)
//! lightarchitects-ayin = { path = "../lightarchitects-ayin" }
//!
//! # Enable conversation JSONL + pivot detection
//! lightarchitects-ayin = { path = "../lightarchitects-ayin", features = ["conversations"] }
//! ```
//!
//! # Schema
//!
//! Records match the v2 schema defined in
//! `~/lightarchitects/soul/helix/ayin/conversations/SCHEMA.md`.
//!
//! # `CognitivePhase` is always compiled
//!
//! [`CognitivePhase`] and its [`CognitivePhase::from_tool`] constructor are
//! **not** feature-gated — lÆx0-cli needs them for routing logic independent
//! of whether AYIN tracing is enabled.

// ── CognitivePhase — always compiled (no feature gate) ───────────────────────

/// Ordered cognitive phases used for Rule 2 backtrack pivot detection.
///
/// Tools are mapped to phases that reflect the cognitive work being done:
/// reading/searching (explore) → writing (execute) → running/testing (verify)
/// → handing off to agents or skills (deliver).
///
/// A pivot is detected when recovery moves to an *earlier* phase than the
/// error (e.g. `Bash` fails then the next success is a `Read` → verify(2) →
/// explore(0) = backtrack).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CognitivePhase {
    /// Read, Grep, Glob, LS — information gathering.
    Explore = 0,
    /// `Edit`, `Write`, `NotebookEdit` — code production.
    Execute = 1,
    /// Bash — running, testing, building.
    Verify = 2,
    /// Agent, Skill, mcp__* — hand-off to sub-agents.
    Deliver = 3,
}

impl CognitivePhase {
    /// Map a Claude Code tool name to its cognitive phase.
    ///
    /// Matches the mapping in `trace-conversation.sh::tool_phase_name()`.
    #[must_use]
    pub fn from_tool(tool_name: &str) -> Self {
        match tool_name {
            "Read" | "Grep" | "Glob" | "LS" => Self::Explore,
            "Edit" | "Write" | "NotebookEdit" => Self::Execute,
            "Bash" => Self::Verify,
            _ => Self::Deliver,
        }
    }

    /// Numeric order value — lower = earlier phase.
    #[must_use]
    pub fn order(self) -> u8 {
        self as u8
    }
}

// ── Feature-enabled: full implementation ─────────────────────────────────────

#[cfg(feature = "conversations")]
mod conversations_impl {
    use std::path::{Path, PathBuf};

    use crate::core::paths;
    use serde::{Deserialize, Serialize};
    use thiserror::Error;
    use tokio::fs::OpenOptions;
    use tokio::io::AsyncWriteExt as _;

    use super::CognitivePhase;

    // ── Error ─────────────────────────────────────────────────────────────────

    /// Errors that can occur during conversation tracing.
    #[derive(Debug, Error)]
    pub enum ConversationError {
        /// A JSONL record could not be serialised.
        #[error("serialise record: {0}")]
        Serialise(#[from] serde_json::Error),
        /// The trace file could not be written.
        #[error("write trace file {path}: {source}")]
        Write {
            /// The file that could not be written.
            path: PathBuf,
            /// The underlying I/O error.
            #[source]
            source: std::io::Error,
        },
        /// The trace directory could not be created.
        #[error("create trace dir {path}: {source}")]
        CreateDir {
            /// The directory path that failed.
            path: PathBuf,
            /// The underlying I/O error.
            #[source]
            source: std::io::Error,
        },
    }

    // ── Record types (SCHEMA.md v2) ───────────────────────────────────────────

    /// A single tool-call record (`type: "tool"`).
    #[derive(Debug, Serialize, Deserialize)]
    pub struct ToolRecord {
        /// Always `"tool"`.
        #[serde(rename = "type")]
        pub record_type: String,
        /// The request ID this tool call belongs to.
        pub request_id: String,
        /// Auto-incrementing node index within the request (1, 2, 3…).
        pub node_id: u64,
        /// ISO 8601 UTC timestamp.
        pub timestamp: String,
        /// Claude Code tool name (e.g. `"Read"`, `"Bash"`, `"mcp__SOUL__soulTools"`).
        pub tool_name: String,
        /// Human-readable summary of the tool input (max 120 chars).
        pub input_summary: String,
        /// Wall-clock duration in milliseconds (0 when not measured).
        pub duration_ms: u64,
        /// `true` if the tool call succeeded.
        pub success: bool,
        /// Error token if the call failed, `null` otherwise.
        pub error: Option<String>,
        /// Brief preview of the result (max 120 chars).
        pub result_preview: String,
        /// Branch index: 0 = initial approach, increments after each pivot.
        pub branch: u32,
        /// UUID of the corresponding AYIN `TraceSpan` for cross-layer correlation.
        ///
        /// Set by the CLI when it emits both a conversation record and a span for
        /// the same tool call. `None` for records produced without AYIN tracing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub span_ref: Option<String>,
    }

    /// An auto-detected or manual pivot record (`type: "pivot"`).
    #[derive(Debug, Serialize, Deserialize)]
    pub struct PivotRecord {
        /// Always `"pivot"`.
        #[serde(rename = "type")]
        pub record_type: String,
        /// The request ID this pivot belongs to.
        pub request_id: String,
        /// `node_id` of the last erroring tool call that triggered the pivot.
        pub after_node: u64,
        /// ISO 8601 UTC timestamp.
        pub timestamp: String,
        /// Human-readable reason (error message or thinking summary).
        pub reason: String,
        /// The new branch number being started.
        pub branch: u32,
        /// `"auto"` for pattern-detected pivots; `"manual"` for Claude-annotated.
        pub source: String,
        /// Detection rule that fired (`"rule1"` or `"rule2"`), if auto.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub rule: Option<String>,
        /// UUID of the erroring tool-call `TraceSpan` that triggered this pivot.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub span_ref: Option<String>,
    }

    // ── PivotState ────────────────────────────────────────────────────────────

    /// Tracks consecutive errors and branch state for pivot detection.
    ///
    /// This is the Rust equivalent of the v2 JSON state file maintained by
    /// `trace-conversation.sh`. It implements the same two detection rules:
    ///
    /// - **Rule 1**: 2+ consecutive errors → next success is a definite pivot.
    /// - **Rule 2**: 1 error → next success in an earlier cognitive phase is a
    ///   backtrack pivot.
    #[derive(Debug, Default, Clone)]
    pub struct PivotState {
        node_id: u64,
        branch: u32,
        consecutive_errors: u32,
        last_error_node: u64,
        last_error_reason: String,
        last_error_phase: Option<CognitivePhase>,
    }

    /// The result of a pivot check after a successful tool call.
    #[derive(Debug)]
    pub enum PivotCheckResult {
        /// No pivot detected.
        None,
        /// A pivot was detected. The embedded record is ready to append.
        Pivot {
            /// Serialisable pivot record.
            record: PivotRecord,
        },
    }

    impl PivotState {
        /// Create a fresh state for a new request.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Current branch index.
        #[must_use]
        pub fn branch(&self) -> u32 {
            self.branch
        }

        /// Current node counter (last emitted `node_id`).
        #[must_use]
        pub fn node_id(&self) -> u64 {
            self.node_id
        }

        /// Advance the node counter and check for a pivot after a **successful**
        /// tool call.
        ///
        /// Returns `PivotCheckResult::Pivot` when a pivot should be emitted,
        /// updating `self.branch` in-place so the subsequent tool record uses
        /// the new branch number.
        pub fn advance_success(
            &mut self,
            tool_name: &str,
            request_id: &str,
            timestamp: &str,
        ) -> PivotCheckResult {
            self.node_id += 1;

            if self.consecutive_errors == 0 {
                return PivotCheckResult::None;
            }

            let current_phase = CognitivePhase::from_tool(tool_name);
            let emit = if self.consecutive_errors >= 2 {
                // Rule 1: definite pivot after 2+ consecutive errors.
                Some("rule1")
            } else if let Some(last_phase) = self.last_error_phase {
                // Rule 2: backtrack — recovery in an earlier cognitive phase.
                if current_phase < last_phase {
                    Some("rule2")
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(rule) = emit {
                self.branch += 1;
                let reason = std::mem::take(&mut self.last_error_reason);
                let after_node = self.last_error_node;

                // Reset error tracking.
                self.consecutive_errors = 0;
                self.last_error_node = 0;
                self.last_error_phase = None;

                PivotCheckResult::Pivot {
                    record: PivotRecord {
                        record_type: "pivot".to_owned(),
                        request_id: request_id.to_owned(),
                        after_node,
                        timestamp: timestamp.to_owned(),
                        reason,
                        branch: self.branch,
                        source: "auto".to_owned(),
                        rule: Some(rule.to_owned()),
                        // TODO(Phase A): wire from lÆx0-cli once it emits span UUIDs.
                        span_ref: None,
                    },
                }
            } else {
                // No pivot — still reset error state on first success.
                self.consecutive_errors = 0;
                self.last_error_node = 0;
                self.last_error_phase = None;
                self.last_error_reason.clear();
                PivotCheckResult::None
            }
        }

        /// Advance the node counter after a **failed** tool call.
        pub fn advance_error(&mut self, tool_name: &str, error_reason: impl Into<String>) {
            self.node_id += 1;
            self.consecutive_errors += 1;
            self.last_error_node = self.node_id;
            self.last_error_reason = error_reason.into();
            self.last_error_phase = Some(CognitivePhase::from_tool(tool_name));
        }
    }

    // ── ConversationTracer ────────────────────────────────────────────────────

    /// Writes conversation-level JSONL records to the AYIN conversations dir.
    ///
    /// Each instance is bound to a single request: it holds the `request_id`,
    /// increments a node counter per tool call, and emits pivot records
    /// automatically according to the v2 detection rules.
    ///
    /// # Concurrency
    ///
    /// `ConversationTracer` is **not** `Clone` or `Sync` — one instance per
    /// in-flight request. For concurrent requests use separate instances.
    pub struct ConversationTracer {
        dir: PathBuf,
        request_id: String,
        state: PivotState,
    }

    impl ConversationTracer {
        /// Create a tracer writing to the default AYIN conversations directory
        /// (`~/lightarchitects/soul/helix/ayin/conversations/`).
        ///
        /// Returns an error if the home directory cannot be resolved.
        #[must_use]
        pub fn new(request_id: impl Into<String>) -> Self {
            let dir = default_conversations_dir();
            Self {
                dir,
                request_id: request_id.into(),
                state: PivotState::new(),
            }
        }

        /// Create a tracer that writes to a custom directory (for tests).
        #[must_use]
        pub fn with_dir(request_id: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
            Self {
                dir: dir.into(),
                request_id: request_id.into(),
                state: PivotState::new(),
            }
        }

        /// Record one tool call and append JSONL to today's trace file.
        ///
        /// If pivot detection fires, the pivot record is appended **before**
        /// the tool record — matching the hook behaviour.
        ///
        /// # Arguments
        ///
        /// - `tool_name`: the Claude Code tool name (e.g. `"Read"`, `"Bash"`).
        /// - `input_summary`: brief description of the tool input (max 120 chars).
        /// - `success`: whether the tool call succeeded.
        /// - `error`: error token if `!success`, `None` otherwise.
        /// - `result_preview`: brief preview of the result (max 120 chars).
        /// - `duration_ms`: wall-clock duration.
        ///
        /// # Errors
        ///
        /// Returns [`ConversationError`] if the trace directory cannot be created
        /// or the JSONL line cannot be written to disk.
        #[allow(clippy::too_many_arguments)]
        pub async fn record_tool(
            &mut self,
            tool_name: &str,
            input_summary: impl Into<String>,
            success: bool,
            error: Option<String>,
            result_preview: impl Into<String>,
            duration_ms: u64,
            span_ref: Option<String>,
        ) -> Result<(), ConversationError> {
            let timestamp = utc_timestamp_now();

            // Advance state and check for pivot.
            let pivot = if success {
                self.state
                    .advance_success(tool_name, &self.request_id, &timestamp)
            } else {
                let reason = error.clone().unwrap_or_default();
                self.state.advance_error(tool_name, &reason);
                PivotCheckResult::None
            };

            let trace_file = self.trace_path();

            // Emit pivot record before tool record when detected.
            if let PivotCheckResult::Pivot { record } = pivot {
                append_jsonl(&trace_file, &record).await?;
            }

            let tool_record = ToolRecord {
                record_type: "tool".to_owned(),
                request_id: self.request_id.clone(),
                node_id: self.state.node_id(),
                timestamp,
                tool_name: tool_name.to_owned(),
                input_summary: truncate(input_summary.into(), 120),
                duration_ms,
                success,
                error,
                result_preview: truncate(result_preview.into(), 120),
                branch: self.state.branch(),
                span_ref,
            };
            append_jsonl(&trace_file, &tool_record).await
        }

        /// Path to today's JSONL trace file.
        fn trace_path(&self) -> PathBuf {
            let today = utc_date_today();
            self.dir.join(format!("{today}.jsonl"))
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn default_conversations_dir() -> PathBuf {
        paths::helix_root_or_fallback().join("ayin/conversations")
    }

    /// Append one JSON record followed by a newline to `path`.
    ///
    /// Creates the parent directory and file if they do not exist.
    async fn append_jsonl<T: serde::Serialize>(
        path: &Path,
        record: &T,
    ) -> Result<(), ConversationError> {
        // Ensure directory exists.
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|source| {
                ConversationError::CreateDir {
                    path: parent.to_owned(),
                    source,
                }
            })?;
        }

        let mut line = serde_json::to_string(record)?;
        line.push('\n');

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map_err(|source| ConversationError::Write {
                path: path.to_owned(),
                source,
            })?;

        file.write_all(line.as_bytes())
            .await
            .map_err(|source| ConversationError::Write {
                path: path.to_owned(),
                source,
            })
    }

    /// Current UTC date as `YYYY-MM-DD` — uses Howard Hinnant's civil-from-days
    /// algorithm via `std::time::SystemTime` (no `chrono` dependency).
    fn utc_date_today() -> String {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
        let (y, m, d) = civil_from_days(days);
        format!("{y:04}-{m:02}-{d:02}")
    }

    /// Current UTC timestamp as ISO 8601 (`YYYY-MM-DDTHH:MM:SSZ`).
    fn utc_timestamp_now() -> String {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
        let (y, m, d) = civil_from_days(days);
        let rem = secs % 86_400;
        let hh = rem / 3600;
        let mm = (rem % 3600) / 60;
        let ss = rem % 60;
        format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
    }

    /// Howard Hinnant's "civil from days" algorithm.
    ///
    /// Converts a count of days since the Unix epoch (1970-01-01) to a
    /// (year, month, day) civil calendar tuple. No external crates required.
    ///
    /// Reference: <https://howardhinnant.github.io/date_algorithms.html>
    ///
    /// All intermediate values stay as `i64`; the algorithm guarantees that
    /// `m` and `d` are in `[1, 12]` / `[1, 31]` and `y` fits in `i32` for
    /// any date in the foreseeable future, so the final conversions are safe.
    #[allow(clippy::similar_names)] // `doe`/`doy` are canonical Hinnant variable names
    #[allow(clippy::cast_possible_truncation)] // year fits i32 for all realistic dates
    fn civil_from_days(z: i64) -> (i32, u32, u32) {
        let z = z + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        // `z - era * 146_097` is in [0, 146_096] by construction — safe to cast.
        #[allow(clippy::cast_sign_loss)]
        let doe = (z - era * 146_097) as u32;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let y = i64::from(yoe) + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        // `y` fits i32 for any date 0001–9999; `m` ∈ [1,12], `d` ∈ [1,31].
        (y as i32, m, d)
    }

    /// Truncate a string to `max` bytes, appending `…` if truncated.
    fn truncate(mut s: String, max: usize) -> String {
        if s.len() > max {
            s.truncate(max.saturating_sub(3));
            s.push_str("...");
        }
        s
    }
}

// ── Feature-enabled public exports ───────────────────────────────────────────

#[cfg(feature = "conversations")]
pub use conversations_impl::{
    ConversationError, ConversationTracer, PivotCheckResult, PivotRecord, PivotState, ToolRecord,
};

// ── Feature-disabled: zero-cost noops ────────────────────────────────────────

#[cfg(not(feature = "conversations"))]
mod noop_conversations {
    use std::path::PathBuf;

    /// Zero-cost conversation tracer noop.
    ///
    /// When the `conversations` feature is **disabled**, all methods are
    /// inlined to `Ok(())` / `PivotCheckResult::None` with no allocations
    /// or I/O.
    pub struct ConversationTracer;

    impl ConversationTracer {
        /// No-op constructor.
        #[must_use]
        #[inline]
        pub fn new(_request_id: impl Into<String>) -> Self {
            Self
        }

        /// No-op constructor with custom dir (for tests).
        #[must_use]
        #[inline]
        pub fn with_dir(_request_id: impl Into<String>, _dir: impl Into<PathBuf>) -> Self {
            Self
        }

        /// No-op tool record.
        ///
        /// # Errors
        ///
        /// Never returns an error; the noop always succeeds.
        // `async` is present for API parity with the feature-gated real implementation.
        #[allow(clippy::unused_async)]
        #[inline]
        pub async fn record_tool(
            &mut self,
            _tool_name: &str,
            _input_summary: impl Into<String>,
            _success: bool,
            _error: Option<String>,
            _result_preview: impl Into<String>,
            _duration_ms: u64,
            _span_ref: Option<String>,
        ) -> Result<(), std::convert::Infallible> {
            Ok(())
        }
    }
}

#[cfg(not(feature = "conversations"))]
pub use noop_conversations::ConversationTracer;
