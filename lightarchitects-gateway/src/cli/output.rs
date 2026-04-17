//! Output formatting — JSON vs human-readable.
//!
//! # Output format
//!
//! Every command accepts `--output-format <json|text>` (global flag, default: `text`).
//!
//! * `text` — human-readable prose printed directly to stdout.
//! * `json` — structured JSON (`serde_json::to_string_pretty`). Fields whose
//!   key matches a sensitive pattern (`api_key`, `token`, `secret`, `password`,
//!   `credential`, case-insensitive) are redacted before printing.

use serde_json::Value;

/// Sensitive key patterns — case-insensitive substring match against JSON object keys.
const SENSITIVE_PATTERNS: &[&str] = &[
    "api-key",
    "api_key",
    "apikey",
    "token",
    "secret",
    "password",
    "credential",
];

/// Output mode selected by `--output-format`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputMode {
    /// Human-readable text (default).
    #[default]
    Human,
    /// Machine-readable JSON with sensitive-field redaction.
    Json,
}

impl std::str::FromStr for OutputMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "text" => Ok(Self::Human),
            other => Err(format!(
                "unknown output format '{other}': expected 'json' or 'text'"
            )),
        }
    }
}

/// Print a text response in the selected output mode.
///
/// - `text`: prints `text` directly, followed by a newline.
/// - `json`: wraps `text` in `{"output": "..."}` and pretty-prints.
pub fn print_text(mode: OutputMode, text: &str) {
    match mode {
        OutputMode::Human => println!("{text}"),
        OutputMode::Json => {
            let v = serde_json::json!({ "output": text });
            println!("{}", pretty(&v));
        }
    }
}

/// Print a JSON value in the selected output mode.
///
/// - `text`: pretty-prints the value as JSON.
/// - `json`: redacts sensitive keys then pretty-prints.
pub fn print_value(mode: OutputMode, value: &Value) {
    match mode {
        OutputMode::Human => println!("{}", pretty(value)),
        OutputMode::Json => {
            let redacted = redact_sensitive(value.clone());
            println!("{}", pretty(&redacted));
        }
    }
}

/// Print a list of string entries, one per line (human) or as JSON array.
pub fn print_list(mode: OutputMode, items: &[String]) {
    match mode {
        OutputMode::Human => {
            for item in items {
                println!("  {item}");
            }
        }
        OutputMode::Json => {
            let v = Value::Array(items.iter().map(|s| Value::String(s.clone())).collect());
            println!("{}", pretty(&v));
        }
    }
}

/// Recursively redact JSON object keys that match any [`SENSITIVE_PATTERNS`].
///
/// Matching is case-insensitive substring: a key is redacted when any pattern
/// appears anywhere in the lowercase key. Arrays and nested objects are recursed.
pub fn redact_sensitive(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let redacted = map
                .into_iter()
                .map(|(k, v)| {
                    if is_sensitive_key(&k) {
                        (k, Value::String("[REDACTED]".to_owned()))
                    } else {
                        (k, redact_sensitive(v))
                    }
                })
                .collect();
            Value::Object(redacted)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(redact_sensitive).collect()),
        other => other,
    }
}

/// Returns `true` when `key` (case-insensitive) contains any sensitive pattern.
fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_PATTERNS.iter().any(|p| lower.contains(p))
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn output_mode_from_str_json() {
        let m: OutputMode = "json".parse().unwrap();
        assert_eq!(m, OutputMode::Json);
    }

    #[test]
    fn output_mode_from_str_text() {
        let m: OutputMode = "text".parse().unwrap();
        assert_eq!(m, OutputMode::Human);
    }

    #[test]
    fn output_mode_from_str_case_insensitive() {
        assert_eq!("JSON".parse::<OutputMode>().unwrap(), OutputMode::Json);
        assert_eq!("Text".parse::<OutputMode>().unwrap(), OutputMode::Human);
    }

    #[test]
    fn output_mode_from_str_invalid() {
        let r: Result<OutputMode, _> = "yaml".parse();
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("yaml"));
    }

    #[test]
    fn redact_api_key() {
        let v = json!({ "api_key": "sk-secret", "name": "test" });
        let r = redact_sensitive(v);
        assert_eq!(r["api_key"], "[REDACTED]");
        assert_eq!(r["name"], "test");
    }

    #[test]
    fn redact_token_substring() {
        let v = json!({ "access_token": "abc123", "data": "ok" });
        let r = redact_sensitive(v);
        assert_eq!(r["access_token"], "[REDACTED]");
        assert_eq!(r["data"], "ok");
    }

    #[test]
    fn redact_nested_object() {
        let v = json!({ "meta": { "password": "hunter2", "user": "alice" } });
        let r = redact_sensitive(v);
        assert_eq!(r["meta"]["password"], "[REDACTED]");
        assert_eq!(r["meta"]["user"], "alice");
    }

    #[test]
    fn redact_array_of_objects() {
        let v = json!([{ "secret": "x", "ok": 1 }, { "ok": 2 }]);
        let r = redact_sensitive(v);
        let arr = r.as_array().unwrap();
        assert_eq!(arr[0]["secret"], "[REDACTED]");
        assert_eq!(arr[0]["ok"], 1);
        assert_eq!(arr[1]["ok"], 2);
    }

    #[test]
    fn redact_case_insensitive_key() {
        let v = json!({ "API_KEY": "should-be-redacted", "Title": "keep" });
        let r = redact_sensitive(v);
        assert_eq!(r["API_KEY"], "[REDACTED]");
        assert_eq!(r["Title"], "keep");
    }

    #[test]
    fn redact_credential_pattern() {
        let v = json!({ "db_credential": "pw", "id": 1 });
        let r = redact_sensitive(v);
        assert_eq!(r["db_credential"], "[REDACTED]");
        assert_eq!(r["id"], 1);
    }

    #[test]
    fn non_sensitive_keys_unchanged() {
        assert!(!is_sensitive_key("username"));
        assert!(!is_sensitive_key("output"));
        assert!(!is_sensitive_key("node_count"));
        assert!(is_sensitive_key("my_api_key_value"));
        assert!(is_sensitive_key("JWT_TOKEN"));
        assert!(is_sensitive_key("user_password"));
    }
}
