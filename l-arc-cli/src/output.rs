//! Output formatting — JSON vs human-readable.

use serde_json::Value;

/// Output mode selected by the `--json` flag.
#[derive(Debug, Clone, Copy, Default)]
pub enum OutputMode {
    /// Human-readable text (default).
    #[default]
    Human,
    /// Machine-readable JSON.
    Json,
}

/// Print a text response in the selected output mode.
///
/// - Human: prints `text` directly, followed by a newline.
/// - JSON: wraps `text` in `{"output": "..."}` and pretty-prints.
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
/// Both modes output pretty-printed JSON — structured data is always
/// represented as JSON regardless of output mode.
pub fn print_value(_mode: OutputMode, value: &Value) {
    println!("{}", pretty(value));
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

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}
