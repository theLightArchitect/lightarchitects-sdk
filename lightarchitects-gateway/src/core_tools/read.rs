//! `lightarchitects_read` — read file contents with optional line-range selection.

use std::fmt::Write as _;

use serde_json::Value;

use crate::config::expand_tilde;
use crate::core_tools::text_result;
use crate::error::GatewayError;

/// Execute `lightarchitects_read`.
///
/// # Parameters (JSON object)
/// - `path` (string, required): file path, `~/` prefix is expanded.
/// - `offset` (integer, optional): 1-indexed first line to return.
/// - `limit` (integer, optional): maximum number of lines to return.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] when `path` is absent, and
/// [`GatewayError::File`] when the file cannot be read.
pub fn run(params: Value) -> Result<Value, GatewayError> {
    let path_str = params["path"]
        .as_str()
        .ok_or(GatewayError::MissingParam("path"))?;
    let path = expand_tilde(path_str);

    let offset = params["offset"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok());
    let limit = params["limit"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok());

    let content = std::fs::read_to_string(&path)
        .map_err(|e| GatewayError::File(format!("{}: {e}", path.display())))?;

    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();

    // Convert 1-indexed offset to 0-indexed start, default to beginning.
    let start = offset.map_or(0, |o| o.saturating_sub(1).min(total));
    let end = limit.map_or(total, |l| (start + l).min(total));

    let mut output = String::new();
    for (i, line) in lines[start..end].iter().enumerate() {
        let line_num = start + i + 1;
        let _ = writeln!(output, "{line_num:>6}\t{line}");
    }

    Ok(text_result(output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write as _;

    #[test]
    fn reads_all_lines() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "line1\nline2\nline3").expect("write");
        let result = run(json!({"path": tmp.path().to_str().unwrap()})).expect("run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("line1"));
        assert!(text.contains("line2"));
        assert!(text.contains("line3"));
    }

    #[test]
    fn respects_offset_and_limit() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "a\nb\nc\nd\ne").expect("write");
        let result = run(json!({"path": tmp.path().to_str().unwrap(), "offset": 2, "limit": 2}))
            .expect("run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains('b'));
        assert!(text.contains('c'));
        assert!(!text.contains('a'));
        assert!(!text.contains('d'));
    }

    #[test]
    fn missing_path_returns_error() {
        let result = run(json!({}));
        assert!(result.is_err());
    }
}
