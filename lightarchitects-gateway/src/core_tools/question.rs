//! `question` — native LA operator question tool.
//!
//! Replaces Claude Code's host-level `AskUserQuestion` with a first-class
//! gateway tool that dispatches across four transports:
//!
//! 1. **Webshell SSE** — `POST /api/sessions/:id/question` → `QuestionCard.svelte`
//! 2. **CLI (dialoguer)** — terminal `Select` / `MultiSelect` prompt
//! 3. **Claude Code passthrough** — stderr intercept for CC interactive sessions
//! 4. **Headless policy** — auto-select per [`HeadlessPolicy`]
//!
//! Schema matches Anthropic's `AskUserQuestion` 1:1 so SKILL.md files need no
//! changes when migrated from the native CC host primitive.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::text_result;
use crate::{config::GatewayConfig, error::GatewayError};

/// Canonical base URL for the local webshell process.
const WEBSHELL_BASE: &str = "http://localhost:8733";

/// Long-poll budget (slightly over the webshell's 300 s TTL).
const QUESTION_TIMEOUT_SECS: u64 = 310;

/// Top-level input for the `question` tool.
///
/// Matches Anthropic's `AskUserQuestion` schema verbatim. The optional
/// `headless_policy` field is an LA extension for CI / unattended contexts.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestionInput {
    /// One or more questions to present to the operator sequentially.
    pub questions: Vec<Question>,
    /// Behaviour when no interactive transport is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headless_policy: Option<HeadlessPolicy>,
}

/// A single question with option chips.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    /// The question text shown as the modal heading.
    pub question: String,
    /// Short label shown as a chip / tag above the options (max 12 chars).
    pub header: String,
    /// When `true` the operator may select multiple options.
    #[serde(default)]
    pub multi_select: bool,
    /// Available choices.
    pub options: Vec<QuestionOption>,
}

/// One selectable option within a [`Question`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QuestionOption {
    /// Short display label.
    pub label: String,
    /// Explanation shown beneath the label.
    pub description: String,
}

/// What the `question` tool does when running without an interactive transport.
///
/// Default when absent: [`HeadlessPolicy::FailLoud`] — the tool returns an
/// error `tool_result` containing the full question so the LLM can see the
/// failure, not a silent default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HeadlessPolicy {
    /// Return an error `tool_result` with the question text — LLM sees failure.
    FailLoud,
    /// Silently select the first option and continue.
    AutoFirst,
    /// Skip the question (return empty answers) and continue.
    AutoSkip,
}

impl Default for HeadlessPolicy {
    fn default() -> Self {
        Self::FailLoud
    }
}

/// Answer returned by the operator (or auto-policy) for a `question` invocation.
///
/// One entry per question in [`QuestionInput::questions`]. For single-select
/// questions the vec contains one element; for multi-select it may contain
/// zero or more.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionAnswer {
    /// Per-question selected labels (or typed free text when no options given).
    pub answers: Vec<Vec<String>>,
}

impl QuestionAnswer {
    /// Flatten to a single readable string for the MCP `tool_result` content.
    #[must_use]
    pub fn to_tool_result_text(&self) -> String {
        self.answers
            .iter()
            .enumerate()
            .map(|(i, selected)| {
                if selected.is_empty() {
                    format!("Q{}: (no answer)", i + 1)
                } else {
                    format!("Q{}: {}", i + 1, selected.join(", "))
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Read the webshell bearer token from the canonical token file.
///
/// Returns an empty string when the file is absent or unreadable — callers
/// receive a 401 from the webshell, which surfaces clearly to the LLM.
fn read_webshell_token() -> String {
    dirs_next::home_dir()
        .map(|h| h.join(".lightarchitects").join("webshell").join(".token"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

/// Validate that `url` resolves to localhost before issuing the request.
///
/// SSRF guard: the webshell base URL must be `localhost`, `127.0.0.1`, or
/// `[::1]`. Any other host is rejected — prevents a misconfigured
/// `WEBSHELL_BASE` from reaching an unintended network target.
fn assert_localhost(url: &str) -> Result<(), GatewayError> {
    let parsed = url::Url::parse(url).map_err(|e| {
        GatewayError::InvalidRequest(format!("question: invalid webshell URL {url}: {e}"))
    })?;
    let host = parsed.host_str().unwrap_or("");
    if host == "localhost" || host == "127.0.0.1" || host == "[::1]" || host == "::1" {
        Ok(())
    } else {
        Err(GatewayError::InvalidRequest(format!(
            "question: SSRF guard — webshell host must be localhost, got '{host}'"
        )))
    }
}

/// Dispatch the `question` tool.
///
/// Serialises `params` as [`QuestionInput`], POSTs to
/// `POST /api/question` on the local webshell (long-poll, up to 310 s),
/// and returns the operator's answer as a formatted MCP `tool_result`.
///
/// # Headless behaviour
///
/// If the webshell is unreachable the tool falls back to the
/// [`HeadlessPolicy`] embedded in the request:
///
/// - `fail_loud` (default) — returns an error `tool_result`
/// - `auto_first` — selects the first option from each question silently
/// - `auto_skip` — returns empty answers for all questions
///
/// # SSRF protection
///
/// The webshell base URL is validated to be `localhost` / `127.0.0.1` /
/// `[::1]` before any HTTP request is issued.
///
/// # Errors
///
/// Returns [`GatewayError::InvalidRequest`] when `params` cannot be
/// deserialized, the webshell URL fails the SSRF guard, the HTTP client
/// cannot be constructed, the answer body is malformed, or the headless
/// policy is [`HeadlessPolicy::FailLoud`] and the webshell is unreachable.
pub async fn run(params: Value, _config: &GatewayConfig) -> Result<Value, GatewayError> {
    let input: QuestionInput = serde_json::from_value(params)
        .map_err(|e| GatewayError::InvalidRequest(format!("question: invalid input: {e}")))?;

    // W3.4 — Claude Code passthrough: when CLAUDE_CODE_INTERACTIVE is set the
    // operator's terminal is available.  Use the CLI inquire transport directly
    // instead of the webshell long-poll — avoids an unnecessary HTTP round-trip
    // when the webshell is not open.
    if std::env::var("CLAUDE_CODE_INTERACTIVE").is_ok() {
        return match super::question_transport_cli::prompt_cli(&input) {
            Ok(answer) => Ok(text_result(answer.to_tool_result_text())),
            Err(e) => headless_fallback(&input, Some(e.to_string())),
        };
    }

    let webshell_url = format!("{WEBSHELL_BASE}/api/question");
    assert_localhost(&webshell_url)?;

    let token = read_webshell_token();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(QUESTION_TIMEOUT_SECS))
        .build()
        .map_err(|e| GatewayError::InvalidRequest(format!("question: http client: {e}")))?;

    let resp = client
        .post(&webshell_url)
        .bearer_auth(&token)
        .json(&input)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let answer: QuestionAnswer = r.json().await.map_err(|e| {
                GatewayError::InvalidRequest(format!("question: bad answer body: {e}"))
            })?;
            Ok(text_result(answer.to_tool_result_text()))
        }
        Ok(r) => {
            let status = r.status();
            // Headless fallback when webshell returned a non-success status.
            headless_fallback(&input, Some(format!("webshell {status}")))
        }
        Err(e) => {
            // Webshell unreachable — apply headless policy.
            headless_fallback(&input, Some(e.to_string()))
        }
    }
}

/// Apply the [`HeadlessPolicy`] when no interactive transport is available.
fn headless_fallback(input: &QuestionInput, reason: Option<String>) -> Result<Value, GatewayError> {
    let policy = input.headless_policy.unwrap_or_default();
    match policy {
        HeadlessPolicy::FailLoud => {
            let summary: String = input
                .questions
                .iter()
                .map(|q| format!("  • {} — {}", q.header, q.question))
                .collect::<Vec<_>>()
                .join("\n");
            let msg = format!(
                "question: no interactive transport available{}.\nUnanswered questions:\n{summary}",
                reason.map_or(String::new(), |r| format!(" ({r})"))
            );
            Err(GatewayError::InvalidRequest(msg))
        }
        HeadlessPolicy::AutoFirst => {
            let answers: Vec<Vec<String>> = input
                .questions
                .iter()
                .map(|q| {
                    q.options
                        .first()
                        .map(|o| vec![o.label.clone()])
                        .unwrap_or_default()
                })
                .collect();
            Ok(text_result(
                QuestionAnswer { answers }.to_tool_result_text(),
            ))
        }
        HeadlessPolicy::AutoSkip => {
            let answers = vec![vec![]; input.questions.len()];
            Ok(text_result(
                QuestionAnswer { answers }.to_tool_result_text(),
            ))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip_single_select() {
        let raw = json!({
            "questions": [{
                "question": "What should we do?",
                "header": "Decision",
                "multiSelect": false,
                "options": [
                    {"label": "Proceed", "description": "Continue the build"},
                    {"label": "Abort", "description": "Stop here"}
                ]
            }]
        });
        let input: QuestionInput = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(input.questions.len(), 1);
        assert!(!input.questions[0].multi_select);
        assert_eq!(input.questions[0].options.len(), 2);
        assert_eq!(input.questions[0].options[0].label, "Proceed");
        // Re-serialise round-trips camelCase
        let out = serde_json::to_value(&input).unwrap();
        assert_eq!(out["questions"][0]["multiSelect"], false);
        assert!(out.get("headlessPolicy").is_none());
    }

    #[test]
    fn roundtrip_multi_select_with_headless_policy() {
        let raw = json!({
            "questions": [{
                "question": "Pick tools",
                "header": "Tools",
                "multiSelect": true,
                "options": [
                    {"label": "Read", "description": "Read files"},
                    {"label": "Write", "description": "Write files"},
                    {"label": "Bash", "description": "Run shell commands"}
                ]
            }],
            "headlessPolicy": "auto_first"
        });
        let input: QuestionInput = serde_json::from_value(raw).unwrap();
        assert!(input.questions[0].multi_select);
        assert_eq!(input.headless_policy, Some(HeadlessPolicy::AutoFirst));
    }

    #[test]
    fn answer_to_tool_result_text() {
        let ans = QuestionAnswer {
            answers: vec![
                vec!["Proceed".to_owned()],
                vec!["Read".to_owned(), "Write".to_owned()],
            ],
        };
        let text = ans.to_tool_result_text();
        assert!(text.contains("Q1: Proceed"));
        assert!(text.contains("Q2: Read, Write"));
    }

    #[test]
    fn answer_empty_per_question() {
        let ans = QuestionAnswer {
            answers: vec![vec![]],
        };
        assert!(ans.to_tool_result_text().contains("no answer"));
    }

    #[test]
    fn schema_generates_without_panic() {
        let schema = schemars::schema_for!(QuestionInput);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("questions"));
    }
}
