//! `lightarchitects_write` — create or overwrite a file atomically.

use serde_json::{Value, json};

use crate::config::expand_tilde;
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
pub fn run(params: Value) -> Result<Value, GatewayError> {
    let path_str = params["path"]
        .as_str()
        .ok_or(GatewayError::MissingParam("path"))?;
    let content = params["content"]
        .as_str()
        .ok_or(GatewayError::MissingParam("content"))?;

    let path = expand_tilde(path_str);

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                GatewayError::File(format!("create dirs {}: {e}", parent.display()))
            })?;
        }
    }

    let bytes = content.len();
    std::fs::write(&path, content)
        .map_err(|e| GatewayError::File(format!("{}: {e}", path.display())))?;

    let result = json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&json!({
                "path": path.display().to_string(),
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

    #[test]
    fn writes_new_file() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("out.txt");
        let result = run(json!({
            "path": path.to_str().unwrap(),
            "content": "hello world"
        }))
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
        run(json!({
            "path": path.to_str().unwrap(),
            "content": "nested"
        }))
        .expect("run");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "nested");
    }

    #[test]
    fn missing_content_returns_error() {
        let result = run(json!({"path": "/tmp/x.txt"}));
        assert!(result.is_err());
    }
}
