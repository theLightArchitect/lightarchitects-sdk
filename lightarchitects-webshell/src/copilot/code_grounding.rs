//! Live codebase search grounding for the copilot.
//!
//! When a question references specific Rust or TypeScript/Svelte symbols, this
//! module greps the project source tree for their definition sites and injects
//! the surrounding code into the copilot prelude.  This compensates for
//! recently-shipped code that has not yet been enriched into the SOUL helix.
//!
//! Security: the code block is wrapped in a per-request nonce identical to the
//! pattern used by `soul_grounding::format_block` (OWASP LLM02 SCR13).

use std::path::Path;

/// Hard timeout for the full code search pass.
const TIMEOUT_MS: u64 = 500;
/// Lines of context around each match (`rg -C N`).
const CONTEXT_LINES: &str = "4";
/// Maximum bytes to include per snippet before truncation.
const SNIPPET_BYTES: usize = 600;
/// Maximum number of distinct match groups to inject.
const MAX_SNIPPETS: usize = 5;
/// Maximum symbols to search for — bounds wall-clock cost.
const MAX_SYMBOLS: usize = 4;
/// Minimum identifier length to consider. Filters "is", "fn", "use", etc.
const MIN_SYM_LEN: usize = 5;
/// Maximum `rg` matches per file (prevents one hot file dominating).
const MAX_MATCHES_PER_FILE: &str = "2";

/// Tokens to skip even when they pass the length check.
static STOPWORDS: &[&str] = &[
    "what", "when", "where", "does", "this", "have", "with", "that", "from", "into", "impl",
    "trait", "struct", "function", "method", "value", "returns", "called", "added", "guard",
    "pattern", "added", "guard", "pattern", "first", "chunk", "every", "single", "which", "write",
    "scope", "about", "between", "after", "before", "during", "across", "without", "string",
    "option", "result", "error", "false", "true", "async", "await", "token", "block", "build",
    "phase", "local", "there", "their", "these", "those", "other", "then", "also", "both", "only",
    "just", "most", "some", "such", "when", "each", "more", "well", "over", "under", "used",
    "check", "event", "start", "state", "store", "entry", "index", "match", "place", "model",
    "field", "order", "label", "class", "level", "count", "items", "files", "lines", "bytes",
    "limit", "range", "point", "scope", "route", "layer",
];

/// Extract candidate symbol names from a natural-language message.
///
/// Targets:
/// - `CamelCase` identifiers (struct/type names) — e.g. `StrategyRegistry`
/// - `snake_case` with two-or-more underscores (fn names) — e.g. `emit_turn_start_span`
/// - `Foo::bar` path segments — both halves
fn extract_symbols(message: &str) -> Vec<String> {
    use regex::Regex;
    use std::sync::OnceLock;

    static RE_CAMEL: OnceLock<Regex> = OnceLock::new();
    static RE_SNAKE: OnceLock<Regex> = OnceLock::new();

    #[allow(clippy::expect_used)] // static regex literals cannot fail to compile
    let re_camel =
        RE_CAMEL.get_or_init(|| Regex::new(r"\b([A-Z][a-zA-Z0-9]{4,})\b").expect("static"));
    #[allow(clippy::expect_used)]
    let re_snake = RE_SNAKE.get_or_init(|| {
        // snake_case with at least two underscore segments: fn_name_part
        Regex::new(r"\b([a-z][a-z0-9]*(?:_[a-z0-9]+){2,})\b").expect("static")
    });

    let mut syms: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for cap in re_camel.captures_iter(message) {
        let s = cap[1].to_owned();
        if s.len() >= MIN_SYM_LEN
            && !STOPWORDS.contains(&s.to_lowercase().as_str())
            && seen.insert(s.clone())
        {
            syms.push(s);
        }
    }

    for cap in re_snake.captures_iter(message) {
        let s = cap[1].to_owned();
        if s.len() >= MIN_SYM_LEN && !STOPWORDS.contains(&s.as_str()) && seen.insert(s.clone()) {
            syms.push(s);
        }
    }

    syms.truncate(MAX_SYMBOLS);
    syms
}

/// Truncate raw `rg` output to at most `max_bytes`, breaking on a newline.
fn truncate_snippet(s: &str, max_bytes: usize) -> String {
    let trimmed = s.trim_end();
    if trimmed.len() <= max_bytes {
        return trimmed.to_owned();
    }
    let mut end = max_bytes;
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    // Prefer breaking at a newline so partial lines are not injected.
    if let Some(nl) = trimmed[..end].rfind('\n') {
        trimmed[..nl].trim_end().to_owned()
    } else {
        format!("{}…", &trimmed[..end])
    }
}

/// Search the source tree rooted at `src_root` for symbols inferred from `message`.
///
/// Runs `rg` in a `spawn_blocking` task with a hard [`TIMEOUT_MS`] wall-clock cap.
/// Returns `None` when: `rg` is absent, no symbols were found, or the search
/// produces no results.  Never panics.
///
/// The returned string is a `[CODE-REF::{nonce}]…[/CODE-REF::{nonce}]` block
/// suitable for injection into the copilot system-prompt prelude.
pub async fn search_code(src_root: &Path, message: &str) -> Option<String> {
    let symbols = extract_symbols(message);
    if symbols.is_empty() {
        return None;
    }

    let root = src_root.to_path_buf();
    let search = tokio::task::spawn_blocking(move || {
        let mut snippets: Vec<String> = Vec::new();

        'sym: for sym in &symbols {
            if snippets.len() >= MAX_SNIPPETS {
                break;
            }
            // Word-boundary match so `Strategy` doesn't match `StrategyRunner`.
            let pattern = format!(r"\b{sym}\b");

            let out = std::process::Command::new("rg")
                .args([
                    "--smart-case",
                    "-n",
                    "-C",
                    CONTEXT_LINES,
                    "--max-count",
                    MAX_MATCHES_PER_FILE,
                    "--max-filesize=200K",
                    "--no-heading",
                    // Include Rust and common web-frontend extensions.
                    "--type=rust",
                    "--type-add=svelte:*.svelte",
                    "--type=svelte",
                    // TypeScript via the built-in `ts` type.
                    "--type=ts",
                    &pattern,
                ])
                .current_dir(&root)
                .output();

            let raw = match out {
                Ok(o) if !o.stdout.is_empty() => String::from_utf8_lossy(&o.stdout).into_owned(),
                _ => continue 'sym,
            };

            // Collect contiguous match groups (separated by `--` in rg -C output).
            let mut group = String::new();
            for line in raw.lines() {
                if line == "--" {
                    if !group.is_empty() {
                        let snippet = truncate_snippet(&group, SNIPPET_BYTES);
                        if !snippet.is_empty() {
                            snippets.push(snippet);
                            group = String::new();
                            if snippets.len() >= MAX_SNIPPETS {
                                break 'sym;
                            }
                        }
                    }
                } else {
                    group.push_str(line);
                    group.push('\n');
                }
            }
            // Flush the last group.
            if !group.is_empty() && snippets.len() < MAX_SNIPPETS {
                let snippet = truncate_snippet(&group, SNIPPET_BYTES);
                if !snippet.is_empty() {
                    snippets.push(snippet);
                }
            }
        }
        snippets
    });

    let Ok(Ok(snippets)) =
        tokio::time::timeout(std::time::Duration::from_millis(TIMEOUT_MS), search).await
    else {
        return None;
    };

    if snippets.is_empty() {
        return None;
    }

    // Wrap in a nonce-prefixed block identical in style to soul_grounding blocks.
    let nonce = super::soul_grounding::vault_nonce();
    let mut block = format!("[CODE-REF::{nonce}]\n");
    for s in &snippets {
        block.push_str(s);
        if !s.ends_with('\n') {
            block.push('\n');
        }
        block.push_str("---\n");
    }
    #[allow(clippy::format_push_string)]
    block.push_str(&format!("[/CODE-REF::{nonce}]\n"));
    Some(block)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_symbols_camel_case() {
        let syms = extract_symbols("How does StrategyRegistry::lookup work?");
        assert!(syms.contains(&"StrategyRegistry".to_owned()), "{syms:?}");
    }

    #[test]
    fn extract_symbols_snake_case() {
        let syms = extract_symbols("What is emit_turn_start_span and when is it called?");
        assert!(
            syms.contains(&"emit_turn_start_span".to_owned()),
            "{syms:?}"
        );
    }

    #[test]
    fn extract_symbols_filters_stopwords() {
        let syms = extract_symbols("What does the function return");
        // "function" and "return" are in stopwords or too short; should be empty/minimal.
        assert!(!syms.contains(&"function".to_owned()));
    }

    #[test]
    fn truncate_snippet_prefers_newline() {
        let s = "line1\nline2\nline3";
        // 12 bytes = "line1\nline2\n"; last \n is at index 11 → cut gives "line1\nline2"
        let truncated = truncate_snippet(s, 12);
        assert_eq!(truncated, "line1\nline2");
    }

    #[test]
    fn truncate_snippet_short_passthrough() {
        let s = "hello";
        assert_eq!(truncate_snippet(s, 100), "hello");
    }
}
