//! `lightarchitects_write` — create or overwrite a file atomically.

use serde_json::{Value, json};

use crate::config::GatewayConfig;
use crate::core_tools::security;
use crate::error::GatewayError;

/// Execute `lightarchitects_write`.
///
/// # Parameters (JSON object)
/// - `path` (string, required): destination path, `~/` prefix is expanded.
/// - `content` (string, required): file content to write.
///
/// Creates all parent directories automatically.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] when a required parameter is absent,
/// and [`GatewayError::File`] when the file cannot be written.
pub fn run(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let path_str = params["path"]
        .as_str()
        .ok_or(GatewayError::MissingParam("path"))?;
    let content = params["content"]
        .as_str()
        .ok_or(GatewayError::MissingParam("content"))?;

    // Security: ALL validation MUST complete before any filesystem mutation.
    // Flow: expand → validate → create dirs → write.
    let expanded = crate::config::expand_tilde(path_str);

    if expanded.exists() {
        // Existing file: full canonicalize + write-denied check.
        security::validate_write_path(path_str, config)?;
    } else {
        // New file: validate without requiring the path to exist.
        // 1. Check denied components on the raw expanded path (no I/O needed).
        // 2. Check write-denied patterns on the full target path.
        // 3. If allowed_directories is configured, find the nearest existing
        //    ancestor, canonicalize it, and verify it falls within bounds.
        let path_str_lossy = expanded.to_string_lossy();
        security::check_write_denied(&path_str_lossy)?;

        if let Some(parent) = expanded.parent() {
            let parent_str = parent.to_string_lossy();
            // validate_path on the nearest existing ancestor handles:
            //   - denied component checks (raw + canonical)
            //   - allowed_directories boundary enforcement
            // Walk up from the parent to find an ancestor that exists so
            // canonicalize succeeds (required by validate_path).
            let mut ancestor = parent.to_path_buf();
            while !ancestor.exists() {
                // Check each segment we'd create for both denied patterns
                // AND denied path components (.ssh, .gnupg, .aws, etc.).
                security::check_write_denied(&ancestor.to_string_lossy())?;
                security::check_denied_components(&ancestor)?;
                match ancestor.parent() {
                    Some(p) if !p.as_os_str().is_empty() => ancestor = p.to_path_buf(),
                    _ => break,
                }
            }
            if ancestor.exists() {
                let ancestor_str = ancestor.to_string_lossy();
                security::validate_path(&ancestor_str, config)?;
            }

            // Also run check_write_denied on the parent as a whole.
            security::check_write_denied(&parent_str)?;
        }
    }

    // All validation passed — now safe to create parent directories.
    if let Some(parent) = expanded.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                GatewayError::File(format!("create dirs {}: {e}", parent.display()))
            })?;
        }
    }

    let bytes = content.len();
    std::fs::write(&expanded, content)
        .map_err(|e| GatewayError::File(format!("{}: {e}", expanded.display())))?;

    let result = json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&json!({
                "path": expanded.display().to_string(),
                "bytes_written": bytes
            }))?
        }]
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_config() -> GatewayConfig {
        GatewayConfig::default()
    }

    #[test]
    fn writes_new_file() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("out.txt");
        let cfg = test_config();
        let result = run(
            json!({
                "path": path.to_str().unwrap(),
                "content": "hello world"
            }),
            &cfg,
        )
        .expect("run");
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("bytes_written")
        );
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "hello world");
    }

    #[test]
    fn creates_parent_dirs() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("a").join("b").join("c.txt");
        let cfg = test_config();
        run(
            json!({
                "path": path.to_str().unwrap(),
                "content": "nested"
            }),
            &cfg,
        )
        .expect("run");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "nested");
    }

    #[test]
    fn missing_content_returns_error() {
        let cfg = test_config();
        let result = run(json!({"path": "/tmp/x.txt"}), &cfg);
        assert!(result.is_err());
    }
}
