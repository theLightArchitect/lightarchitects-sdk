//! `lightarchitects_read` — read file contents with optional line-range selection.

use std::fmt::Write as _;

use serde_json::Value;

use crate::config::GatewayConfig;
use crate::core_tools::security;
use crate::core_tools::text_result;
use crate::error::GatewayError;

/// Maximum file size for read operations (10 MiB).
const MAX_READ_SIZE: u64 = security::MAX_READ_SIZE;

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
pub fn run(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let path_str = params["path"]
        .as_str()
        .ok_or(GatewayError::MissingParam("path"))?;

    // Security: validate path boundaries before any I/O.
    let canonical = security::validate_path(path_str, config)?;

    // Security: enforce file size limit before reading.
    let metadata = std::fs::metadata(&canonical)
        .map_err(|e| GatewayError::File(format!("{}: {e}", canonical.display())))?;
    if metadata.len() > MAX_READ_SIZE {
        return Err(GatewayError::File(format!(
            "file too large: {} bytes (max {MAX_READ_SIZE})",
            metadata.len()
        )));
    }

    let offset = params["offset"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok());
    let limit = params["limit"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok());

    let content = std::fs::read_to_string(&canonical)
        .map_err(|e| GatewayError::File(format!("{}: {e}", canonical.display())))?;

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

    fn test_config() -> GatewayConfig {
        GatewayConfig::default()
    }

    #[test]
    fn reads_all_lines() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "line1\nline2\nline3").expect("write");
        let cfg = test_config();
        let result = run(json!({"path": tmp.path().to_str().unwrap()}), &cfg).expect("run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("line1"));
        assert!(text.contains("line2"));
        assert!(text.contains("line3"));
    }

    #[test]
    fn respects_offset_and_limit() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        writeln!(tmp, "a\nb\nc\nd\ne").expect("write");
        let cfg = test_config();
        let result = run(
            json!({"path": tmp.path().to_str().unwrap(), "offset": 2, "limit": 2}),
            &cfg,
        )
        .expect("run");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains('b'));
        assert!(text.contains('c'));
        assert!(!text.contains('a'));
        assert!(!text.contains('d'));
    }

    #[test]
    fn missing_path_returns_error() {
        let cfg = test_config();
        let result = run(json!({}), &cfg);
        assert!(result.is_err());
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_file_size_limit_enforced() {
        // Create a file larger than MAX_READ_SIZE.
        let dir = tempfile::tempdir().expect("tempdir");
        let big_file = dir.path().join("big.bin");
        // Write 10MB + 1 byte.
        let data = vec![0u8; (MAX_READ_SIZE as usize) + 1];
        std::fs::write(&big_file, &data).expect("write big file");

        let cfg = test_config();
        let result = run(json!({"path": big_file.to_str().unwrap()}), &cfg);
        assert!(result.is_err(), "should reject files > 10MB");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("file too large"),
            "error should mention size limit, got: {err}"
        );
    }
}
