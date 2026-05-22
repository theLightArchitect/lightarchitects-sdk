//! Tool-use format translator between Claude and Ollama schemas.
//!
//! # Security model
//!
//! [`sanitize_prompt`] is a secondary defense layer against prompt injection.
//! Primary protection is `Command::new("ollama")` with arguments as separate
//! `Vec` items (execve(2) semantics — no shell interpolation). This function
//! adds defense-in-depth by rejecting prompts containing shell metacharacters
//! before they reach the subprocess argument list.
//!
//! ## Tested injection vectors
//!
//! Backtick substitution (`` ` ``), dollar expansion (`$`), output redirection
//! (`>`/`<`), pipe (`|`), background/AND/OR (`&`/`&&`/`||`), semicolon chain
//! (`;`), backslash escape (`\`), null byte and other control chars 0x00–0x1F.
//! Newline (`\n`) and tab (`\t`) are explicitly permitted.

use serde_json::{Value, json};

use super::error::OllamaError;

/// Convert a Claude `tool_use` content block to Ollama `function_call` format.
///
/// | Claude field | Ollama field | Notes |
/// |---|---|---|
/// | `name` | `function_call.name` | string identity |
/// | `input` (object) | `function_call.arguments` | JSON-serialized string |
///
/// Missing or non-string `name` fields produce an empty string. Missing `input`
/// fields produce a serialized `null` argument string.
pub fn claude_tool_use_to_ollama_function(block: &Value) -> Value {
    let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let arguments =
        serde_json::to_string(block.get("input").unwrap_or(&Value::Null)).unwrap_or_default();
    json!({
        "function_call": {
            "name": name,
            "arguments": arguments,
        }
    })
}

/// Convert an Ollama `function_call` to Claude `tool_use` format.
///
/// `id` is a caller-supplied correlation identifier (e.g. `"toolu_01..."`).
/// Malformed `arguments` strings (not valid JSON) are replaced with `null`.
pub fn ollama_function_call_to_claude_tool_use(call: &Value, id: &str) -> Value {
    let args_str = call
        .get("arguments")
        .and_then(|v| v.as_str())
        .unwrap_or("{}");
    let input: Value = serde_json::from_str(args_str).unwrap_or(Value::Null);
    let name = call.get("name").and_then(|v| v.as_str()).unwrap_or("");
    json!({
        "type": "tool_use",
        "id": id,
        "name": name,
        "input": input,
    })
}

/// Sanitize an operator-supplied prompt before passing it to `ollama run`.
///
/// Rejected characters: `` ` `` `$` `>` `<` `|` `&` `;` `\` and control
/// bytes 0x00–0x1F (except `\n` LF and `\t` HT which are permitted).
///
/// # Errors
///
/// Returns [`OllamaError::PromptInvalid`] if any forbidden character is found.
pub fn sanitize_prompt(s: &str) -> Result<String, OllamaError> {
    const BLACKLIST: &[char] = &['`', '$', '>', '<', '|', '&', ';', '\\'];
    let is_forbidden =
        |c: char| BLACKLIST.contains(&c) || ((c as u32) < 0x20 && c != '\n' && c != '\t');
    if s.chars().any(is_forbidden) {
        return Err(OllamaError::PromptInvalid);
    }
    Ok(s.to_owned())
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_to_ollama_roundtrip_simple() {
        let block = json!({
            "type": "tool_use",
            "id": "toolu_01",
            "name": "Read",
            "input": { "file_path": "/etc/hosts" }
        });
        let result = claude_tool_use_to_ollama_function(&block);
        let fc = result
            .get("function_call")
            .expect("must have function_call key");
        assert_eq!(fc["name"], "Read");
        let args: Value = serde_json::from_str(fc["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(args["file_path"], "/etc/hosts");
    }

    #[test]
    fn claude_to_ollama_roundtrip_nested_input() {
        let block = json!({
            "type": "tool_use",
            "name": "Edit",
            "input": {
                "file_path": "/src/lib.rs",
                "changes": [{ "old": "x", "new": "y" }]
            }
        });
        let result = claude_tool_use_to_ollama_function(&block);
        let args_str = result["function_call"]["arguments"].as_str().unwrap();
        let args: Value = serde_json::from_str(args_str).unwrap();
        assert_eq!(args["file_path"], "/src/lib.rs");
        assert!(args["changes"].is_array(), "nested array must round-trip");
        assert_eq!(args["changes"][0]["old"], "x");
    }

    #[test]
    fn ollama_to_claude_function_call_roundtrips() {
        let call = json!({
            "name": "Bash",
            "arguments": r#"{"command": "cargo test"}"#
        });
        let result = ollama_function_call_to_claude_tool_use(&call, "toolu_99");
        assert_eq!(result["type"], "tool_use");
        assert_eq!(result["id"], "toolu_99");
        assert_eq!(result["name"], "Bash");
        assert_eq!(result["input"]["command"], "cargo test");
    }

    #[test]
    fn sanitize_prompt_rejects_all_injection_vectors() {
        // 8 blacklisted shell metacharacters
        for &c in &['`', '$', '>', '<', '|', '&', ';', '\\'] {
            let prompt = format!("hello {c} world");
            assert!(
                sanitize_prompt(&prompt).is_err(),
                "must reject metachar {c:?}"
            );
        }
        // Control bytes 0x00–0x1F (excluding \n, \t)
        assert!(sanitize_prompt("a\x00b").is_err(), "must reject null byte");
        assert!(sanitize_prompt("a\x01b").is_err(), "must reject SOH 0x01");
        assert!(sanitize_prompt("a\x1Fb").is_err(), "must reject 0x1F");
        // Permitted control chars
        assert!(sanitize_prompt("line1\nline2").is_ok(), "\\n allowed");
        assert!(sanitize_prompt("col1\tcol2").is_ok(), "\\t allowed");
        // Normal prose
        assert!(sanitize_prompt("implement a binary search in Rust").is_ok());
        // Unicode non-control chars
        assert!(sanitize_prompt("Hej! Kan du hjälpe mig?").is_ok());
    }
}
