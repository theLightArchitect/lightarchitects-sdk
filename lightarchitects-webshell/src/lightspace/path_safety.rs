//! Path validation for Lightspace session directories (CWE-22).
//!
//! All session artefacts (NDJSON logs, snapshots) are written under
//! `~/.lightarchitects/lightspace/<session_id>/`.  [`safe_lightspace_path`]
//! validates that a constructed path stays within this root, preventing
//! path-traversal attacks from poisoned session IDs or filenames.
//!
//! Uses the ancestor-walk canonicalization pattern from Cookbook §63.P5:
//! `std::fs::canonicalize` fails on non-existent paths, so we walk to the
//! nearest existing ancestor before resolving and confirming containment.

use std::path::{Component, Path, PathBuf};

/// Error returned when a path escapes the Lightspace root.
#[derive(Debug)]
pub struct PathEscapeError(pub String);

impl std::fmt::Display for PathEscapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "path escape: {}", self.0)
    }
}

/// Resolve `root/<subpath>` and verify it stays inside `root`.
///
/// Creates `root` if it does not yet exist.  Returns the absolute, normalised
/// path on success, or [`PathEscapeError`] if the resolved path escapes.
///
/// # Errors
///
/// Returns [`PathEscapeError`] when:
/// - `subpath` contains `..` components that would escape `root`
/// - Filesystem operations fail (I/O error is wrapped in the message)
pub fn safe_lightspace_path(root: &Path, subpath: &Path) -> Result<PathBuf, PathEscapeError> {
    // Reject obvious traversal components before touching the filesystem.
    for component in subpath.components() {
        if matches!(component, Component::ParentDir) {
            return Err(PathEscapeError(format!(
                "subpath '{}' contains '..' component",
                subpath.display()
            )));
        }
    }

    let candidate = root.join(subpath);

    // Ensure the root exists so ancestor-walk canonicalization can resolve it.
    std::fs::create_dir_all(root)
        .map_err(|e| PathEscapeError(format!("cannot create root '{}': {e}", root.display())))?;

    // Walk to nearest existing ancestor — handles partially-created dirs.
    let canon_root = ancestor_canonicalize(root)?;
    let canon_candidate = ancestor_canonicalize(&candidate)?;

    if canon_candidate.starts_with(&canon_root) {
        Ok(canon_candidate)
    } else {
        Err(PathEscapeError(format!(
            "'{}' escapes root '{}'",
            canon_candidate.display(),
            canon_root.display()
        )))
    }
}

/// Canonicalize by walking up to the nearest existing ancestor.
fn ancestor_canonicalize(path: &Path) -> Result<PathBuf, PathEscapeError> {
    let mut current = path.to_path_buf();
    loop {
        if let Ok(canon) = std::fs::canonicalize(&current) {
            // Re-append the suffix that was stripped during the walk.
            let suffix = path
                .strip_prefix(&current)
                .unwrap_or_else(|_| Path::new(""));
            return Ok(canon.join(suffix));
        }
        let parent = current.parent().ok_or_else(|| {
            PathEscapeError(format!("no existing ancestor for '{}'", path.display()))
        })?;
        current = parent.to_path_buf();
    }
}

/// Build the per-session directory path: `~/.lightarchitects/lightspace/<id>/`.
///
/// # Errors
///
/// Returns [`PathEscapeError`] when `HOME` is unset or the session UUID
/// produces a path that escapes the expected prefix.
pub fn session_dir(session_id: uuid::Uuid) -> Result<PathBuf, PathEscapeError> {
    let home = std::env::var("HOME").map_err(|_| PathEscapeError("HOME env var unset".into()))?;
    let root = PathBuf::from(home)
        .join(".lightarchitects")
        .join("lightspace");
    // session_id.to_string() is a UUID hex string — no path components.
    let sub = PathBuf::from(session_id.to_string());
    safe_lightspace_path(&root, &sub)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_component_rejected() {
        let root = std::env::temp_dir().join("ls-path-test");
        let bad = Path::new("../etc/passwd");
        assert!(safe_lightspace_path(&root, bad).is_err());
    }
}
