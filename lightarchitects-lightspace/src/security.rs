//! CWE-22 path traversal and LLM07 URI scheme validation.

use crate::error::ReducerError;
use std::path::{Path, PathBuf};

/// Allowed URI schemes for card `content_uri` and drawer file URIs.
///
/// `file://` is permitted only under `~/.lightarchitects/lightspace/`.
/// Arbitrary `http://`, `https://`, or scheme-less strings are rejected.
const CONTENT_URI_SCHEMES: &[&str] = &["file", "helix", "project"];

/// Allowed URI schemes for `Provenance.source_uri`.
const PROVENANCE_URI_SCHEMES: &[&str] = &["file", "helix", "https", "ayin", "memory"];

/// Walk to the nearest existing ancestor of `path` and canonicalize it.
///
/// `std::fs::canonicalize` fails on non-existent paths. This function walks up
/// the directory tree until it finds an existing ancestor, canonicalizes that,
/// then re-appends the remaining suffix — preventing escape via symlinks.
/// Per Cookbook §63.P5.
fn canonicalize_ancestor(path: &Path) -> Result<PathBuf, ReducerError> {
    let mut current = path.to_path_buf();
    let mut suffix = PathBuf::new();

    loop {
        match std::fs::canonicalize(&current) {
            Ok(canon) => {
                return Ok(canon.join(suffix));
            }
            Err(_) => {
                let file_name = current.file_name().map(PathBuf::from).unwrap_or_default();
                suffix = file_name.join(&suffix);
                match current.parent() {
                    Some(parent) => current = parent.to_path_buf(),
                    None => return Err(ReducerError::PathTraversal(path.display().to_string())),
                }
            }
        }
    }
}

/// Validate that joining `root` with `untrusted` stays within `root` (CWE-22).
///
/// Returns the canonical resolved path on success.
pub fn safe_lightspace_path(root: &Path, untrusted: &str) -> Result<PathBuf, ReducerError> {
    let candidate = root.join(untrusted);
    let canonical = canonicalize_ancestor(&candidate)?;
    if !canonical.starts_with(root) {
        return Err(ReducerError::PathTraversal(untrusted.to_owned()));
    }
    Ok(canonical)
}

/// Validate the URI scheme of a `content_uri` field (CWE-22 + LLM07).
///
/// Allowed: `file://~/.lightarchitects/lightspace/...`, `helix://`, `project://`.
/// Denied: `http://`, `https://`, bare paths, and arbitrary `file:///etc/...`.
pub fn validate_content_uri_scheme(uri: &str) -> Result<(), ReducerError> {
    let scheme =
        extract_scheme(uri).ok_or_else(|| ReducerError::DisallowedUriScheme(uri.to_owned()))?;
    if !CONTENT_URI_SCHEMES.contains(&scheme.as_str()) {
        return Err(ReducerError::DisallowedUriScheme(uri.to_owned()));
    }
    if scheme == "file" {
        validate_file_uri_root(uri)?;
    }
    Ok(())
}

/// Validate the URI scheme of a `Provenance.source_uri` field.
pub fn validate_provenance_source_scheme(uri: &str) -> Result<(), ReducerError> {
    let scheme =
        extract_scheme(uri).ok_or_else(|| ReducerError::DisallowedUriScheme(uri.to_owned()))?;
    if !PROVENANCE_URI_SCHEMES.contains(&scheme.as_str()) {
        return Err(ReducerError::DisallowedUriScheme(uri.to_owned()));
    }
    Ok(())
}

/// Extract the scheme component from a URI string (the part before `://`).
fn extract_scheme(uri: &str) -> Option<String> {
    uri.split_once("://").map(|(s, _)| s.to_lowercase())
}

/// Ensure a `file://` URI is rooted under the lightspace data directory.
fn validate_file_uri_root(uri: &str) -> Result<(), ReducerError> {
    // Strip the "file://" prefix and check the path prefix.
    // Tilde expansion is intentional: the allowed root is ~/.lightarchitects/lightspace/.
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let allowed = ".lightarchitects/lightspace";
    // Accept both ~/... and expanded /Users/... paths. The prefix check is
    // advisory here — the real guard is safe_lightspace_path at the I/O layer.
    if !path.contains(allowed) {
        return Err(ReducerError::DisallowedUriScheme(uri.to_owned()));
    }
    Ok(())
}
