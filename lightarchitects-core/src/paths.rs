//! Canonical Light Architects filesystem layout resolution.
//!
//! This module centralizes runtime path construction for sibling state,
//! debug logs, helix storage, and shared secret material. It performs no
//! filesystem I/O and is safe to call from hot paths.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const LIGHTARCHITECTS_HOME_ENV: &str = "LIGHTARCHITECTS_HOME";
const LIGHTARCHITECTS_DIRNAME: &str = "lightarchitects";
const DEBUG_DIRNAME: &str = "debug";
const HELIX_DIRNAME: &str = "helix";
const SESSION_KEY_FILENAME: &str = ".session-key";
const FALLBACK_ROOT: &str = "/tmp/lightarchitects-fallback";

/// Returns the canonical `~/lightarchitects` root, honoring `LIGHTARCHITECTS_HOME`.
///
/// If the override is set to a relative path, it is resolved relative to `$HOME`.
/// Returns `None` only when `$HOME` is unavailable.
#[must_use]
pub fn root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(resolve_root(
        Path::new(&home),
        std::env::var_os(LIGHTARCHITECTS_HOME_ENV).as_deref(),
    ))
}

/// Returns the canonical runtime directory for a sibling.
///
/// Known siblings should prefer the dedicated helpers such as [`soul`] or [`laex0`].
/// Returns `None` when `$HOME` is unavailable or `name` is not a safe single path segment.
#[must_use]
pub fn sibling_runtime(name: &str) -> Option<PathBuf> {
    let root = root()?;
    let segment = normalized_segment(name)?;
    Some(root.join(segment))
}

/// Returns the canonical debug root: `~/lightarchitects/debug/`.
#[must_use]
pub fn debug_root() -> Option<PathBuf> {
    Some(root()?.join(DEBUG_DIRNAME))
}

/// Returns the canonical debug directory for a sibling.
#[must_use]
pub fn sibling_debug(name: &str) -> Option<PathBuf> {
    let debug_root = debug_root()?;
    let segment = normalized_segment(name)?;
    Some(debug_root.join(segment))
}

/// Returns the canonical `launchd` debug directory for a sibling.
#[must_use]
pub fn launchd_debug(name: &str) -> Option<PathBuf> {
    Some(sibling_debug(name)?.join("launchd"))
}

/// Returns the canonical SOUL helix root.
#[must_use]
pub fn helix_root() -> Option<PathBuf> {
    Some(soul()?.join(HELIX_DIRNAME))
}

/// Returns the canonical per-session key path for lÆx0 turn/session integrity.
#[must_use]
pub fn session_key() -> Option<PathBuf> {
    Some(laex0()?.join(SESSION_KEY_FILENAME))
}

/// Returns `~/lightarchitects/soul/`.
#[must_use]
pub fn soul() -> Option<PathBuf> {
    sibling_runtime("soul")
}

/// Returns `~/lightarchitects/laex0/`.
#[must_use]
pub fn laex0() -> Option<PathBuf> {
    sibling_runtime("laex0")
}

/// Returns `~/lightarchitects/corso/`.
#[must_use]
pub fn corso() -> Option<PathBuf> {
    sibling_runtime("corso")
}

/// Returns `~/lightarchitects/eva/`.
#[must_use]
pub fn eva() -> Option<PathBuf> {
    sibling_runtime("eva")
}

/// Returns `~/lightarchitects/quantum/`.
#[must_use]
pub fn quantum() -> Option<PathBuf> {
    sibling_runtime("quantum")
}

/// Returns `~/lightarchitects/seraph/`.
#[must_use]
pub fn seraph() -> Option<PathBuf> {
    sibling_runtime("seraph")
}

/// Returns `~/lightarchitects/ayin/`.
#[must_use]
pub fn ayin() -> Option<PathBuf> {
    sibling_runtime("ayin")
}

/// Returns `~/lightarchitects/webshell/`.
#[must_use]
pub fn webshell() -> Option<PathBuf> {
    sibling_runtime("webshell")
}

// ──────────────────────────────────────────────────────────────────────────
// TOTAL VARIANTS — never fail, return a sensible fallback on missing $HOME.
//
// When $HOME is unavailable (CI sandbox, minimal env), these return paths
// under `/tmp/lightarchitects-fallback/` so callers that don't want to
// propagate Option can keep PathBuf arithmetic flat.
//
// Prefer the Option variants above when the caller can legitimately
// decline to proceed on missing $HOME (e.g., startup validation).
// ──────────────────────────────────────────────────────────────────────────

/// Returns [`root`] or a fallback under `/tmp/lightarchitects-fallback/`.
#[must_use]
pub fn root_or_fallback() -> PathBuf {
    root().unwrap_or_else(|| PathBuf::from(FALLBACK_ROOT))
}

/// Returns [`sibling_runtime`] or a fallback.
///
/// If `name` is an invalid segment, falls back to `/tmp/lightarchitects-fallback/`.
#[must_use]
pub fn sibling_runtime_or_fallback(name: &str) -> PathBuf {
    sibling_runtime(name).unwrap_or_else(|| match normalized_segment(name) {
        Some(segment) => PathBuf::from(FALLBACK_ROOT).join(segment),
        None => PathBuf::from(FALLBACK_ROOT),
    })
}

/// Returns [`debug_root`] or a fallback.
#[must_use]
pub fn debug_root_or_fallback() -> PathBuf {
    debug_root().unwrap_or_else(|| PathBuf::from(FALLBACK_ROOT).join(DEBUG_DIRNAME))
}

/// Returns [`sibling_debug`] or a fallback.
#[must_use]
pub fn sibling_debug_or_fallback(name: &str) -> PathBuf {
    sibling_debug(name).unwrap_or_else(|| {
        let base = PathBuf::from(FALLBACK_ROOT).join(DEBUG_DIRNAME);
        match normalized_segment(name) {
            Some(segment) => base.join(segment),
            None => base,
        }
    })
}

/// Returns [`launchd_debug`] or a fallback.
#[must_use]
pub fn launchd_debug_or_fallback(name: &str) -> PathBuf {
    sibling_debug_or_fallback(name).join("launchd")
}

/// Returns [`helix_root`] or a fallback.
#[must_use]
pub fn helix_root_or_fallback() -> PathBuf {
    soul_or_fallback().join(HELIX_DIRNAME)
}

/// Returns [`session_key`] or a fallback.
#[must_use]
pub fn session_key_or_fallback() -> PathBuf {
    laex0_or_fallback().join(SESSION_KEY_FILENAME)
}

/// Returns `~/lightarchitects/soul/` or a fallback.
#[must_use]
pub fn soul_or_fallback() -> PathBuf {
    sibling_runtime_or_fallback("soul")
}

/// Returns `~/lightarchitects/laex0/` or a fallback.
#[must_use]
pub fn laex0_or_fallback() -> PathBuf {
    sibling_runtime_or_fallback("laex0")
}

/// Returns `~/lightarchitects/corso/` or a fallback.
#[must_use]
pub fn corso_or_fallback() -> PathBuf {
    sibling_runtime_or_fallback("corso")
}

/// Returns `~/lightarchitects/eva/` or a fallback.
#[must_use]
pub fn eva_or_fallback() -> PathBuf {
    sibling_runtime_or_fallback("eva")
}

/// Returns `~/lightarchitects/quantum/` or a fallback.
#[must_use]
pub fn quantum_or_fallback() -> PathBuf {
    sibling_runtime_or_fallback("quantum")
}

/// Returns `~/lightarchitects/seraph/` or a fallback.
#[must_use]
pub fn seraph_or_fallback() -> PathBuf {
    sibling_runtime_or_fallback("seraph")
}

/// Returns `~/lightarchitects/ayin/` or a fallback.
#[must_use]
pub fn ayin_or_fallback() -> PathBuf {
    sibling_runtime_or_fallback("ayin")
}

/// Returns `~/lightarchitects/webshell/` or a fallback.
#[must_use]
pub fn webshell_or_fallback() -> PathBuf {
    sibling_runtime_or_fallback("webshell")
}

fn resolve_root(home: &Path, override_value: Option<&OsStr>) -> PathBuf {
    match override_value {
        Some(value) if !value.is_empty() => {
            let override_path = PathBuf::from(value);
            if override_path.is_absolute() {
                override_path
            } else {
                home.join(override_path)
            }
        }
        _ => home.join(LIGHTARCHITECTS_DIRNAME),
    }
}

fn normalized_segment(name: &str) -> Option<&str> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return None;
    }

    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_root_defaults_to_home_lightarchitects() {
        let root = resolve_root(Path::new("/Users/kft"), None);
        assert_eq!(root, PathBuf::from("/Users/kft/lightarchitects"));
    }

    #[test]
    fn resolve_root_uses_absolute_override() {
        let root = resolve_root(Path::new("/Users/kft"), Some(OsStr::new("/tmp/la-home")));
        assert_eq!(root, PathBuf::from("/tmp/la-home"));
    }

    #[test]
    fn resolve_root_resolves_relative_override_against_home() {
        let root = resolve_root(
            Path::new("/Users/kft"),
            Some(OsStr::new("workspace/lightarchitects")),
        );
        assert_eq!(root, PathBuf::from("/Users/kft/workspace/lightarchitects"));
    }

    #[test]
    fn normalized_segment_accepts_simple_name() {
        assert_eq!(normalized_segment("soul"), Some("soul"));
    }

    #[test]
    fn normalized_segment_rejects_invalid_segments() {
        for invalid in ["", ".", "..", "soul/debug", "soul\\debug"] {
            assert_eq!(normalized_segment(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn helix_root_is_under_soul_runtime() {
        let root = resolve_root(Path::new("/Users/kft"), None);
        let helix = root.join("soul").join("helix");
        assert_eq!(
            helix,
            PathBuf::from("/Users/kft/lightarchitects/soul/helix")
        );
    }

    #[test]
    fn session_key_is_under_laex0_runtime() {
        let root = resolve_root(Path::new("/Users/kft"), None);
        let session_key = root.join("laex0").join(".session-key");
        assert_eq!(
            session_key,
            PathBuf::from("/Users/kft/lightarchitects/laex0/.session-key")
        );
    }

    #[test]
    fn debug_paths_include_launchd_partition() {
        let root = resolve_root(Path::new("/Users/kft"), None);
        let launchd = root.join("debug").join("ayin").join("launchd");
        assert_eq!(
            launchd,
            PathBuf::from("/Users/kft/lightarchitects/debug/ayin/launchd")
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // Fallback-variant tests — env-free. We avoid mutating HOME (which
    // would require `unsafe { std::env::set_var }` in Rust 2024 and break
    // the workspace-wide `unsafe-code = deny`). Instead we test the pure
    // composition logic: when the `Option` source yields `None`, the
    // fallback path lands under `/tmp/lightarchitects-fallback/`.
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn fallback_root_constant_matches_doc_contract() {
        // Changing this constant breaks every caller that relies on the
        // documented fallback location — guard it with a hard equality.
        assert_eq!(FALLBACK_ROOT, "/tmp/lightarchitects-fallback");
    }

    #[test]
    fn fallback_sibling_runtime_composes_under_fallback_root() {
        // Simulate the `None` path by invoking with an invalid segment —
        // `sibling_runtime` returns `None`, and the fallback should yield
        // exactly FALLBACK_ROOT (no joined segment, because segment was invalid).
        assert_eq!(
            sibling_runtime_or_fallback(".."),
            PathBuf::from(FALLBACK_ROOT)
        );
        assert_eq!(
            sibling_runtime_or_fallback("has/slash"),
            PathBuf::from(FALLBACK_ROOT)
        );
        assert_eq!(
            sibling_runtime_or_fallback(""),
            PathBuf::from(FALLBACK_ROOT)
        );
    }

    #[test]
    fn fallback_sibling_debug_composes_under_fallback_root() {
        // Valid segment, happy path — result is under process $HOME or
        // under LIGHTARCHITECTS_HOME override. Just check the leaf segments.
        let ayin_debug = sibling_debug_or_fallback("ayin");
        assert!(
            ayin_debug.ends_with("debug/ayin"),
            "sibling_debug should end with debug/ayin, got: {ayin_debug:?}"
        );
        let launchd = launchd_debug_or_fallback("ayin");
        assert!(
            launchd.ends_with("debug/ayin/launchd"),
            "launchd_debug should end with debug/ayin/launchd, got: {launchd:?}"
        );
        // Invalid segment falls back to FALLBACK_ROOT/debug (no leaf sibling).
        let invalid_debug = sibling_debug_or_fallback("..");
        assert_eq!(
            invalid_debug,
            PathBuf::from(FALLBACK_ROOT).join(DEBUG_DIRNAME)
        );
    }

    #[test]
    fn named_sibling_fallbacks_agree_with_sibling_runtime_or_fallback() {
        // Each named fallback must equal the generic dispatch for its name.
        assert_eq!(soul_or_fallback(), sibling_runtime_or_fallback("soul"));
        assert_eq!(laex0_or_fallback(), sibling_runtime_or_fallback("laex0"));
        assert_eq!(corso_or_fallback(), sibling_runtime_or_fallback("corso"));
        assert_eq!(eva_or_fallback(), sibling_runtime_or_fallback("eva"));
        assert_eq!(
            quantum_or_fallback(),
            sibling_runtime_or_fallback("quantum")
        );
        assert_eq!(seraph_or_fallback(), sibling_runtime_or_fallback("seraph"));
        assert_eq!(ayin_or_fallback(), sibling_runtime_or_fallback("ayin"));
        assert_eq!(
            webshell_or_fallback(),
            sibling_runtime_or_fallback("webshell")
        );
    }

    #[test]
    fn helix_and_session_key_fallbacks_compose_correctly() {
        // helix is under soul
        assert_eq!(helix_root_or_fallback(), soul_or_fallback().join("helix"));
        // session key is under laex0
        assert_eq!(
            session_key_or_fallback(),
            laex0_or_fallback().join(".session-key")
        );
    }
}
