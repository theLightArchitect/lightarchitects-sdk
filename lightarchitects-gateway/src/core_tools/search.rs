//! `lightarchitects_search` — search file contents using `rg` (ripgrep).

use serde_json::{Value, json};
use tokio::process::Command;

use crate::config::GatewayConfig;
use crate::core_tools::security;
use crate::error::GatewayError;

/// Execute `lightarchitects_search`.
///
/// Delegates to `rg` (ripgrep) if available; falls back to `grep -rn` otherwise.
///
/// # Parameters (JSON object)
/// - `pattern` (string, required): search pattern (regex).
/// - `path` (string, optional): directory or file to search (default: current working dir).
/// - `glob` (string, optional): file-glob filter, e.g. `"*.rs"`.
/// - `case_insensitive` (bool, optional, default `false`).
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] when `pattern` is absent, and
/// [`GatewayError::Subprocess`] when the search process cannot be spawned.
pub async fn run(params: Value, config: &GatewayConfig) -> Result<Value, GatewayError> {
    let pattern = params["pattern"]
        .as_str()
        .ok_or(GatewayError::MissingParam("pattern"))?;

    // Security: validate the search path if provided.
    let search_path = if let Some(p) = params["path"].as_str() {
        Some(security::validate_path(p, config)?)
    } else {
        None
    };

    let glob_filter = params["glob"].as_str();
    let case_insensitive = params["case_insensitive"].as_bool().unwrap_or(false);

    let output = if rg_available().await {
        run_rg(
            pattern,
            search_path.as_deref(),
            glob_filter,
            case_insensitive,
        )
        .await?
    } else {
        run_grep(
            pattern,
            search_path.as_deref(),
            glob_filter,
            case_insensitive,
        )
        .await?
    };

    Ok(json!({
        "content": [{"type": "text", "text": output}]
    }))
}

/// Returns `true` if `rg` is on `$PATH`.
async fn rg_available() -> bool {
    Command::new("rg")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run ripgrep and return stdout as a string.
async fn run_rg(
    pattern: &str,
    path: Option<&std::path::Path>,
    glob: Option<&str>,
    case_insensitive: bool,
) -> Result<String, GatewayError> {
    let mut cmd = Command::new("rg");
    cmd.arg("--line-number");
    // Cap output to prevent unbounded result sets from large repos.
    cmd.arg("--max-count").arg("1000");
    if case_insensitive {
        cmd.arg("--ignore-case");
    }
    if let Some(g) = glob {
        cmd.arg("--glob").arg(g);
    }
    cmd.arg(pattern);
    if let Some(p) = path {
        cmd.arg(p);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let out = cmd
        .output()
        .await
        .map_err(|e| GatewayError::Subprocess(format!("rg failed: {e}")))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Fallback: GNU/BSD grep with `-rn`.
///
/// Supports glob filtering via `--include` (supported on both macOS BSD grep
/// and GNU grep). Glob patterns like `*.toml` are passed directly; `rg`-style
/// negation patterns (`!...`) are silently ignored since grep has no equivalent.
async fn run_grep(
    pattern: &str,
    path: Option<&std::path::Path>,
    glob: Option<&str>,
    case_insensitive: bool,
) -> Result<String, GatewayError> {
    let mut cmd = Command::new("grep");
    cmd.arg("-rn");
    if case_insensitive {
        cmd.arg("-i");
    }
    // Apply file-glob filter when present and not a negation pattern.
    if let Some(g) = glob {
        if !g.starts_with('!') {
            cmd.arg(format!("--include={g}"));
        }
    }
    cmd.arg(pattern);
    if let Some(p) = path {
        cmd.arg(p);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let out = cmd
        .output()
        .await
        .map_err(|e| GatewayError::Subprocess(format!("grep failed: {e}")))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_config() -> GatewayConfig {
        GatewayConfig::default()
    }

    #[tokio::test]
    async fn missing_pattern_is_error() {
        let cfg = test_config();
        let result = run(json!({}), &cfg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn search_returns_text_content() {
        let cfg = test_config();
        // Search for a pattern that almost certainly exists on any dev machine.
        let result = run(
            json!({
                "pattern": "Cargo",
                "path": "/Users/kft/Projects/lightarchitects-sdk",
                "glob": "*.toml"
            }),
            &cfg,
        )
        .await
        .expect("run");
        assert!(result["content"][0]["type"].as_str() == Some("text"));
    }
}
