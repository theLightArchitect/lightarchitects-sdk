//! `lightarchitects_edit` — replace an exact string in a file.

use serde_json::{Value, json};

use crate::config::expand_tilde;
use crate::error::GatewayError;

/// Execute `lightarchitects_edit`.
///
/// # Parameters (JSON object)
/// - `path` (string, required): file to edit, `~/` prefix is expanded.
/// - `old_string` (string, required): exact text to find and replace.
/// - `new_string` (string, required): replacement text.
/// - `replace_all` (bool, optional, default `false`): replace every occurrence;
///   when `false` the edit fails if `old_string` appears more than once.
///
/// # Errors
///
/// - [`GatewayError::MissingParam`] — a required parameter is absent.
/// - [`GatewayError::File`] — the file cannot be read or written.
/// - [`GatewayError::EditNotFound`] — `old_string` does not appear in the file.
/// - [`GatewayError::EditNotUnique`] — `old_string` matches more than once
///   and `replace_all` is `false`.
pub fn run(params: Value) -> Result<Value, GatewayError> {
    let path_str = params["path"]
        .as_str()
        .ok_or(GatewayError::MissingParam("path"))?;
    let old_string = params["old_string"]
        .as_str()
        .ok_or(GatewayError::MissingParam("old_string"))?;
    let new_string = params["new_string"]
        .as_str()
        .ok_or(GatewayError::MissingParam("new_string"))?;
    let replace_all = params["replace_all"].as_bool().unwrap_or(false);

    let path = expand_tilde(path_str);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| GatewayError::File(format!("{}: {e}", path.display())))?;

    let count = content.matches(old_string).count();
    if count == 0 {
        return Err(GatewayError::EditNotFound);
    }
    if count > 1 && !replace_all {
        return Err(GatewayError::EditNotUnique { count });
    }

    let new_content = if replace_all {
        content.replace(old_string, new_string)
    } else {
        // count == 1, safe to call replacen
        content.replacen(old_string, new_string, 1)
    };

    std::fs::write(&path, &new_content)
        .map_err(|e| GatewayError::File(format!("{}: {e}", path.display())))?;

    let replacements = if replace_all { count } else { 1 };
    let result = json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&json!({
                "path": path.display().to_string(),
                "replacements": replacements
            }))?
        }]
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write as _;

    #[test]
    fn replaces_single_occurrence() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tmp");
        write!(tmp, "foo bar foo").expect("write");
        // Only one occurrence expected — should fail with not-unique.
        // Use a string that appears exactly once:
        run(json!({
            "path": tmp.path().to_str().unwrap(),
            "old_string": "bar",
            "new_string": "baz"
        }))
        .expect("run");
        let content = std::fs::read_to_string(tmp.path()).expect("read");
        assert_eq!(content, "foo baz foo");
    }

    #[test]
    fn replace_all_flag_replaces_all() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tmp");
        write!(tmp, "a a a").expect("write");
        run(json!({
            "path": tmp.path().to_str().unwrap(),
            "old_string": "a",
            "new_string": "b",
            "replace_all": true
        }))
        .expect("run");
        let content = std::fs::read_to_string(tmp.path()).expect("read");
        assert_eq!(content, "b b b");
    }

    #[test]
    fn not_unique_without_replace_all_is_error() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tmp");
        write!(tmp, "x x").expect("write");
        let result = run(json!({
            "path": tmp.path().to_str().unwrap(),
            "old_string": "x",
            "new_string": "y"
        }));
        assert!(matches!(result, Err(GatewayError::EditNotUnique { .. })));
    }

    #[test]
    fn not_found_is_error() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tmp");
        write!(tmp, "hello").expect("write");
        let result = run(json!({
            "path": tmp.path().to_str().unwrap(),
            "old_string": "missing",
            "new_string": "replacement"
        }));
        assert!(matches!(result, Err(GatewayError::EditNotFound)));
    }
}
