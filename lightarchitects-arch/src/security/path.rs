//! Hardened path resolution — H1 (canonicalize_and_check) + S-3 (per-segment symlink guard).
//!
//! # Two-pass strategy
//!
//! **Pass 1 — per-segment symlink check**: walks each accumulated prefix before following
//! symlinks.  When a segment is itself a symlink its single-hop target is canonicalized and
//! compared against `allowed_roots`.  This closes the TOCTOU window that exists between a
//! plain `starts_with` pre-check and the subsequent `fs::canonicalize()` call.
//!
//! **Pass 2 — post-canonicalize root re-check**: after `fs::canonicalize()` fully resolves
//! all recursive symlinks, the result is verified again.  Catching anything that slipped
//! through a multi-hop chain.

use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors produced by the path hardening layer.
#[derive(Debug, Error)]
pub enum PathError {
    /// The resolved path escapes every allowed root.
    #[error("path '{0}' resolves outside all allowed roots")]
    EscapesRoot(PathBuf),

    /// A symlink mid-path points outside the allowed roots.
    #[error("symlink at '{0}' points outside allowed roots (single-hop check)")]
    SymlinkEscapesRoot(PathBuf),

    /// An I/O error occurred during resolution.
    #[error("path resolution failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Resolves `path` to its canonical form, rejecting escapes from `allowed_roots`.
///
/// # Errors
///
/// - [`PathError::SymlinkEscapesRoot`] — a per-segment symlink points outside roots.
/// - [`PathError::EscapesRoot`] — the fully-resolved path is outside roots.
/// - [`PathError::Io`] — `fs::canonicalize` or `fs::read_link` failed.
pub fn canonicalize_and_check(
    path: &Path,
    allowed_roots: &[PathBuf],
) -> Result<PathBuf, PathError> {
    // Pre-canonicalize roots so comparisons work correctly on systems where allowed_roots
    // may contain symlinks (e.g. /tmp → /private/tmp on macOS).  Roots that don't yet
    // exist on disk are kept as-is; they will never match a canonical path.
    let canonical_roots: Vec<PathBuf> = allowed_roots
        .iter()
        .map(|r| std::fs::canonicalize(r).unwrap_or_else(|_| r.clone()))
        .collect();

    // --- Pass 1: per-segment symlink guard (S-3) ---
    let mut accumulated = PathBuf::new();
    for component in path.components() {
        accumulated.push(component);

        // Only act when the segment actually exists on disk.
        match std::fs::symlink_metadata(&accumulated) {
            Err(_) => continue, // non-existent segment; canonicalize will error in pass 2
            Ok(meta) if !meta.file_type().is_symlink() => continue,
            Ok(_) => {
                // Segment is a symlink — resolve one hop and check the target.
                let target = std::fs::read_link(&accumulated)?;
                let hop = if target.is_absolute() {
                    target
                } else {
                    let parent = accumulated.parent().unwrap_or(Path::new("/"));
                    parent.join(target)
                };
                let canonical_hop = std::fs::canonicalize(&hop)?;
                // Allow the symlink if its target is either:
                //   (a) within any root (descendant or equal), OR
                //   (b) a structural ancestor of any root — handles OS-level path aliasing
                //       (e.g. /var → /private/var on macOS).  The only explicit rejection
                //       is the filesystem root `/`: a symlink to `/` would admit traversal
                //       to the entire filesystem if Pass 2 were ever skipped.  All other
                //       ancestors are caught by Pass 2's unconditional root re-check.
                let within = root_contains(&canonical_hop, &canonical_roots);
                let ancestor = canonical_roots.iter().any(|r| {
                    r.starts_with(&canonical_hop)             // target is a prefix of root
                        && canonical_hop.components().count() > 1 // reject filesystem root /
                });
                if !within && !ancestor {
                    return Err(PathError::SymlinkEscapesRoot(accumulated));
                }
            }
        }
    }

    // --- Pass 2: full resolution + root re-check ---
    let canonical = std::fs::canonicalize(path)?;
    if !root_contains(&canonical, &canonical_roots) {
        return Err(PathError::EscapesRoot(canonical));
    }
    Ok(canonical)
}

fn root_contains(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|r| path.starts_with(r))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn accepts_path_inside_root() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("file.rs");
        std::fs::write(&file, b"fn main() {}").unwrap();
        // Use the canonical form of the root; on macOS /tmp is a symlink to /private/tmp.
        let canonical_root = std::fs::canonicalize(tmp.path()).unwrap();
        let roots = vec![canonical_root.clone()];
        let resolved = canonicalize_and_check(&file, &roots).unwrap();
        assert!(resolved.starts_with(&canonical_root));
    }

    #[test]
    fn rejects_path_outside_root() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("../../etc/passwd");
        let roots = vec![tmp.path().to_path_buf()];
        // canonicalize will fail on a missing path OR EscapesRoot on real /etc/passwd
        assert!(canonicalize_and_check(&file, &roots).is_err());
    }

    #[test]
    fn rejects_symlink_escaping_root() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("outside");
        std::fs::create_dir_all(&target).unwrap();
        // symlink inside root pointing outside
        let link = tmp.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink("/tmp", &link).unwrap();
        #[cfg(not(unix))]
        {
            // Skip symlink test on non-unix
            return;
        }
        let roots = vec![tmp.path().to_path_buf()];
        assert!(matches!(
            canonicalize_and_check(&link, &roots),
            Err(PathError::SymlinkEscapesRoot(_) | PathError::EscapesRoot(_))
        ));
    }
}
