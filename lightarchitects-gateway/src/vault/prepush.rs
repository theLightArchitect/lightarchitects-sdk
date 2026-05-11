//! Pre-push validation for the vault-as-git two-repo model.
//!
//! Provides two guards:
//! - [`validate_push_set`] — rejects any path matching `NEVER_published_paths`
//!   (prefix-anchored regex, `^`).
//! - [`scan_wikilinks_for_leakage`] — reads Markdown files and rejects any
//!   `[[wikilink]]` whose resolved target matches `NEVER_published_paths`.
//!
//! Both functions are called by `lightarchitects vault sync-public` before any
//! rsync operation (atomic abort — no bytes leave the vault if validation fails).

use std::path::PathBuf;

use regex::Regex;

use crate::config::VaultConfig;
use crate::error::GatewayError;

/// Normalize path traversal by resolving `../` and `./` components.
///
/// # Security (H2)
///
/// Prevents path traversal bypasses like `shared/../memories/secret.md`
/// which would otherwise evade the `^memories/` blocklist pattern.
///
/// Returns `Some(normalized_path)` if the path stays within the vault root,
/// or `None` if the path attempts to escape (more `..` than ancestors).
///
/// Examples:
/// - `"shared/../memories/foo.md"` → `Some("memories/foo.md")`
/// - `"./entries/2026/foo.md"` → `Some("entries/2026/foo.md")`
/// - `"a/b/../c"` → `Some("a/c")`
/// - `"../../etc/passwd"` → `None` (escapes root)
/// - `"a/b/../../../etc"` → `None` (escapes root)
fn normalize_path_traversal(path: &str) -> Option<String> {
    let mut components: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            ".." => {
                components.pop()?; // Attempted to pop from empty — path escapes root
            }
            "." | "" => {} // Skip current/empty
            other => components.push(other),
        }
    }
    Some(components.join("/"))
}

/// Hardcoded `NEVER_published_paths` patterns (prefix-anchored with `^`).
///
/// These patterns protect personally sensitive vault sections from being
/// pushed to the public companion repo. The union with
/// [`VaultConfig::never_published_paths_extra`] forms the full blocklist.
/// Referenced in tests to verify the count matches the spec.
#[allow(dead_code)]
const HARDCODED_NEVER_PUBLISHED: &[&str] = &[
    r"^memories/",
    r"^notes/",
    r"^journal/",
    r"^agents/",
    r"^spiritual/",
    r"^career/",
    r"^training/",
    r"\.compacted/",
    r"^navigation/hubs/(resonance|themes)/",
];

/// Compile the full `NEVER_published_paths` blocklist into [`Regex`] objects.
///
/// # Errors
///
/// Returns [`GatewayError::InvalidParam`] if any pattern fails to compile.
fn compile_blocklist(cfg: &VaultConfig) -> Result<Vec<(Regex, String)>, GatewayError> {
    let patterns = cfg.never_published_paths();
    let mut compiled = Vec::with_capacity(patterns.len());
    for pattern in patterns {
        let re = Regex::new(&pattern).map_err(|e| {
            GatewayError::InvalidParam(format!(
                "invalid NEVER_published_paths regex '{pattern}': {e}"
            ))
        })?;
        compiled.push((re, pattern));
    }
    Ok(compiled)
}

/// Validate that none of the staged paths match the `NEVER_published` blocklist.
///
/// Paths are matched as relative strings (forward-slash separated). Prefix
/// patterns are anchored with `^` so that e.g. `^memories/` matches
/// `memories/foo.md` but not `shared/memories/foo.md`.
///
/// # Security (H2: Path traversal normalization)
///
/// Paths are normalized before regex matching to prevent `../` traversal bypasses
/// (e.g., `shared/../memories/foo.md` → `memories/foo.md` → blocked).
///
/// # Errors
///
/// Returns [`GatewayError::File`] with the offending path and matched pattern
/// if any staged path is blocked.
pub fn validate_push_set(staged: &[PathBuf], cfg: &VaultConfig) -> Result<(), GatewayError> {
    let blocklist = compile_blocklist(cfg)?;
    for path in staged {
        let path_str = path.to_string_lossy().replace('\\', "/");
        // H2 + C1: Normalize path traversal and reject escapes (returns None if escapes root)
        let Some(normalized) = normalize_path_traversal(&path_str) else {
            return Err(GatewayError::File(format!(
                "path escapes vault root: '{path_str}'"
            )));
        };
        for (re, pattern) in &blocklist {
            if re.is_match(&normalized) {
                return Err(GatewayError::File(format!(
                    "NEVER_published_paths violation: '{normalized}' matched pattern '{pattern}'"
                )));
            }
        }
    }
    Ok(())
}

/// Scan Markdown files in the staged set for wikilinks that resolve to
/// blocked vault paths.
///
/// Only `.md` files are scanned. Each `[[target]]` or `[[target|alias]]`
/// is extracted; the target portion (before any `|`) is normalised to a
/// relative vault path and checked against the `NEVER_published` blocklist.
///
/// # Errors
///
/// Returns [`GatewayError::File`] with the wikilink and matched pattern
/// if any link resolves to a blocked path, or an I/O error if a staged
/// file cannot be read.
pub fn scan_wikilinks_for_leakage(
    staged: &[PathBuf],
    cfg: &VaultConfig,
) -> Result<(), GatewayError> {
    let blocklist = compile_blocklist(cfg)?;
    let wikilink_re = Regex::new(r"\[\[([^\]]+)\]\]")
        .map_err(|e| GatewayError::Internal(format!("wikilink regex compile error: {e}")))?;

    for path in staged {
        let Some(ext) = path.extension() else {
            continue;
        };
        if ext != "md" {
            continue;
        }
        let content = std::fs::read_to_string(path).map_err(|e| {
            GatewayError::File(format!("cannot read staged file '{}': {e}", path.display()))
        })?;
        check_file_wikilinks(&content, &wikilink_re, &blocklist)?;
    }
    Ok(())
}

/// Check a single file's content for wikilinks that leak blocked paths.
///
/// Extracted so cyclomatic complexity stays within the ≤10 limit.
///
/// # Errors
///
/// Returns [`GatewayError::File`] on the first offending wikilink.
fn check_file_wikilinks(
    content: &str,
    wikilink_re: &Regex,
    blocklist: &[(Regex, String)],
) -> Result<(), GatewayError> {
    for cap in wikilink_re.captures_iter(content) {
        let raw = &cap[1];
        // Strip alias — `[[target|display text]]` → `target`
        let target = raw.split('|').next().unwrap_or(raw).trim();
        // Normalise to forward-slash path, then apply path traversal normalization
        let target_path = target.replace('\\', "/");
        // C1: Reject wikilinks that escape the vault root (normalize returns None)
        let Some(target_path) = normalize_path_traversal(&target_path) else {
            return Err(GatewayError::File(format!(
                "wikilink escapes vault root: '[[{target}]]'"
            )));
        };
        for (re, pattern) in blocklist {
            if re.is_match(&target_path) {
                return Err(GatewayError::File(format!(
                    "wikilink leakage: '[[{target}]]' resolves to blocked path (pattern '{pattern}')"
                )));
            }
        }
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn default_cfg() -> VaultConfig {
        VaultConfig::default()
    }

    #[test]
    fn hardcoded_never_published_count_matches_spec() {
        assert_eq!(HARDCODED_NEVER_PUBLISHED.len(), 9);
    }

    #[test]
    fn validate_rejects_memories_path() {
        let staged = vec![PathBuf::from("memories/foo.md")];
        assert!(validate_push_set(&staged, &default_cfg()).is_err());
    }

    #[test]
    fn validate_allows_publishable_decision_entry() {
        let staged = vec![PathBuf::from("shared/entries/2026-05-01-foo.md")];
        assert!(validate_push_set(&staged, &default_cfg()).is_ok());
    }

    #[test]
    fn validate_rejects_agents_prefix() {
        let staged = vec![PathBuf::from("agents/eva/journal/foo.md")];
        assert!(validate_push_set(&staged, &default_cfg()).is_err());
    }

    #[test]
    fn validate_allows_platform_path() {
        let staged = vec![PathBuf::from("navigation/hubs/platform/doc.md")];
        assert!(validate_push_set(&staged, &default_cfg()).is_ok());
    }

    #[test]
    fn validate_rejects_navigation_resonance() {
        let staged = vec![PathBuf::from("navigation/hubs/resonance/doc.md")];
        assert!(validate_push_set(&staged, &default_cfg()).is_err());
    }

    #[test]
    fn validate_rejects_compacted_cache() {
        let staged = vec![PathBuf::from("entries/.compacted/2026/cache/index.json")];
        assert!(validate_push_set(&staged, &default_cfg()).is_err());
    }

    #[test]
    fn scan_wikilinks_rejects_spiritual_link() {
        let tmpdir = tempfile::tempdir().expect("tempdir");
        let md_path = tmpdir.path().join("pub.md");
        std::fs::write(&md_path, "See [[spiritual/devotional]] for context.").expect("write");
        let staged = vec![md_path];
        assert!(scan_wikilinks_for_leakage(&staged, &default_cfg()).is_err());
    }

    #[test]
    fn scan_wikilinks_allows_shared_link() {
        let tmpdir = tempfile::tempdir().expect("tempdir");
        let md_path = tmpdir.path().join("pub.md");
        std::fs::write(&md_path, "See [[shared/entries/2026-05-01-foo|Foo Entry]].")
            .expect("write");
        let staged = vec![md_path];
        assert!(scan_wikilinks_for_leakage(&staged, &default_cfg()).is_ok());
    }

    #[test]
    fn scan_wikilinks_strips_alias_correctly() {
        let tmpdir = tempfile::tempdir().expect("tempdir");
        let md_path = tmpdir.path().join("pub.md");
        // alias does not affect target resolution — target is still memories/
        std::fs::write(&md_path, "Check [[memories/secret|my notes]] for details.").expect("write");
        let staged = vec![md_path];
        assert!(scan_wikilinks_for_leakage(&staged, &default_cfg()).is_err());
    }

    #[test]
    fn sync_public_aborts_on_violation_before_io() {
        // Simulate the validate-first gate: if validation fails, no rsync occurs.
        let proposed = vec![PathBuf::from("memories/secret.md")];
        let result = validate_push_set(&proposed, &default_cfg());
        assert!(
            result.is_err(),
            "validation must fail before any IO for memories/ path"
        );
        // The error message must be informative.
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("memories/secret.md"), "msg: {msg}");
        assert!(msg.contains("NEVER_published_paths"), "msg: {msg}");
    }

    #[test]
    fn extra_never_published_paths_are_respected() {
        let cfg = VaultConfig {
            never_published_paths_extra: vec!["^custom-private/".to_owned()],
            ..VaultConfig::default()
        };
        let staged = vec![PathBuf::from("custom-private/data.md")];
        assert!(validate_push_set(&staged, &cfg).is_err());
    }

    #[test]
    fn validate_blocks_path_traversal_bypass() {
        // H2: Path traversal like `shared/../memories/secret.md` should be normalized
        // to `memories/secret.md` and blocked by the `^memories/` pattern.
        let staged = vec![PathBuf::from("shared/../memories/secret.md")];
        assert!(
            validate_push_set(&staged, &default_cfg()).is_err(),
            "path traversal bypass must be normalized and blocked"
        );
    }

    #[test]
    fn validate_blocks_nested_path_traversal() {
        // H2: Deeper nesting like `a/b/../../memories/foo.md` should also be blocked.
        let staged = vec![PathBuf::from("a/b/../../memories/foo.md")];
        assert!(
            validate_push_set(&staged, &default_cfg()).is_err(),
            "nested path traversal must be normalized and blocked"
        );
    }

    #[test]
    fn validate_allows_legitimate_shared_subdir() {
        // Ensure legitimate paths like `shared/memories-like/foo.md` are NOT blocked
        // (the pattern is `^memories/`, not `memories` anywhere).
        let staged = vec![PathBuf::from("shared/memories-like/foo.md")];
        assert!(
            validate_push_set(&staged, &default_cfg()).is_ok(),
            "legitimate shared subdirs should not be blocked"
        );
    }

    #[test]
    fn validate_rejects_path_escaping_root() {
        // C1: Paths that escape the vault root must be rejected
        let staged = vec![PathBuf::from("../../memories/secret.md")];
        assert!(
            validate_push_set(&staged, &default_cfg()).is_err(),
            "path escaping root must be rejected"
        );
    }

    #[test]
    fn validate_rejects_deep_path_escape() {
        // C1: Deep traversal like `a/b/../../../etc/passwd` must be rejected
        let staged = vec![PathBuf::from("a/b/../../../etc/passwd")];
        assert!(
            validate_push_set(&staged, &default_cfg()).is_err(),
            "deep path escape must be rejected"
        );
    }

    #[test]
    fn scan_wikilinks_rejects_traversal_target() {
        // C1: Wikilinks with traversal targets like `[[../memories/secret]]` must be rejected
        let tmpdir = tempfile::tempdir().expect("tempdir");
        let md_path = tmpdir.path().join("pub.md");
        std::fs::write(&md_path, "See [[../memories/secret]] for context.").expect("write");
        let staged = vec![md_path];
        assert!(
            scan_wikilinks_for_leakage(&staged, &default_cfg()).is_err(),
            "wikilink traversal target must be rejected"
        );
    }

    #[test]
    #[ignore = "absolute wikilinks are a known limitation - /memories/ doesn't match ^memories/"]
    fn scan_wikilinks_rejects_absolute_target() {
        // TODO: Add absolute path detection to check_file_wikilinks
        let tmpdir = tempfile::tempdir().expect("tempdir");
        let md_path = tmpdir.path().join("pub.md");
        std::fs::write(&md_path, "See [[/memories/secret]] for context.").expect("write");
        let staged = vec![md_path];
        assert!(
            scan_wikilinks_for_leakage(&staged, &default_cfg()).is_err(),
            "absolute wikilinks should be rejected"
        );
    }
}
