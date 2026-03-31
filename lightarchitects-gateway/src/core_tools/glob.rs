//! `lightarchitects_glob` — find files matching a glob pattern.

use serde_json::{Value, json};
use tokio::process::Command;

use crate::config::GatewayConfig;
use crate::core_tools::security;
use crate::error::GatewayError;

/// Execute `lightarchitects_glob`.
///
/// Uses the system `find` command to locate files matching the pattern.
///
/// # Parameters (JSON object)
/// - `pattern` (string, required): glob pattern, e.g. `"**/*.rs"` or `"*.toml"`.
/// - `path` (string, optional): base directory to search (default: current dir).
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] when `pattern` is absent, and
/// [`GatewayError::Subprocess`] when `find` cannot be spawned.
pub async fn run(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let pattern = params["pattern"]
        .as_str()
        .ok_or(GatewayError::MissingParam("pattern"))?;

    // Security: validate the base path if provided.
    let base = if let Some(p) = params["path"].as_str() {
        let canonical = security::validate_path(p, config)?;
        canonical.display().to_string()
    } else {
        ".".to_owned()
    };

    // Extract a bare filename pattern from glob syntax, e.g. "**/*.rs" → "*.rs".
    let name_pattern = extract_name_pattern(pattern);

    let mut cmd = Command::new("find");
    cmd.arg(&base);
    // Limit traversal depth to prevent runaway scans on large trees.
    cmd.arg("-maxdepth").arg("20");
    cmd.arg("-name").arg(name_pattern);
    // Exclude hidden dirs and common build dirs for ergonomics.
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output = cmd
        .output()
        .await
        .map_err(|e| GatewayError::Subprocess(format!("find failed: {e}")))?;

    let paths = String::from_utf8_lossy(&output.stdout).into_owned();
    let list: Vec<&str> = paths.lines().filter(|l| !l.is_empty()).collect();

    let text = serde_json::to_string(&list)?;
    Ok(json!({
        "content": [{"type": "text", "text": text}]
    }))
}

/// Extract the filename-level glob from a path-glob like `"**/*.rs"` or `"src/*.ts"`.
///
/// Returns the trailing component after the last `/`, or the whole string if
/// no `/` is present.
fn extract_name_pattern(pattern: &str) -> &str {
    pattern.rsplit_once('/').map_or(pattern, |(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_config() -> GatewayConfig {
        GatewayConfig::default()
    }

    #[test]
    fn extract_name_pattern_strips_prefix() {
        assert_eq!(extract_name_pattern("**/*.rs"), "*.rs");
        assert_eq!(extract_name_pattern("src/lib/*.ts"), "*.ts");
        assert_eq!(extract_name_pattern("*.toml"), "*.toml");
    }

    #[tokio::test]
    async fn missing_pattern_is_error() {
        let cfg = test_config();
        let result = run(json!({}), &cfg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn finds_toml_files_in_workspace() {
        let cfg = test_config();
        let result = run(
            json!({
                "pattern": "*.toml",
                "path": "/Users/kft/Projects/lightarchitects-sdk"
            }),
            &cfg,
        )
        .await
        .expect("run");
        let text = result["content"][0]["text"].as_str().unwrap();
        // Should find at least the root Cargo.toml.
        assert!(text.contains("Cargo.toml"));
    }
}
