//! `lightarchitects_ask_user` — surface a question to the user via stderr.
//!
//! In an MCP context the host (Claude Code) intercepts this and presents the
//! question to the user. The gateway writes the question to stderr so it appears
//! in the Claude Code UI, then returns a placeholder response.

use std::io::Write as _;

use serde_json::{Value, json};

use crate::core_tools::text_result;
use crate::error::GatewayError;

/// Execute `lightarchitects_ask_user`.
///
/// # Parameters (JSON object)
/// - `question` (string, required): the question to present to the user.
/// - `options` (array of strings, optional): allowed answer choices.
///
/// Writes the question (and options if provided) to stderr, then returns a
/// placeholder `"awaiting_user"` response.  The MCP host is responsible for
/// collecting the actual user input.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] when `question` is absent.
pub fn run(params: Value) -> Result<Value, GatewayError> {
    let question = params["question"]
        .as_str()
        .ok_or(GatewayError::MissingParam("question"))?;

    let options: Option<Vec<&str>> = params["options"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect());

    // Write to stderr — visible in Claude Code's UI without polluting stdout
    // (which carries MCP JSON-RPC frames).
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();

    writeln!(handle, "\n[lightarchitects_ask_user] {question}").ok();
    if let Some(ref opts) = options {
        for (i, opt) in opts.iter().enumerate() {
            writeln!(handle, "  {i}. {opt}").ok();
        }
    }

    let payload = json!({
        "question": question,
        "options": options.unwrap_or_default(),
        "response": "awaiting_user"
    });

    Ok(text_result(serde_json::to_string(&payload)?))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn returns_awaiting_user() {
        let result = run(json!({"question": "Are you ready?"})).expect("run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("awaiting_user"));
        assert!(text.contains("Are you ready?"));
    }

    #[test]
    fn includes_options_when_provided() {
        let result = run(json!({
            "question": "Pick one",
            "options": ["yes", "no"]
        }))
        .expect("run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("yes"));
        assert!(text.contains("no"));
    }

    #[test]
    fn missing_question_is_error() {
        let result = run(json!({}));
        assert!(result.is_err());
    }
}
