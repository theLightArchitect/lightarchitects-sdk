#![cfg(feature = "agent-cli")]
#![allow(clippy::too_many_lines, clippy::cast_precision_loss)]
//! BFCL-style tool-call accuracy benchmark for `OllamaCliProvider`.
//!
//! Verifies that an Ollama-served model achieves ≥ 70 % accuracy on a
//! representative subset of Berkeley Function Calling Leaderboard (BFCL)
//! single-turn, single-function-call cases.
//!
//! # Running
//!
//! Tests are marked `#[ignore]` because they require a live Ollama server.
//! Run them with:
//!
//! ```sh
//! # default model (qwen3:4b or env OLLAMA_BFCL_MODEL)
//! cargo test -p lightarchitects --test ollama_bfcl -- --ignored
//!
//! # specific model
//! OLLAMA_BFCL_MODEL=qwen3:14b cargo test -p lightarchitects --test ollama_bfcl -- --ignored
//! ```
//!
//! # Threshold gate
//!
//! The suite enforces `pass_rate >= 0.70` (70 %).  At gate time the observed
//! score is written to `.gate-evals/phase-3-gate.yaml` by /GATE.
//!
//! ## Scoring
//!
//! A case is "correct" when the model's response contains:
//! 1. A JSON object with a `name` key matching the expected function name (exact).
//! 2. Every required parameter key present in the arguments object.
//!
//! Partial parameter values are not checked — presence is sufficient for the
//! 70 % threshold gate (full value parity is a Phase 6 accuracy gate).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

// ── BFCL test cases ────────────────────────────────────────────────────────────

/// One BFCL-style test case.
struct BfclCase {
    /// Human-readable label for test output.
    label: &'static str,
    /// JSON tool schema (Anthropic `tools[]` format).
    tool_schema: serde_json::Value,
    /// User prompt that should trigger the tool call.
    user_prompt: &'static str,
    /// Expected function name in the model's `tool_use` response.
    expected_fn: &'static str,
    /// Required parameter keys that must appear in the arguments object.
    required_params: &'static [&'static str],
}

fn bfcl_cases() -> Vec<BfclCase> {
    use serde_json::json;
    vec![
        BfclCase {
            label: "get_weather",
            tool_schema: json!({
                "name": "get_current_weather",
                "description": "Get the current weather for a location.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "location": {"type": "string", "description": "City name, e.g. 'San Francisco, CA'"},
                        "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}
                    },
                    "required": ["location"]
                }
            }),
            user_prompt: "What's the weather like in San Francisco right now?",
            expected_fn: "get_current_weather",
            required_params: &["location"],
        },
        BfclCase {
            label: "search_products",
            tool_schema: json!({
                "name": "search_products",
                "description": "Search for products in the catalog by keyword.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Search keywords"},
                        "max_results": {"type": "integer", "description": "Max number of results to return"}
                    },
                    "required": ["query"]
                }
            }),
            user_prompt: "Find me some blue running shoes.",
            expected_fn: "search_products",
            required_params: &["query"],
        },
        BfclCase {
            label: "calculator_add",
            tool_schema: json!({
                "name": "calculator",
                "description": "Perform arithmetic operations.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "operation": {"type": "string", "enum": ["add", "subtract", "multiply", "divide"]},
                        "a": {"type": "number"},
                        "b": {"type": "number"}
                    },
                    "required": ["operation", "a", "b"]
                }
            }),
            user_prompt: "What is 42 plus 58?",
            expected_fn: "calculator",
            required_params: &["operation", "a", "b"],
        },
        BfclCase {
            label: "send_email",
            tool_schema: json!({
                "name": "send_email",
                "description": "Send an email to a recipient.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "to": {"type": "string", "description": "Recipient email address"},
                        "subject": {"type": "string"},
                        "body": {"type": "string"}
                    },
                    "required": ["to", "subject", "body"]
                }
            }),
            user_prompt: "Send an email to alice@example.com with subject 'Hello' and body 'How are you?'",
            expected_fn: "send_email",
            required_params: &["to", "subject", "body"],
        },
        BfclCase {
            label: "create_calendar_event",
            tool_schema: json!({
                "name": "create_event",
                "description": "Create a calendar event.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "date": {"type": "string", "description": "ISO 8601 date, e.g. 2026-06-01"},
                        "duration_minutes": {"type": "integer"}
                    },
                    "required": ["title", "date"]
                }
            }),
            user_prompt: "Schedule a meeting called 'Sprint Review' for next Monday.",
            expected_fn: "create_event",
            required_params: &["title", "date"],
        },
        BfclCase {
            label: "translate_text",
            tool_schema: json!({
                "name": "translate",
                "description": "Translate text to another language.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "text": {"type": "string"},
                        "target_language": {"type": "string", "description": "ISO 639-1 code, e.g. 'fr', 'de'"}
                    },
                    "required": ["text", "target_language"]
                }
            }),
            user_prompt: "Translate 'Good morning' into French.",
            expected_fn: "translate",
            required_params: &["text", "target_language"],
        },
        BfclCase {
            label: "get_stock_price",
            tool_schema: json!({
                "name": "get_stock_price",
                "description": "Retrieve the current stock price for a ticker symbol.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "ticker": {"type": "string", "description": "Stock ticker symbol, e.g. 'AAPL'"}
                    },
                    "required": ["ticker"]
                }
            }),
            user_prompt: "What is Apple's current stock price?",
            expected_fn: "get_stock_price",
            required_params: &["ticker"],
        },
        BfclCase {
            label: "file_read",
            tool_schema: json!({
                "name": "read_file",
                "description": "Read the contents of a file.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Absolute file path"},
                        "encoding": {"type": "string", "default": "utf-8"}
                    },
                    "required": ["path"]
                }
            }),
            user_prompt: "Read the file at /etc/hostname.",
            expected_fn: "read_file",
            required_params: &["path"],
        },
        BfclCase {
            label: "database_query",
            tool_schema: json!({
                "name": "query_database",
                "description": "Execute a SQL query against the database.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "sql": {"type": "string", "description": "SQL SELECT statement"},
                        "database": {"type": "string", "description": "Database name"}
                    },
                    "required": ["sql", "database"]
                }
            }),
            user_prompt: "Query the users table in the 'app_db' database to get all active users.",
            expected_fn: "query_database",
            required_params: &["sql", "database"],
        },
        BfclCase {
            label: "set_reminder",
            tool_schema: json!({
                "name": "set_reminder",
                "description": "Set a reminder for a specified time.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "message": {"type": "string"},
                        "remind_at": {"type": "string", "description": "ISO 8601 datetime"}
                    },
                    "required": ["message", "remind_at"]
                }
            }),
            user_prompt: "Remind me to take my medication at 9pm tonight.",
            expected_fn: "set_reminder",
            required_params: &["message", "remind_at"],
        },
    ]
}

// ── Scoring ────────────────────────────────────────────────────────────────────

/// Returns `true` when `response` contains a tool-use block with the correct
/// function name and all required parameter keys.
fn score_response(response: &str, expected_fn: &str, required_params: &[&str]) -> bool {
    // The response is text; we search for a JSON object containing "name": "<fn>"
    // and an "input" or "parameters" object with the required keys.
    //
    // Strategy: find all JSON objects in the response via serde_json, and check
    // if any has a matching "name" and required argument keys.

    // Try direct parse first (model returned a clean JSON object).
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(response.trim()) {
        if check_json_object(&v, expected_fn, required_params) {
            return true;
        }
    }

    // Scan for JSON objects embedded in prose.
    let mut depth: i32 = 0;
    let mut start = None;
    let chars: Vec<char> = response.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        match c {
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        let slice: String = chars[s..=i].iter().collect();
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&slice) {
                            if check_json_object(&v, expected_fn, required_params) {
                                return true;
                            }
                        }
                    }
                    start = None;
                }
            }
            _ => {}
        }
    }

    false
}

fn check_json_object(v: &serde_json::Value, expected_fn: &str, required_params: &[&str]) -> bool {
    // Matches Anthropic tool_use format: {"type":"tool_use","name":"fn","input":{...}}
    // or OpenAI format: {"name":"fn","arguments":{...}}
    // or flat: {"function":{"name":"fn","arguments":{...}}}

    let name_match = v["name"].as_str() == Some(expected_fn)
        || v["function"]["name"].as_str() == Some(expected_fn);

    if !name_match {
        return false;
    }

    let args = if v["input"].is_object() {
        &v["input"]
    } else if v["arguments"].is_object() {
        &v["arguments"]
    } else if v["function"]["arguments"].is_object() {
        &v["function"]["arguments"]
    } else {
        // Arguments may be a JSON string (OpenAI-style).
        if let Some(s) = v["arguments"].as_str() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                return required_params.iter().all(|p| !parsed[p].is_null());
            }
        }
        if let Some(s) = v["function"]["arguments"].as_str() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                return required_params.iter().all(|p| !parsed[p].is_null());
            }
        }
        return required_params.is_empty();
    };

    required_params.iter().all(|p| !args[p].is_null())
}

// ── Integration test ───────────────────────────────────────────────────────────

/// Check whether Ollama is reachable at the configured base URL.
async fn ollama_is_reachable(base_url: &str) -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();
    client
        .get(format!("{base_url}/api/version"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

#[tokio::test]
#[ignore = "requires a live Ollama instance; run with --ignored"]
async fn qwen3_bfcl_tool_call_accuracy_70pct() {
    use futures_util::stream::StreamExt as _;
    use lightarchitects::agent::{AgentRequest, LlmAgentProvider, OllamaCliProvider};

    let base_url =
        std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_owned());
    let model = std::env::var("OLLAMA_BFCL_MODEL").unwrap_or_else(|_| "qwen3:4b".to_owned());

    if !ollama_is_reachable(&base_url).await {
        eprintln!("SKIP: Ollama not reachable at {base_url}");
        return;
    }

    let auth_token = std::env::var("OLLAMA_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .map(secrecy::SecretString::from);
    let provider = OllamaCliProvider::new(&model, auth_token).expect("model in registry");
    let cases = bfcl_cases();
    let total = cases.len();
    let mut passed = 0_usize;

    for case in &cases {
        // Build a prompt that includes the tool schema so the model knows what to call.
        let schema_json = serde_json::to_string_pretty(&case.tool_schema).unwrap_or_default();
        let prompt = format!(
            "You are a function-calling assistant. You have access to this function:\n\n\
             {schema_json}\n\n\
             When the user asks something that matches the function, respond with ONLY a \
             JSON object like: {{\"name\": \"<fn_name>\", \"input\": {{...args...}}}}\n\
             Do not add any explanation text.\n\n\
             User: {}",
            case.user_prompt
        );

        let req = AgentRequest {
            sibling_identity: String::new(),
            user_prompt: prompt,
            schema: None,
            allowed_tools: vec![],
            max_turns: 1,
            max_budget_usd: 1.0,
            model_hint: Some(model.clone()),
            parent_span_id: None,
            chain_origin: None,
            chain_depth: 0,
            aud: None,
            conversation_history: Vec::new(),
            tool_definitions: Vec::new(),
        };

        let sanitized = match req.sanitize() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  [{}] sanitization failed: {e}", case.label);
                continue;
            }
        };

        let response_text = match provider.spawn_streaming(sanitized).await {
            Ok(mut stream) => {
                let mut text = String::new();
                while let Some(ev) = stream.next().await {
                    if let lightarchitects::agent::ProviderEvent::TextDelta { text: t, .. } = ev {
                        text.push_str(&t);
                    }
                }
                text
            }
            Err(e) => {
                eprintln!("  [{}] streaming error: {e}", case.label);
                continue;
            }
        };

        let ok = score_response(&response_text, case.expected_fn, case.required_params);
        if ok {
            passed += 1;
            eprintln!("  [{}] PASS", case.label);
        } else {
            eprintln!("  [{}] FAIL — response: {response_text}", case.label);
        }
    }

    let pass_rate = passed as f64 / total as f64;
    eprintln!(
        "\nBFCL result: {passed}/{total} ({:.0}%) — threshold 70%",
        pass_rate * 100.0
    );

    assert!(
        pass_rate >= 0.70,
        "BFCL pass rate {:.0}% < 70% threshold ({passed}/{total} cases passed)",
        pass_rate * 100.0
    );
}

// ── Scoring unit tests (no Ollama required) ────────────────────────────────────

#[test]
fn score_clean_json_response_passes() {
    let resp = r#"{"name": "get_current_weather", "input": {"location": "San Francisco"}}"#;
    assert!(score_response(resp, "get_current_weather", &["location"]));
}

#[test]
fn score_wrong_function_name_fails() {
    let resp = r#"{"name": "wrong_fn", "input": {"location": "SF"}}"#;
    assert!(!score_response(resp, "get_current_weather", &["location"]));
}

#[test]
fn score_missing_required_param_fails() {
    let resp = r#"{"name": "get_current_weather", "input": {}}"#;
    assert!(!score_response(resp, "get_current_weather", &["location"]));
}

#[test]
fn score_json_embedded_in_prose_passes() {
    let resp = r#"Sure, I'll call the function. {"name": "get_current_weather", "input": {"location": "NYC"}} That's the call."#;
    assert!(score_response(resp, "get_current_weather", &["location"]));
}

#[test]
fn score_openai_format_passes() {
    let resp = r#"{"name": "calculator", "arguments": {"operation": "add", "a": 1, "b": 2}}"#;
    assert!(score_response(resp, "calculator", &["operation", "a", "b"]));
}

#[test]
fn score_no_required_params_always_passes_name_match() {
    let resp = r#"{"name": "get_stock_price", "input": {}}"#;
    assert!(score_response(resp, "get_stock_price", &[]));
}
