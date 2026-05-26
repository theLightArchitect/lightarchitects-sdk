//! `helix.toml` filesystem marker — declares a directory as a helix root with a scope tier.
//!
//! A `helix.toml` file placed in any directory signals that directory is a helix root.
//! The file declares the helix's name, scope tier, and optional publishing metadata.
//!
//! # Format
//!
//! ```toml
//! [helix]
//! name = "platform"
//! scope_tier = "platform"     # platform | user | project | shared
//! schema_version = 1
//! publish = false              # optional, default false
//! platform_helix_version = "1.0.0"  # optional
//! ```
//!
//! # Usage
//!
//! ```rust,no_run
//! use std::path::Path;
//! use lightarchitects::helix::helix_toml::{find_helix_root, load_helix_toml};
//!
//! if let Some((root, toml)) = find_helix_root(Path::new("/some/nested/dir")) {
//!     println!("helix root at {:?}, tier={}", root, toml.scope_tier());
//! }
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::helix::types::ScopeTier;

/// Maximum directory levels to walk upward when searching for a `helix.toml` marker.
///
/// Value 7 mirrors [`MAX_TRAVERSAL_DEPTH`](crate::helix::types::MAX_TRAVERSAL_DEPTH) —
/// covers all practical vault nesting depths with margin.
const MAX_FS_HELIX_DEPTH: usize = 7;

// ============================================================================
// Structs
// ============================================================================

/// Contents of the `[helix]` section in a `helix.toml` marker file.
///
/// Unknown keys in `[helix]` are rejected at parse time (`deny_unknown_fields`)
/// so that typos in vault markers fail loudly rather than silently falling back
/// to defaults.
///
/// # Forward-compatibility hazard
///
/// Because `deny_unknown_fields` is in effect, adding a new field to `helix.toml`
/// before the matching code is deployed causes `load_helix_toml` to return `None`.
/// This propagates as "no helix root found" in `find_helix_root`, which makes
/// `check_helix_writeable` **fail open** on platform-tier directories — writes that
/// should be rejected will silently succeed.
///
/// Mitigation: bump `schema_version` when adding fields, and gate new-field reads
/// on `schema_version >= N`. Never deploy a `helix.toml` with new keys before the
/// code that recognises them is live.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelixTomlSection {
    /// Human-readable name for this helix root (e.g. `"platform"`, `"my-project"`).
    pub name: String,
    /// Scope tier string — one of `"platform"`, `"user"`, `"project"`, `"shared"`.
    ///
    /// Parsed into [`ScopeTier`] via [`HelixToml::scope_tier`].
    /// Unknown values fall back to [`ScopeTier::User`].
    pub scope_tier: String,
    /// Schema version for forward-compatibility checks.
    pub schema_version: u32,
    /// Whether this helix root is published for platform-wide distribution.
    ///
    /// Absent from the file is treated as `false`.
    #[serde(default)]
    pub publish: bool,
    /// Platform helix bundle version (semver string), if applicable.
    ///
    /// Only meaningful when `scope_tier = "platform"`.
    pub platform_helix_version: Option<String>,
}

/// Parsed representation of a `helix.toml` marker file.
///
/// The TOML file must contain a `[helix]` section; all other keys are ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelixToml {
    /// The `[helix]` section parsed from the file.
    pub helix: HelixTomlSection,
}

impl HelixToml {
    /// Parse `self.helix.scope_tier` string into a [`ScopeTier`] enum value.
    ///
    /// Falls back to [`ScopeTier::User`] for any unrecognised string so that
    /// forward-compatible vault layouts degrade gracefully.
    #[must_use]
    pub fn scope_tier(&self) -> ScopeTier {
        match self.helix.scope_tier.as_str() {
            "platform" => ScopeTier::Platform,
            "project" => ScopeTier::Project,
            "shared" => ScopeTier::Shared,
            // "user" and unknown strings both map to the default User tier.
            _ => ScopeTier::User,
        }
    }
}

// ============================================================================
// Public functions
// ============================================================================

/// Read and parse `{dir}/helix.toml`.
///
/// Returns `None` on any error: missing file, unreadable bytes, or TOML parse
/// failure. Callers should treat `None` as "this directory is not a helix root".
#[must_use]
pub fn load_helix_toml(dir: &Path) -> Option<HelixToml> {
    let path = dir.join("helix.toml");
    let contents = std::fs::read_to_string(&path).ok()?;
    toml::from_str::<HelixToml>(&contents).ok()
}

/// Walk up the directory tree from `start`, looking for a `helix.toml` marker.
///
/// Ascends at most [`MAX_FS_HELIX_DEPTH`] levels (including `start` itself).
/// Returns the first ancestor directory that contains a valid `helix.toml`
/// along with the parsed [`HelixToml`], or `None` if no marker is found within
/// the depth limit.
#[must_use]
pub fn find_helix_root(start: &Path) -> Option<(PathBuf, HelixToml)> {
    let mut current = start.to_path_buf();
    for _ in 0..MAX_FS_HELIX_DEPTH {
        if let Some(parsed) = load_helix_toml(&current) {
            return Some((current, parsed));
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }
    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn write_helix_toml(dir: &Path, content: &str) {
        std::fs::write(dir.join("helix.toml"), content).expect("write helix.toml");
    }

    #[test]
    fn scope_tier_parses_all_variants() {
        let cases = [
            ("platform", ScopeTier::Platform),
            ("user", ScopeTier::User),
            ("project", ScopeTier::Project),
            ("shared", ScopeTier::Shared),
        ];
        for (raw, expected) in cases {
            let ht = HelixToml {
                helix: HelixTomlSection {
                    name: "test".into(),
                    scope_tier: raw.into(),
                    schema_version: 1,
                    publish: false,
                    platform_helix_version: None,
                },
            };
            assert_eq!(ht.scope_tier(), expected, "failed for {raw}");
        }
    }

    #[test]
    fn scope_tier_unknown_falls_back_to_user() {
        let ht = HelixToml {
            helix: HelixTomlSection {
                name: "test".into(),
                scope_tier: "enterprise".into(),
                schema_version: 1,
                publish: false,
                platform_helix_version: None,
            },
        };
        assert_eq!(ht.scope_tier(), ScopeTier::User);
    }

    #[test]
    fn load_helix_toml_returns_none_for_missing_file() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        assert!(load_helix_toml(tmp.path()).is_none());
    }

    #[test]
    fn load_helix_toml_parses_valid_file() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        write_helix_toml(
            tmp.path(),
            r#"
[helix]
name = "platform"
scope_tier = "platform"
schema_version = 1
publish = false
platform_helix_version = "1.0.0"
"#,
        );
        let parsed = load_helix_toml(tmp.path()).expect("should parse");
        assert_eq!(parsed.helix.name, "platform");
        assert_eq!(parsed.scope_tier(), ScopeTier::Platform);
        assert_eq!(parsed.helix.schema_version, 1);
        assert!(!parsed.helix.publish);
        assert_eq!(
            parsed.helix.platform_helix_version.as_deref(),
            Some("1.0.0")
        );
    }

    #[test]
    fn load_helix_toml_returns_none_for_invalid_toml() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        write_helix_toml(tmp.path(), "not valid toml ][");
        assert!(load_helix_toml(tmp.path()).is_none());
    }

    #[test]
    fn load_helix_toml_returns_none_for_missing_helix_section() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        write_helix_toml(tmp.path(), "[other]\nfoo = 1\n");
        assert!(load_helix_toml(tmp.path()).is_none());
    }

    #[test]
    fn find_helix_root_finds_marker_in_start_dir() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        write_helix_toml(
            tmp.path(),
            "[helix]\nname=\"root\"\nscope_tier=\"user\"\nschema_version=1\n",
        );
        let result = find_helix_root(tmp.path());
        assert!(result.is_some());
        let (found_path, found_toml) = result.expect("should find root");
        assert_eq!(found_path, tmp.path());
        assert_eq!(found_toml.scope_tier(), ScopeTier::User);
    }

    #[test]
    fn find_helix_root_walks_up_to_find_marker() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let nested = tmp.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&nested).expect("create nested dirs");
        write_helix_toml(
            tmp.path(),
            "[helix]\nname=\"root\"\nscope_tier=\"project\"\nschema_version=1\n",
        );
        let result = find_helix_root(&nested);
        assert!(result.is_some());
        let (found_path, found_toml) = result.expect("should find root");
        assert_eq!(found_path, tmp.path());
        assert_eq!(found_toml.scope_tier(), ScopeTier::Project);
    }

    #[test]
    fn publish_defaults_to_false_when_absent() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        write_helix_toml(
            tmp.path(),
            "[helix]\nname=\"x\"\nscope_tier=\"user\"\nschema_version=1\n",
        );
        let parsed = load_helix_toml(tmp.path()).expect("should parse");
        assert!(
            !parsed.helix.publish,
            "absent publish key must default to false"
        );
    }

    #[test]
    fn publish_true_is_parsed_correctly() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        write_helix_toml(
            tmp.path(),
            "[helix]\nname=\"x\"\nscope_tier=\"shared\"\nschema_version=1\npublish=true\n",
        );
        let parsed = load_helix_toml(tmp.path()).expect("should parse");
        assert!(parsed.helix.publish);
    }

    #[test]
    fn find_helix_root_returns_none_when_no_marker_exists() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        // No helix.toml anywhere in the tree
        assert!(find_helix_root(tmp.path()).is_none());
    }

    #[test]
    fn load_helix_toml_rejects_unknown_fields() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        // `unknown_key` is not in HelixTomlSection — must be rejected.
        write_helix_toml(
            tmp.path(),
            "[helix]\nname=\"x\"\nscope_tier=\"user\"\nschema_version=1\nunknown_key=\"bad\"\n",
        );
        assert!(
            load_helix_toml(tmp.path()).is_none(),
            "unknown fields must cause parse failure"
        );
    }

    /// Contract C2 — `HelixToml` serializes to TOML and parses back without loss.
    ///
    /// Regression guard for `#[derive(Serialize)]` on `HelixTomlSection`: if a
    /// field is accidentally removed or renamed, the round-trip assertion fails.
    #[test]
    fn helix_toml_serialize_round_trip() {
        let original = HelixToml {
            helix: HelixTomlSection {
                name: "test-vault".into(),
                scope_tier: "project".into(),
                schema_version: 2,
                publish: true,
                platform_helix_version: Some("3.1.0".into()),
            },
        };
        let serialized = toml::to_string(&original).expect("serialize HelixToml to TOML");
        let back: HelixToml = toml::from_str(&serialized).expect("parse HelixToml from TOML");
        assert_eq!(back.helix.name, original.helix.name);
        assert_eq!(back.helix.scope_tier, original.helix.scope_tier);
        assert_eq!(back.helix.schema_version, original.helix.schema_version);
        assert_eq!(back.helix.publish, original.helix.publish);
        assert_eq!(
            back.helix.platform_helix_version,
            original.helix.platform_helix_version
        );
    }

    #[test]
    fn find_helix_root_stops_at_depth_limit() {
        // Create MAX_FS_HELIX_DEPTH + 2 levels, place marker at the very top.
        let tmp = tempfile::tempdir().expect("tmpdir");
        let mut deepest = tmp.path().to_path_buf();
        for i in 0..(MAX_FS_HELIX_DEPTH + 2) {
            deepest = deepest.join(format!("lvl{i}"));
        }
        std::fs::create_dir_all(&deepest).expect("create deep dirs");
        write_helix_toml(
            tmp.path(),
            "[helix]\nname=\"deep\"\nscope_tier=\"shared\"\nschema_version=1\n",
        );
        // The marker is beyond the depth limit from `deepest` — should NOT find it.
        assert!(find_helix_root(&deepest).is_none());
    }

    /// Perf baseline: 1,000 sequential `load_helix_toml` calls must complete in <100 ms.
    ///
    /// Recorded baseline (2026-05-02, Apple M-series): ~2 ms for 1,000 iterations (~2 µs/call).
    /// SLA threshold: 250 ms (125× headroom). Chosen to tolerate parallel test suite load on
    /// macOS (observed ~130ms under full suite; 100ms threshold was too tight).
    #[test]
    fn load_helix_toml_perf_baseline_1000_iterations() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        write_helix_toml(
            tmp.path(),
            "[helix]\nname=\"perf\"\nscope_tier=\"user\"\nschema_version=1\n",
        );
        let start = std::time::Instant::now();
        for _ in 0..1_000 {
            let _ = load_helix_toml(tmp.path());
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 250,
            "load_helix_toml 1000× took {elapsed:?}, expected <250ms"
        );
    }
}
