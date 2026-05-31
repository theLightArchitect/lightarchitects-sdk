//! Parser and rewriter for `ask` fenced blocks in SKILL.md content.
//!
//! Skill authors declare HITL checkpoints using triple-backtick `ask` blocks:
//!
//! ````text
//! ```ask
//! questions:
//!   - question: "Which deployment path?"
//!     header: "Deploy"
//!     multiSelect: false
//!     options:
//!       - label: "Canary"
//!         description: "10% traffic first"
//!       - label: "Full"
//!         description: "All traffic immediately"
//! ```
//! ````
//!
//! `parse_blocks` extracts these into [`AskBlock`] values with their line ranges.
//! `rewrite_ask_blocks` replaces each block with a deterministic inline
//! instruction that tells the LLM to call the `question` tool — making HITL
//! pauses mechanical rather than LLM-discretionary.

use crate::core_tools::question::QuestionInput;

/// A parsed `ask` fenced block extracted from a SKILL.md file.
#[derive(Debug, Clone)]
pub struct AskBlock {
    /// 1-indexed line number of the opening ` ```ask ` fence.
    pub line_start: usize,
    /// 1-indexed line number of the closing ` ``` ` fence.
    pub line_end: usize,
    /// Validated `QuestionInput` deserialized from the block's YAML body.
    pub input: QuestionInput,
}

/// Error returned when an `ask` block cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-indexed line number where the block started.
    pub line: usize,
    /// Human-readable reason.
    pub reason: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ask block at line {}: {}", self.line, self.reason)
    }
}

/// Parse all `ask` fenced blocks from `content`.
///
/// Returns `(blocks, errors)` — blocks that parsed cleanly and any that failed.
/// A failed block is left in the content unchanged by `rewrite_ask_blocks`.
///
/// # Format
///
/// Opening fence: a line whose trimmed content equals ` ```ask `.
/// Closing fence: a line whose trimmed content equals ` ``` `.
/// Body: valid YAML serializable as [`QuestionInput`].
pub fn parse_blocks(content: &str) -> (Vec<AskBlock>, Vec<ParseError>) {
    let mut blocks = Vec::new();
    let mut errors = Vec::new();

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line == "```ask" {
            let open_line = i + 1; // 1-indexed
            let body_start = i + 1;
            let mut close_idx = None;

            for (j, line_j) in lines.iter().enumerate().skip(i + 1) {
                if line_j.trim() == "```" {
                    close_idx = Some(j);
                    break;
                }
            }

            match close_idx {
                None => {
                    errors.push(ParseError {
                        line: open_line,
                        reason: "unclosed ```ask block (no matching ``` found)".to_owned(),
                    });
                    // Advance past the unclosed opener to avoid infinite loops.
                    i += 1;
                    continue;
                }
                Some(close) => {
                    let body: String = lines[body_start..close].join("\n");
                    match serde_yaml::from_str::<QuestionInput>(&body) {
                        Ok(input) => {
                            blocks.push(AskBlock {
                                line_start: open_line,
                                line_end: close + 1, // 1-indexed
                                input,
                            });
                        }
                        Err(e) => {
                            errors.push(ParseError {
                                line: open_line,
                                reason: format!("YAML parse failed: {e}"),
                            });
                        }
                    }
                    i = close + 1;
                    continue;
                }
            }
        }
        i += 1;
    }

    (blocks, errors)
}

/// Rewrite `ask` fenced blocks in `content` to deterministic inline instructions.
///
/// Each successfully-parsed block is replaced with a `[HITL CHECKPOINT]` section
/// that instructs the LLM to call the `question` tool with the exact JSON input.
/// Blocks that fail to parse are left unchanged so the error is visible.
///
/// Returns `(rewritten_content, block_count, error_count)`.
pub fn rewrite_ask_blocks(content: &str) -> (String, usize, usize) {
    let (blocks, errors) = parse_blocks(content);
    let error_count = errors.len();

    if blocks.is_empty() {
        return (content.to_owned(), 0, error_count);
    }

    let lines: Vec<&str> = content.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());
    let mut block_iter = blocks.iter().peekable();
    let mut i = 0usize;

    while i < lines.len() {
        // line_start/line_end are 1-indexed; i is 0-indexed.
        if let Some(block) = block_iter.peek() {
            if i + 1 == block.line_start {
                // Emit the inline HITL instruction instead of the raw block.
                let json = serde_json::to_string(&block.input).unwrap_or_else(|_| "{}".to_owned());
                result_lines.push(format!(
                    "[HITL CHECKPOINT — call the question tool now with this exact input:\n```json\n{json}\n```\nWait for the operator tool_result answer before continuing.]"
                ));
                // Skip all lines of the original block (open fence through close fence).
                i = block.line_end; // line_end is 1-indexed == next 0-indexed line after close
                block_iter.next();
                continue;
            }
        }
        result_lines.push(lines[i].to_owned());
        i += 1;
    }

    (result_lines.join("\n"), blocks.len(), error_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Golden-file fixtures ──────────────────────────────────────────────────

    const SINGLE_SELECT_BLOCK: &str = r#"```ask
questions:
  - question: "Deploy to production?"
    header: "Deploy"
    multiSelect: false
    options:
      - label: "Yes"
        description: "Deploy immediately"
      - label: "No"
        description: "Abort deployment"
```"#;

    const MULTI_SELECT_BLOCK: &str = r#"```ask
questions:
  - question: "Which features to enable?"
    header: "Features"
    multiSelect: true
    options:
      - label: "Alpha"
        description: "Enable alpha features"
      - label: "Beta"
        description: "Enable beta features"
      - label: "Stable"
        description: "Enable stable features"
```"#;

    const MULTI_QUESTION_BLOCK: &str = r#"```ask
questions:
  - question: "Choose deployment strategy"
    header: "Strategy"
    multiSelect: false
    options:
      - label: "Canary"
        description: "10% traffic first"
      - label: "Full"
        description: "All traffic immediately"
  - question: "Notify team?"
    header: "Notify"
    multiSelect: false
    options:
      - label: "Yes"
        description: "Send Slack alert"
      - label: "No"
        description: "Silent deploy"
```"#;

    const HEADLESS_POLICY_BLOCK: &str = r#"```ask
headlessPolicy: auto_first
questions:
  - question: "Continue in CI?"
    header: "CI Gate"
    multiSelect: false
    options:
      - label: "Continue"
        description: "Proceed automatically"
      - label: "Halt"
        description: "Stop pipeline"
```"#;

    const MALFORMED_YAML_BLOCK: &str = r#"```ask
this: is: not: valid: yaml: {{{
```"#;

    const NESTED_OPTIONS_NEWLINES: &str = "```ask\nquestions:\n  - question: \"Multi-line\\noption desc?\"\n    header: \"Test\"\n    multiSelect: false\n    options:\n      - label: \"A\"\n        description: \"First\\nline\\nsecond line\"\n      - label: \"B\"\n        description: \"Short\"\n```";

    #[test]
    fn single_select_parses_correctly() {
        let (blocks, errors) = parse_blocks(SINGLE_SELECT_BLOCK);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(blocks.len(), 1);
        let b = &blocks[0];
        assert_eq!(b.line_start, 1);
        assert_eq!(b.input.questions.len(), 1);
        assert_eq!(b.input.questions[0].question, "Deploy to production?");
        assert!(!b.input.questions[0].multi_select);
        assert_eq!(b.input.questions[0].options.len(), 2);
    }

    #[test]
    fn multi_select_parses_correctly() {
        let (blocks, errors) = parse_blocks(MULTI_SELECT_BLOCK);
        assert!(errors.is_empty());
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].input.questions[0].multi_select);
        assert_eq!(blocks[0].input.questions[0].options.len(), 3);
    }

    #[test]
    fn multi_question_array_has_correct_structure() {
        let (blocks, errors) = parse_blocks(MULTI_QUESTION_BLOCK);
        assert!(errors.is_empty());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].input.questions.len(), 2);
        assert_eq!(blocks[0].input.questions[0].header, "Strategy");
        assert_eq!(blocks[0].input.questions[1].header, "Notify");
    }

    #[test]
    fn headless_policy_declared() {
        use crate::core_tools::question::HeadlessPolicy;
        let (blocks, errors) = parse_blocks(HEADLESS_POLICY_BLOCK);
        assert!(errors.is_empty());
        assert_eq!(
            blocks[0].input.headless_policy,
            Some(HeadlessPolicy::AutoFirst)
        );
    }

    #[test]
    fn malformed_yaml_produces_parse_error_with_line_number() {
        let (blocks, errors) = parse_blocks(MALFORMED_YAML_BLOCK);
        assert!(blocks.is_empty());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 1);
        assert!(errors[0].reason.contains("YAML parse failed"));
    }

    #[test]
    fn unclosed_block_produces_error() {
        let content = "```ask\nquestions:\n  - question: \"Open?\"";
        let (blocks, errors) = parse_blocks(content);
        assert!(blocks.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].reason.contains("unclosed"));
    }

    #[test]
    fn nested_options_description_with_newlines() {
        let (blocks, errors) = parse_blocks(NESTED_OPTIONS_NEWLINES);
        assert!(errors.is_empty(), "errors: {errors:?}");
        assert_eq!(blocks[0].input.questions[0].options[0].label, "A");
    }

    #[test]
    fn rewrite_replaces_block_with_checkpoint_instruction() {
        let content = format!("Before.\n{SINGLE_SELECT_BLOCK}\nAfter.");
        let (rewritten, count, err_count) = rewrite_ask_blocks(&content);
        assert_eq!(count, 1);
        assert_eq!(err_count, 0);
        assert!(
            rewritten.contains("[HITL CHECKPOINT"),
            "missing checkpoint marker"
        );
        assert!(
            rewritten.contains("question tool"),
            "missing tool instruction"
        );
        assert!(rewritten.contains("Before."));
        assert!(rewritten.contains("After."));
        assert!(!rewritten.contains("```ask"), "raw block should be gone");
    }

    #[test]
    fn rewrite_preserves_content_outside_blocks() {
        let content = "# Header\n\nSome text.\n\n```ask\nquestions:\n  - question: \"Q?\"\n    header: \"H\"\n    multiSelect: false\n    options:\n      - label: \"A\"\n        description: \"D\"\n```\n\nTrailing text.";
        let (rewritten, count, _) = rewrite_ask_blocks(content);
        assert_eq!(count, 1);
        assert!(rewritten.contains("# Header"));
        assert!(rewritten.contains("Some text."));
        assert!(rewritten.contains("Trailing text."));
    }

    #[test]
    fn make_input_helper_single_select() {
        // Mirror of question.rs test naming convention (GATE 3 test suite).
        let yaml = "questions:\n  - question: \"Approve?\"\n    header: \"Gate\"\n    multiSelect: false\n    options:\n      - label: \"Yes\"\n        description: \"Approve the change\"\n      - label: \"No\"\n        description: \"Reject\"";
        let input: QuestionInput = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(input.questions[0].options.len(), 2);
    }

    #[test]
    fn make_input_helper_multi_select() {
        let yaml = "questions:\n  - question: \"Flags?\"\n    header: \"Flags\"\n    multiSelect: true\n    options:\n      - label: \"A\"\n        description: \"Flag A\"\n      - label: \"B\"\n        description: \"Flag B\"";
        let input: QuestionInput = serde_yaml::from_str(yaml).expect("parse");
        assert!(input.questions[0].multi_select);
    }
}
