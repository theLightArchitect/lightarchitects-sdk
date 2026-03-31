//! `lightarchitects_import` — import content from external systems.
//!
//! Adapters:
//! - `obsidian` / `markdown`: scan a directory tree for `.md` files and
//!   extract H1 titles from frontmatter.
//! - `mcp`: generate a `[agents.<name>]` TOML block for adding a custom agent.

use std::fmt::Write as _;
use std::path::Path;

use serde_json::{Value, json};

use crate::config::expand_tilde;
use crate::error::GatewayError;

/// Execute `lightarchitects_import`.
///
/// # Parameters (JSON object)
/// - `adapter` (string, required): `"obsidian"` | `"markdown"` | `"mcp"`.
/// - `path` (string, required for `obsidian`/`markdown`): directory to scan.
/// - `name` (string, required for `mcp`): new route name.
/// - `binary` (string, optional for `mcp`): binary path.
/// - `tool_name` (string, optional for `mcp`): MCP tool name.
/// - `role` (string, optional for `mcp`): human-readable description.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] when a required parameter is absent.
/// Returns [`GatewayError::Subprocess`] for unknown adapter names.
pub fn run(params: Value, _config: &crate::config::GatewayConfig) -> Result<Value, GatewayError> {
    let adapter = params["adapter"]
        .as_str()
        .ok_or(GatewayError::MissingParam("adapter"))?;

    match adapter {
        "obsidian" | "markdown" => scan_markdown(&params),
        "mcp" => generate_mcp_block(&params),
        _ => Err(GatewayError::Subprocess(format!(
            "unknown adapter '{adapter}'. Valid adapters: obsidian, markdown, mcp"
        ))),
    }
}

/// Scan a directory tree for `.md` files and return count + title list.
fn scan_markdown(params: &Value) -> Result<Value, GatewayError> {
    let path_str = params["path"]
        .as_str()
        .ok_or(GatewayError::MissingParam("path"))?;
    let root = expand_tilde(path_str);

    let mut count = 0usize;
    let mut titles: Vec<String> = Vec::new();
    scan_dir_md(&root, &mut count, &mut titles)?;

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!(
                "Found {count} markdown files in {}\n\nTitles:\n{}",
                root.display(),
                titles.join("\n")
            )
        }],
        "file_count": count,
        "titles": titles,
    }))
}

/// Recursively scan `dir` for `.md` files, collecting count and titles.
fn scan_dir_md(
    dir: &Path,
    count: &mut usize,
    titles: &mut Vec<String>,
) -> Result<(), GatewayError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| GatewayError::File(format!("{}: {e}", dir.display())))?;

    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            scan_dir_md(&path, count, titles)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            *count += 1;
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .to_owned();
            let title = extract_md_title(&path).unwrap_or(stem);
            titles.push(title);
        }
    }
    Ok(())
}

/// Extract the first H1 (`# …`) from a markdown file.
fn extract_md_title(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    content
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches('#').trim().to_owned())
}

/// Generate a `[agents.<name>]` TOML block for a custom agent.
fn generate_mcp_block(params: &Value) -> Result<Value, GatewayError> {
    let name = params["name"]
        .as_str()
        .ok_or(GatewayError::MissingParam("name"))?;
    let binary = params["binary"].as_str().unwrap_or("");
    let tool_name = params["tool_name"].as_str().unwrap_or("");
    let role = params["role"].as_str().unwrap_or("");

    let mut block = String::new();
    let _ = writeln!(block, "[agents.{name}]");
    let _ = writeln!(block, "enabled = true");
    let _ = writeln!(block, "binary = \"{binary}\"");
    let _ = writeln!(block, "tool_name = \"{tool_name}\"");
    let _ = writeln!(block, "role = \"{role}\"");
    let _ = writeln!(block, "trust = \"trusted\"");
    let _ = writeln!(block, "scope = \"own\"");

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!("Add this block to ~/.lightarchitects/config.toml:\n\n{block}")
        }],
        "toml_block": block,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GatewayConfig;
    use serde_json::json;
    use std::io::Write as _;

    #[test]
    fn missing_adapter_is_error() {
        assert!(run(json!({}), &GatewayConfig::default()).is_err());
    }

    #[test]
    fn unknown_adapter_is_error() {
        assert!(run(json!({"adapter": "unknown"}), &GatewayConfig::default()).is_err());
    }

    #[test]
    fn mcp_missing_name_is_error() {
        assert!(run(json!({"adapter": "mcp"}), &GatewayConfig::default()).is_err());
    }

    #[test]
    fn mcp_generates_toml_block() {
        let result = run(
            json!({"adapter": "mcp", "name": "mybot", "binary": "~/.mybot/bin/bot", "tool_name": "botTools"}),
            &GatewayConfig::default(),
        )
        .expect("run");
        let block = result["toml_block"].as_str().unwrap();
        assert!(block.contains("[agents.mybot]"));
        assert!(block.contains("enabled = true"));
        assert!(block.contains("botTools"));
    }

    #[test]
    fn markdown_scan_counts_files_and_extracts_titles() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut f1 = std::fs::File::create(dir.path().join("a.md")).expect("f1");
        writeln!(f1, "# Title A").expect("write");
        let mut f2 = std::fs::File::create(dir.path().join("b.md")).expect("f2");
        writeln!(f2, "# Title B").expect("write");
        // Non-markdown file — should be ignored.
        std::fs::File::create(dir.path().join("note.txt")).expect("txt");

        let result = run(
            json!({"adapter": "markdown", "path": dir.path().to_str().unwrap()}),
            &GatewayConfig::default(),
        )
        .expect("run");

        assert_eq!(result["file_count"].as_u64().unwrap(), 2);
        let titles = result["titles"].as_array().unwrap();
        assert!(titles.iter().any(|t| t.as_str() == Some("Title A")));
        assert!(titles.iter().any(|t| t.as_str() == Some("Title B")));
    }
}
