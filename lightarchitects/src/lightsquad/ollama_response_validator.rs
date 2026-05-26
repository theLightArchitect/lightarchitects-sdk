//! Response validator — security gate for Ollama-generated code patches.
//!
//! Applies four sequential gates before any file is written to a worktree:
//!
//! 1. **G-TRAVERSAL**: reject relative paths containing `..` components.
//! 2. **G-DENY**: reject files matching the [`DENIED_PREFIXES`] list (ACE-on-CI
//!    prevention); `build.rs` is denied only when newly added.
//! 3. **G-SYMLINK**: canonicalize the parent directory and re-append the
//!    filename; reject paths that escape the worktree root
//!    (Security Guardrails §63.P4).
//! 4. **G-CARGO**: if the path is `Cargo.toml`, scan the content for forbidden
//!    TOML section additions (`[patch.*]`, `[profile.*.build-override]`,
//!    `[target.*.linker]`).
//!
//! Call [`validate_total_diff_size`] after collecting all blocks but before
//! writing any file to enforce the byte ceiling.
//!
//! [`validate_total_diff_size`]: OllamaResponseValidator::validate_total_diff_size

use std::path::{Path, PathBuf};

use tracing::warn;

/// Hard byte ceiling for the sum of all [`CodeBlock`] contents in a single task.
///
/// Configurable via `LIGHTSQUAD_DIFF_BYTES_MAX` at startup.  Defaults to 100 KB.
pub const DIFF_BYTES_MAX_DEFAULT: usize = 102_400;

/// Relative path prefixes that are always denied, regardless of content.
///
/// Matched as a string prefix against the slash-normalised relative path.
/// Prevents ACE-on-CI attacks that target CI runner configuration, Cargo build
/// scripts, and workflow definitions.
const DENIED_PREFIXES: &[&str] = &[".cargo/config.toml", ".cargo/config", ".github/workflows/"];

/// TOML section headers whose presence in a `Cargo.toml` patch is forbidden.
///
/// The check is line-by-line prefix matching (case-sensitive, trimmed).
const CARGO_TOML_FORBIDDEN_SECTIONS: &[&str] = &["[patch.", "[profile.", "[target."];

// ── CodeBlock ─────────────────────────────────────────────────────────────────

/// A single file patch extracted from an Ollama response.
#[derive(Debug, Clone)]
pub struct CodeBlock {
    /// Relative path within the worktree (e.g. `src/lib.rs`).
    pub rel_path: PathBuf,
    /// Complete replacement file content.
    pub content: String,
}

// ── ValidatorRejection ────────────────────────────────────────────────────────

/// Rejection reason returned by [`OllamaResponseValidator`].
#[derive(Debug, thiserror::Error)]
pub enum ValidatorRejection {
    /// Path contains `..` traversal components.
    #[error("path '{path}' contains traversal component '..'")]
    PathTraversal {
        /// The offending relative path string.
        path: String,
    },

    /// Path matches the [`DENIED_PREFIXES`] denylist or the new-`build.rs` rule.
    #[error("denied file path '{path}': {reason}")]
    DeniedFile {
        /// The rejected relative path.
        path: String,
        /// Human-readable reason.
        reason: &'static str,
    },

    /// Path escapes the worktree root after symlink resolution.
    #[error("path '{rel}' escapes worktree root (resolved to '{resolved}', root '{root}')")]
    SymlinkEscape {
        /// Relative path that was rejected.
        rel: String,
        /// Canonical resolved absolute path.
        resolved: PathBuf,
        /// Worktree root.
        root: PathBuf,
    },

    /// Total diff bytes exceed the configured ceiling.
    #[error("diff size {bytes}B exceeds ceiling {limit}B")]
    DiffTooLarge {
        /// Actual total byte count.
        bytes: usize,
        /// Configured limit.
        limit: usize,
    },

    /// A `Cargo.toml` patch contains a forbidden section header.
    #[error("Cargo.toml contains forbidden section '{section}'")]
    ForbiddenCargoSection {
        /// The forbidden TOML section header (e.g. `[patch.crates-io]`).
        section: String,
    },
}

// ── OllamaResponseValidator ───────────────────────────────────────────────────

/// Security gate for Ollama-generated code patches.
///
/// Constructed once per build; `Clone` is cheap (only contains a `usize`).
#[derive(Debug, Clone)]
pub struct OllamaResponseValidator {
    /// Maximum total bytes for all [`CodeBlock`] contents in a single task.
    pub diff_bytes_max: usize,
}

impl Default for OllamaResponseValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl OllamaResponseValidator {
    /// Create a validator reading `LIGHTSQUAD_DIFF_BYTES_MAX` from the environment.
    ///
    /// Falls back to [`DIFF_BYTES_MAX_DEFAULT`] when the variable is absent or
    /// non-numeric.
    #[must_use]
    pub fn new() -> Self {
        let limit = std::env::var("LIGHTSQUAD_DIFF_BYTES_MAX")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DIFF_BYTES_MAX_DEFAULT);
        Self {
            diff_bytes_max: limit,
        }
    }

    /// Validate one [`CodeBlock`] against all path-level and content-level gates.
    ///
    /// Gates applied in order: G-TRAVERSAL → G-DENY → G-SYMLINK → G-CARGO.
    ///
    /// Returns the **absolute canonical** path the file should be written to on
    /// success. Call [`validate_total_diff_size`] separately after all blocks
    /// have been validated individually.
    ///
    /// # Errors
    ///
    /// Returns [`ValidatorRejection`] if any gate fires.
    ///
    /// [`validate_total_diff_size`]: OllamaResponseValidator::validate_total_diff_size
    pub fn validate_block(
        &self,
        worktree_root: &Path,
        block: &CodeBlock,
    ) -> Result<PathBuf, ValidatorRejection> {
        let rel = &block.rel_path;

        // G-TRAVERSAL
        if rel.components().any(|c| c.as_os_str() == "..") {
            return Err(ValidatorRejection::PathTraversal {
                path: rel.to_string_lossy().into_owned(),
            });
        }

        // G-DENY (prefix list)
        Self::check_deny_prefix(rel)?;

        // G-DENY (new build.rs rule)
        if rel == Path::new("build.rs") && !worktree_root.join("build.rs").exists() {
            return Err(ValidatorRejection::DeniedFile {
                path: "build.rs".to_owned(),
                reason: "newly-added build.rs is forbidden (ACE-on-CI prevention)",
            });
        }

        // G-SYMLINK
        let abs = Self::safe_abs_path(worktree_root, rel)?;

        // G-CARGO
        if rel.file_name().is_some_and(|n| n == "Cargo.toml") {
            Self::check_cargo_toml_content(&block.content)?;
        }

        Ok(abs)
    }

    /// Verify the total byte count of all blocks does not exceed [`diff_bytes_max`].
    ///
    /// Call this once after individually validating every block, before writing
    /// any file to disk.
    ///
    /// # Errors
    ///
    /// Returns [`ValidatorRejection::DiffTooLarge`] if the ceiling is exceeded.
    ///
    /// [`diff_bytes_max`]: OllamaResponseValidator::diff_bytes_max
    pub fn validate_total_diff_size(&self, blocks: &[CodeBlock]) -> Result<(), ValidatorRejection> {
        let total: usize = blocks.iter().map(|b| b.content.len()).sum();
        if total > self.diff_bytes_max {
            return Err(ValidatorRejection::DiffTooLarge {
                bytes: total,
                limit: self.diff_bytes_max,
            });
        }
        Ok(())
    }

    // ── Private helpers ──────────────────────────────────────────────────────────

    fn check_deny_prefix(rel: &Path) -> Result<(), ValidatorRejection> {
        // Normalise to forward-slash for platform-independent prefix matching.
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        for prefix in DENIED_PREFIXES {
            if rel_str.starts_with(prefix) {
                return Err(ValidatorRejection::DeniedFile {
                    path: rel_str.clone(),
                    reason: "path matches DENIED_FILES prefix",
                });
            }
        }
        Ok(())
    }

    /// Resolve a symlink-safe absolute path for `rel` within `worktree_root`.
    ///
    /// Strategy (Security Guardrails §63.P4): canonicalize the **parent**
    /// directory (which already exists) and re-append the filename.  The file
    /// itself may not yet exist, so canonicalizing the full path would fail.
    fn safe_abs_path(worktree_root: &Path, rel: &Path) -> Result<PathBuf, ValidatorRejection> {
        let root_canonical = worktree_root
            .canonicalize()
            .unwrap_or_else(|_| worktree_root.to_path_buf());

        let tentative = worktree_root.join(rel);
        let parent = tentative.parent().unwrap_or(worktree_root);

        let parent_canonical = if parent.exists() {
            parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf())
        } else {
            // Parent will be created before the write; use tentative path.
            parent.to_path_buf()
        };

        let file_name = tentative
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new(""));
        let resolved = parent_canonical.join(file_name);

        if !resolved.starts_with(&root_canonical) {
            warn!(
                resolved = %resolved.display(),
                root = %root_canonical.display(),
                "OllamaResponseValidator: path escapes worktree root (G-SYMLINK)"
            );
            return Err(ValidatorRejection::SymlinkEscape {
                rel: rel.to_string_lossy().into_owned(),
                resolved,
                root: root_canonical,
            });
        }

        Ok(resolved)
    }

    fn check_cargo_toml_content(content: &str) -> Result<(), ValidatorRejection> {
        for line in content.lines() {
            let trimmed = line.trim();
            for section in CARGO_TOML_FORBIDDEN_SECTIONS {
                if trimmed.starts_with(section) {
                    return Err(ValidatorRejection::ForbiddenCargoSection {
                        section: trimmed.to_owned(),
                    });
                }
            }
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn validator() -> OllamaResponseValidator {
        OllamaResponseValidator {
            diff_bytes_max: DIFF_BYTES_MAX_DEFAULT,
        }
    }

    fn block(rel: &str, content: &str) -> CodeBlock {
        CodeBlock {
            rel_path: PathBuf::from(rel),
            content: content.to_owned(),
        }
    }

    // ── G-TRAVERSAL ──────────────────────────────────────────────────────────────

    #[test]
    fn traversal_rejected() {
        let dir = TempDir::new().unwrap();
        let v = validator();
        let b = block("../etc/passwd", "evil");
        assert!(matches!(
            v.validate_block(dir.path(), &b),
            Err(ValidatorRejection::PathTraversal { .. })
        ));
    }

    #[test]
    fn nested_traversal_rejected() {
        let dir = TempDir::new().unwrap();
        let v = validator();
        let b = block("src/../../secret", "evil");
        assert!(matches!(
            v.validate_block(dir.path(), &b),
            Err(ValidatorRejection::PathTraversal { .. })
        ));
    }

    // ── G-DENY ───────────────────────────────────────────────────────────────────

    #[test]
    fn cargo_config_toml_rejected() {
        let dir = TempDir::new().unwrap();
        let v = validator();
        let b = block(
            ".cargo/config.toml",
            "[build]\nrustflags = [\"-C\", \"link-arg=evil\"]",
        );
        assert!(matches!(
            v.validate_block(dir.path(), &b),
            Err(ValidatorRejection::DeniedFile { .. })
        ));
    }

    #[test]
    fn cargo_config_no_extension_rejected() {
        let dir = TempDir::new().unwrap();
        let v = validator();
        let b = block(".cargo/config", "evil");
        assert!(matches!(
            v.validate_block(dir.path(), &b),
            Err(ValidatorRejection::DeniedFile { .. })
        ));
    }

    #[test]
    fn github_workflow_rejected() {
        let dir = TempDir::new().unwrap();
        let v = validator();
        let b = block(".github/workflows/evil.yml", "name: pwn");
        assert!(matches!(
            v.validate_block(dir.path(), &b),
            Err(ValidatorRejection::DeniedFile { .. })
        ));
    }

    #[test]
    fn new_build_rs_rejected() {
        let dir = TempDir::new().unwrap();
        let v = validator();
        // build.rs does NOT exist in the tempdir
        let b = block("build.rs", "fn main() {}");
        assert!(matches!(
            v.validate_block(dir.path(), &b),
            Err(ValidatorRejection::DeniedFile { .. })
        ));
    }

    #[test]
    fn existing_build_rs_allowed() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("build.rs"), "fn main() {}").unwrap();
        let v = validator();
        let b = block(
            "build.rs",
            "fn main() { println!(\"cargo:rerun-if-changed=build.rs\"); }",
        );
        // Existing build.rs is allowed to be edited.
        assert!(v.validate_block(dir.path(), &b).is_ok());
    }

    // ── G-SYMLINK ─────────────────────────────────────────────────────────────────

    #[test]
    fn normal_src_file_passes_symlink_gate() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        let v = validator();
        let b = block("src/lib.rs", "pub fn hello() {}");
        assert!(v.validate_block(dir.path(), &b).is_ok());
    }

    #[test]
    fn abs_path_in_root_is_under_root() {
        let dir = TempDir::new().unwrap();
        let v = validator();
        let b = block(
            "Cargo.toml",
            "[package]\nname = \"foo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        let result = v.validate_block(dir.path(), &b);
        assert!(result.is_ok());
        let abs = result.unwrap();
        assert!(abs.starts_with(dir.path().canonicalize().unwrap()));
    }

    // ── G-CARGO ──────────────────────────────────────────────────────────────────

    #[test]
    fn cargo_toml_patch_section_rejected() {
        let dir = TempDir::new().unwrap();
        let v = validator();
        let b = block(
            "Cargo.toml",
            "[package]\nname = \"foo\"\n\n[patch.crates-io]\nserde = { path = \"evil\" }\n",
        );
        assert!(matches!(
            v.validate_block(dir.path(), &b),
            Err(ValidatorRejection::ForbiddenCargoSection { .. })
        ));
    }

    #[test]
    fn cargo_toml_profile_section_rejected() {
        let dir = TempDir::new().unwrap();
        let v = validator();
        let b = block(
            "Cargo.toml",
            "[package]\nname = \"foo\"\n\n[profile.release.build-override]\nopt-level = 3\n",
        );
        assert!(matches!(
            v.validate_block(dir.path(), &b),
            Err(ValidatorRejection::ForbiddenCargoSection { .. })
        ));
    }

    #[test]
    fn cargo_toml_target_section_rejected() {
        let dir = TempDir::new().unwrap();
        let v = validator();
        let b = block(
            "Cargo.toml",
            "[package]\nname = \"foo\"\n\n[target.x86_64-unknown-linux-gnu.linker]\nlinker = \"evil-ld\"\n",
        );
        assert!(matches!(
            v.validate_block(dir.path(), &b),
            Err(ValidatorRejection::ForbiddenCargoSection { .. })
        ));
    }

    #[test]
    fn cargo_toml_safe_dep_section_allowed() {
        let dir = TempDir::new().unwrap();
        let v = validator();
        let content = "[package]\nname = \"foo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n";
        let b = block("Cargo.toml", content);
        assert!(v.validate_block(dir.path(), &b).is_ok());
    }

    // ── G-SIZE ────────────────────────────────────────────────────────────────────

    #[test]
    fn diff_within_limit_passes() {
        let v = OllamaResponseValidator {
            diff_bytes_max: 100,
        };
        let blocks = vec![block("src/a.rs", "a".repeat(50).as_str())];
        assert!(v.validate_total_diff_size(&blocks).is_ok());
    }

    #[test]
    fn diff_exactly_at_limit_passes() {
        let v = OllamaResponseValidator { diff_bytes_max: 10 };
        let blocks = vec![block("src/a.rs", "aaaaaaaaaa")]; // 10 bytes
        assert!(v.validate_total_diff_size(&blocks).is_ok());
    }

    #[test]
    fn diff_over_limit_rejected() {
        let v = OllamaResponseValidator { diff_bytes_max: 10 };
        let blocks = vec![block("src/a.rs", "aaaaaaaaaaa")]; // 11 bytes
        assert!(matches!(
            v.validate_total_diff_size(&blocks),
            Err(ValidatorRejection::DiffTooLarge {
                bytes: 11,
                limit: 10
            })
        ));
    }

    #[test]
    fn multi_block_size_is_summed() {
        let v = OllamaResponseValidator { diff_bytes_max: 10 };
        let blocks = vec![
            block("src/a.rs", "aaaaa"), // 5 bytes
            block("src/b.rs", "bbbbb"), // 5 bytes — total = 10, at limit
        ];
        assert!(v.validate_total_diff_size(&blocks).is_ok());

        let blocks_over = vec![
            block("src/a.rs", "aaaaa"),  // 5 bytes
            block("src/b.rs", "bbbbbb"), // 6 bytes — total = 11, over
        ];
        assert!(matches!(
            v.validate_total_diff_size(&blocks_over),
            Err(ValidatorRejection::DiffTooLarge { .. })
        ));
    }
}
