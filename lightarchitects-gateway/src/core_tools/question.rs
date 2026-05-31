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
