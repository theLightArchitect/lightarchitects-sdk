//! Runtime squad registry — TOML-driven inventory of squad members.
//!
//! Loads `~/.lightarchitects/squad-registry.toml` at startup. On a parse error
//! or absent file the registry falls back to compiled-in defaults and emits a
//! `tracing::warn!` — it never propagates an error to the caller so the binary
//! can always start.
//!
//! ## Schema (`~/.lightarchitects/squad-registry.toml`)
//!
//! ```toml
//! [[squads]]
//! id        = "eva"
//! bin_path  = "eva/bin/eva"
//! helix_dir = "eva"
//! mcp_args  = ["mcp-server"]
//! ```
//!
//! `bin_path` is relative to `LA_HOME` (typically `~/.lightarchitects`).

use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;
use tracing::warn;

/// Error from [`SquadRegistry::validate_entry`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SquadRegistryError {
    /// An entry has an empty `id` field.
    #[error("squad entry has empty id")]
    EmptyId,
    /// An entry's `bin_path` contains a path-traversal component (`..`).
    #[error("squad entry bin_path contains path traversal: {0}")]
    PathTraversal(String),
}

/// A single registered squad member.
#[derive(Debug, Clone, Deserialize)]
pub struct SquadEntry {
    /// Unique identifier for this squad member (e.g. `"eva"`, `"corso"`).
    pub id: String,
    /// Binary path **relative to `LA_HOME`** (e.g. `"eva/bin/eva"`).
    pub bin_path: PathBuf,
    /// Helix directory name under the helix root (e.g. `"eva"`).
    pub helix_dir: String,
    /// Extra arguments passed before the binary's stdio mode flag (e.g. `["mcp-server"]`).
    #[serde(default)]
    pub mcp_args: Vec<String>,
}

/// TOML file structure for `squad-registry.toml`.
#[derive(Debug, Deserialize)]
struct SquadRegistryFile {
    #[serde(default, rename = "squads")]
    entries: Vec<SquadEntry>,
}

/// Inventory of configured squad members.
///
/// Loaded via [`SquadRegistry::load`]; falls back to [`SquadRegistry::default_entries`]
/// on any parse or I/O error.
#[derive(Debug, Clone)]
pub struct SquadRegistry {
    /// Ordered list of squad members.
    pub entries: Vec<SquadEntry>,
}

impl SquadRegistry {
    /// Load the registry from `{home}/squad-registry.toml`.
    ///
    /// Returns compiled defaults (with a `warn!`) on any I/O or parse failure.
    /// Hot-reload is **deferred** — this is a startup-once call.
    #[must_use]
    pub fn load(home: &Path) -> Self {
        let path = home.join("squad-registry.toml");
        match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str::<SquadRegistryFile>(&contents) {
                Ok(f) => Self { entries: f.entries },
                Err(e) => {
                    warn!("squad-registry.toml parse error: {e}; using compiled defaults");
                    Self {
                        entries: Self::default_entries(),
                    }
                }
            },
            Err(_) => Self {
                entries: Self::default_entries(),
            },
        }
    }

    /// Compiled-in fallback for all 6 canonical squad members.
    ///
    /// Paths are relative to `LA_HOME` (`~/.lightarchitects`).
    #[must_use]
    pub fn default_entries() -> Vec<SquadEntry> {
        vec![
            SquadEntry {
                id: "corso".to_string(),
                bin_path: PathBuf::from("corso/bin/corso"),
                helix_dir: "corso".to_string(),
                mcp_args: vec![],
            },
            SquadEntry {
                id: "eva".to_string(),
                bin_path: PathBuf::from("eva/bin/eva"),
                helix_dir: "eva".to_string(),
                mcp_args: vec!["mcp-server".to_string()],
            },
            SquadEntry {
                id: "soul".to_string(),
                bin_path: PathBuf::from("soul/.config/bin/soul"),
                helix_dir: "soul".to_string(),
                mcp_args: vec![],
            },
            SquadEntry {
                id: "quantum".to_string(),
                bin_path: PathBuf::from("quantum/bin/quantum-q"),
                helix_dir: "quantum".to_string(),
                mcp_args: vec![],
            },
            SquadEntry {
                id: "seraph".to_string(),
                bin_path: PathBuf::from("seraph/bin/seraph"),
                helix_dir: "seraph".to_string(),
                mcp_args: vec![],
            },
            SquadEntry {
                id: "ayin".to_string(),
                bin_path: PathBuf::from("ayin/bin/ayin"),
                helix_dir: "ayin".to_string(),
                mcp_args: vec![],
            },
        ]
    }

    /// Resolve the absolute binary path for an entry given `la_home`.
    #[must_use]
    pub fn resolve_bin_path(la_home: &Path, entry: &SquadEntry) -> PathBuf {
        la_home.join(&entry.bin_path)
    }

    /// Validate a squad entry for security and correctness.
    ///
    /// # Errors
    ///
    /// - [`SquadRegistryError::EmptyId`] if `entry.id` is empty.
    /// - [`SquadRegistryError::PathTraversal`] if `entry.bin_path` contains `..`.
    pub fn validate_entry(entry: &SquadEntry) -> Result<(), SquadRegistryError> {
        if entry.id.trim().is_empty() {
            return Err(SquadRegistryError::EmptyId);
        }
        for component in entry.bin_path.components() {
            if matches!(component, Component::ParentDir) {
                return Err(SquadRegistryError::PathTraversal(
                    entry.bin_path.display().to_string(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_toml(dir: &TempDir, contents: &str) {
        std::fs::write(dir.path().join("squad-registry.toml"), contents).unwrap();
    }

    // ── SquadRegistry::load() ─────────────────────────────────────────────────

    #[test]
    fn load_absent_file_returns_defaults() {
        let dir = TempDir::new().unwrap();
        let reg = SquadRegistry::load(dir.path());
        let ids: Vec<_> = reg.entries.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"eva"), "defaults must include eva");
        assert!(ids.contains(&"corso"), "defaults must include corso");
        assert!(ids.contains(&"soul"), "defaults must include soul");
    }

    #[test]
    fn load_valid_toml_parses_entries() {
        let dir = TempDir::new().unwrap();
        write_toml(
            &dir,
            r#"
[[squads]]
id        = "eva"
bin_path  = "eva/bin/eva"
helix_dir = "eva"
mcp_args  = ["mcp-server"]
"#,
        );
        let reg = SquadRegistry::load(dir.path());
        assert_eq!(reg.entries.len(), 1);
        assert_eq!(reg.entries[0].id, "eva");
        assert_eq!(reg.entries[0].bin_path, PathBuf::from("eva/bin/eva"));
        assert_eq!(reg.entries[0].mcp_args, vec!["mcp-server"]);
    }

    #[test]
    fn load_malformed_toml_falls_back_to_defaults() {
        let dir = TempDir::new().unwrap();
        write_toml(&dir, "[[squads\nthis is not valid toml {{");
        let reg = SquadRegistry::load(dir.path());
        // Must return defaults, not panic or Err
        assert!(!reg.entries.is_empty(), "fallback must be non-empty");
    }

    #[test]
    fn load_empty_squads_array_returns_empty_entries() {
        let dir = TempDir::new().unwrap();
        write_toml(&dir, "");
        let reg = SquadRegistry::load(dir.path());
        // Empty file is valid TOML — zero entries (not defaults)
        assert!(reg.entries.is_empty());
    }

    #[test]
    fn load_mcp_args_defaults_to_empty_when_absent() {
        let dir = TempDir::new().unwrap();
        write_toml(
            &dir,
            r#"
[[squads]]
id        = "soul"
bin_path  = "soul/.config/bin/soul"
helix_dir = "soul"
"#,
        );
        let reg = SquadRegistry::load(dir.path());
        assert_eq!(reg.entries.len(), 1);
        assert!(reg.entries[0].mcp_args.is_empty());
    }

    // ── SquadRegistry::default_entries() ─────────────────────────────────────

    #[test]
    fn default_entries_contains_all_six_siblings() {
        let defaults = SquadRegistry::default_entries();
        let ids: Vec<_> = defaults.iter().map(|e| e.id.as_str()).collect();
        for expected in ["corso", "eva", "soul", "quantum", "seraph", "ayin"] {
            assert!(ids.contains(&expected), "missing default entry: {expected}");
        }
    }

    #[test]
    fn default_entries_are_valid() {
        for entry in SquadRegistry::default_entries() {
            SquadRegistry::validate_entry(&entry)
                .unwrap_or_else(|e| panic!("default entry '{}' failed validation: {e}", entry.id));
        }
    }

    // ── SquadRegistry::validate_entry() ──────────────────────────────────────

    #[test]
    fn validate_entry_rejects_empty_id() {
        let entry = SquadEntry {
            id: String::new(),
            bin_path: PathBuf::from("eva/bin/eva"),
            helix_dir: "eva".to_string(),
            mcp_args: vec![],
        };
        assert_eq!(
            SquadRegistry::validate_entry(&entry),
            Err(SquadRegistryError::EmptyId)
        );
    }

    #[test]
    fn validate_entry_rejects_whitespace_only_id() {
        let entry = SquadEntry {
            id: "   ".to_string(),
            bin_path: PathBuf::from("eva/bin/eva"),
            helix_dir: "eva".to_string(),
            mcp_args: vec![],
        };
        assert_eq!(
            SquadRegistry::validate_entry(&entry),
            Err(SquadRegistryError::EmptyId)
        );
    }

    #[test]
    fn validate_entry_rejects_path_traversal() {
        let entry = SquadEntry {
            id: "evil".to_string(),
            bin_path: PathBuf::from("../etc/passwd"),
            helix_dir: "evil".to_string(),
            mcp_args: vec![],
        };
        assert!(matches!(
            SquadRegistry::validate_entry(&entry),
            Err(SquadRegistryError::PathTraversal(_))
        ));
    }

    #[test]
    fn validate_entry_rejects_nested_traversal() {
        let entry = SquadEntry {
            id: "evil".to_string(),
            bin_path: PathBuf::from("legit/../../secret"),
            helix_dir: "evil".to_string(),
            mcp_args: vec![],
        };
        assert!(matches!(
            SquadRegistry::validate_entry(&entry),
            Err(SquadRegistryError::PathTraversal(_))
        ));
    }

    #[test]
    fn validate_entry_accepts_valid_entry() {
        let entry = SquadEntry {
            id: "eva".to_string(),
            bin_path: PathBuf::from("eva/bin/eva"),
            helix_dir: "eva".to_string(),
            mcp_args: vec!["mcp-server".to_string()],
        };
        assert!(SquadRegistry::validate_entry(&entry).is_ok());
    }

    // ── SquadRegistry::resolve_bin_path() ────────────────────────────────────

    #[test]
    fn resolve_bin_path_joins_to_la_home() {
        let home = Path::new("/Users/kft/.lightarchitects");
        let entry = SquadEntry {
            id: "eva".to_string(),
            bin_path: PathBuf::from("eva/bin/eva"),
            helix_dir: "eva".to_string(),
            mcp_args: vec![],
        };
        let resolved = SquadRegistry::resolve_bin_path(home, &entry);
        assert_eq!(
            resolved,
            PathBuf::from("/Users/kft/.lightarchitects/eva/bin/eva")
        );
    }
}
