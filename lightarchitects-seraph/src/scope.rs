//! Engagement scope management — Rust mirror of `~/.seraph/scope.toml`.
//!
//! Build an [`EngagementScope`], call [`EngagementScope::install`] to write it
//! to the expected path, then construct a [`crate::SeraphClient`].

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use lightarchitects_core::error::SdkError;

/// Default maximum concurrent scans when not specified.
const fn default_max_concurrent() -> u8 {
    3
}

// ── EngagementScope ─────────────────────────────────────────────────────────

/// Rust representation of `~/.seraph/scope.toml`.
///
/// Build a scope, call [`EngagementScope::install`] to write it to the
/// expected path, then construct a [`crate::SeraphClient`].
///
/// # Example
///
/// ```no_run
/// use chrono::Utc;
/// use lightarchitects_seraph::scope::EngagementScope;
///
/// let scope = EngagementScope {
///     engagement_id: "ENG-001".into(),
///     targets: vec!["192.168.1.0/24".into()],
///     authorized_tools: vec!["nmap".into(), "tshark".into()],
///     expires_at: Utc::now() + chrono::Duration::hours(8),
///     hitl_required: false,
///     authorized_by: "kevin".into(),
///     max_concurrent_scans: 3,
/// };
/// scope.install().unwrap();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngagementScope {
    /// Unique engagement identifier -- logged with every tool invocation.
    pub engagement_id: String,
    /// Authorised network targets (CIDR or host notation).
    pub targets: Vec<String>,
    /// Allowlist of tool names permitted for this engagement.
    pub authorized_tools: Vec<String>,
    /// Engagement expiry timestamp (ISO 8601 / RFC 3339).
    pub expires_at: DateTime<Utc>,
    /// Whether human-in-the-loop confirmation is required before execution.
    pub hitl_required: bool,
    /// Name of the authorising individual (audit trail).
    pub authorized_by: String,
    /// Maximum number of concurrent scans permitted.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_scans: u8,
}

impl EngagementScope {
    /// Serialize to TOML suitable for writing to `~/.seraph/scope.toml`.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if serialization fails (should not
    /// happen with a well-formed `EngagementScope`).
    pub fn to_toml(&self) -> Result<String, SdkError> {
        toml::to_string(self)
            .map_err(|e| SdkError::Config(format!("failed to serialize scope to TOML: {e}")))
    }

    /// Write the scope to `~/.seraph/scope.toml`, creating the directory if
    /// needed.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] on file-system errors or if `$HOME` is
    /// not set. Returns [`SdkError::Config`] if TOML serialization fails.
    pub fn install(&self) -> Result<PathBuf, SdkError> {
        let path = scope_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                SdkError::Config(format!(
                    "failed to create scope dir {}: {e}",
                    parent.display()
                ))
            })?;
        }
        let toml = self.to_toml()?;
        std::fs::write(&path, toml).map_err(|e| {
            SdkError::Config(format!(
                "failed to write scope file {}: {e}",
                path.display()
            ))
        })?;
        Ok(path)
    }
}

/// Resolve `~/.seraph/scope.toml`.
fn scope_path() -> Result<PathBuf, SdkError> {
    let home = std::env::var("HOME")
        .map_err(|_| SdkError::Config("HOME environment variable not set".to_owned()))?;
    Ok(PathBuf::from(home).join(".seraph").join("scope.toml"))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn test_scope() -> EngagementScope {
        EngagementScope {
            engagement_id: "ENG-TEST-001".into(),
            targets: vec!["192.168.1.0/24".into(), "10.0.0.1".into()],
            authorized_tools: vec!["nmap".into(), "tshark".into()],
            expires_at: Utc::now() + Duration::hours(4),
            hitl_required: false,
            authorized_by: "kevin".into(),
            max_concurrent_scans: 3,
        }
    }

    #[test]
    fn to_toml_produces_valid_toml() {
        let scope = test_scope();
        let toml_str = scope.to_toml().unwrap();
        assert!(toml_str.contains("engagement_id = \"ENG-TEST-001\""));
        assert!(toml_str.contains("authorized_by = \"kevin\""));
        assert!(toml_str.contains("hitl_required = false"));
        assert!(toml_str.contains("max_concurrent_scans = 3"));
    }

    #[test]
    fn to_toml_roundtrip() {
        let scope = test_scope();
        let toml_str = scope.to_toml().unwrap();
        let parsed: EngagementScope = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.engagement_id, scope.engagement_id);
        assert_eq!(parsed.targets, scope.targets);
        assert_eq!(parsed.authorized_tools, scope.authorized_tools);
        assert_eq!(parsed.hitl_required, scope.hitl_required);
        assert_eq!(parsed.authorized_by, scope.authorized_by);
        assert_eq!(parsed.max_concurrent_scans, scope.max_concurrent_scans);
    }

    #[test]
    fn install_creates_file() {
        let temp = tempfile::tempdir().unwrap();
        // SAFETY: test-only; tests run with `--test-threads=1` or accept the race.
        unsafe { std::env::set_var("HOME", temp.path()) };

        let scope = test_scope();
        let path = scope.install().unwrap();
        assert!(path.exists(), "scope.toml should exist at {path:?}");

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("engagement_id = \"ENG-TEST-001\""));
    }

    #[test]
    fn default_max_concurrent_is_3() {
        // Verify the default function returns 3.
        assert_eq!(default_max_concurrent(), 3);
    }

    #[test]
    fn toml_deserialize_missing_max_concurrent_uses_default() {
        let toml_str = r#"
engagement_id = "ENG-X"
targets = ["10.0.0.1"]
authorized_tools = ["nmap"]
expires_at = "2030-01-01T00:00:00Z"
hitl_required = true
authorized_by = "tester"
"#;
        let parsed: EngagementScope = toml::from_str(toml_str).unwrap();
        assert_eq!(parsed.max_concurrent_scans, 3);
    }
}
