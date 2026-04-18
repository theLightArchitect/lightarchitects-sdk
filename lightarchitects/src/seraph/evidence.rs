//! Evidence chain accumulator and engagement logging.
//!
//! [`EvidenceChain`] collects [`EvidenceEntry`] records during an engagement,
//! preserving chronological order for audit logging or QUANTUM handoff.
//!
//! [`engagement_log`] writes individual results to the evidence directory at
//! `~/lightarchitects/seraph/evidence/{scope_id}/{timestamp}-{wing}.json`.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::core::error::SdkError;

// ── EvidenceEntry ───────────────────────────────────────────────────────────

/// A single piece of evidence collected during an engagement.
///
/// This wraps the AI-generated response from a wing action with metadata
/// about when and how it was collected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEntry {
    /// Wing or action that produced this evidence (e.g. `"scan"`, `"osint"`).
    pub action: String,
    /// Target that was investigated.
    pub target: String,
    /// The full output from the SERAPH action.
    pub output: String,
    /// Wall-clock timestamp of when this entry was recorded.
    pub recorded_at: DateTime<Utc>,
    /// Optional scope ID from the active engagement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
}

impl EvidenceEntry {
    /// Create a new evidence entry for the given action and target.
    #[must_use]
    pub fn new(
        action: impl Into<String>,
        target: impl Into<String>,
        output: impl Into<String>,
    ) -> Self {
        Self {
            action: action.into(),
            target: target.into(),
            output: output.into(),
            recorded_at: Utc::now(),
            scope_id: None,
        }
    }

    /// Attach a scope ID to this entry.
    #[must_use]
    pub fn with_scope_id(mut self, scope_id: impl Into<String>) -> Self {
        self.scope_id = Some(scope_id.into());
        self
    }
}

// ── EvidenceChain ───────────────────────────────────────────────────────────

/// An ordered chain of [`EvidenceEntry`] records accumulated during an engagement.
///
/// Callers append entries as each wing action is called. At the end of the
/// engagement, [`EvidenceChain::to_report`] serializes the chain to JSON for
/// audit logging or QUANTUM handoff.
///
/// # Example
///
/// ```no_run
/// use lightarchitects::seraph::evidence::{EvidenceChain, EvidenceEntry};
///
/// let mut chain = EvidenceChain::new();
/// chain.append(EvidenceEntry::new("scan", "192.168.1.0/24", "12 hosts discovered"));
/// chain.append(EvidenceEntry::new("osint", "target.example.com", "DNS records found"));
///
/// assert_eq!(chain.len(), 2);
/// let report = chain.to_report().unwrap();
/// println!("{report}");
/// ```
#[derive(Debug, Default, Clone)]
pub struct EvidenceChain {
    entries: Vec<EvidenceEntry>,
}

impl EvidenceChain {
    /// Create an empty chain.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an evidence entry to the chain.
    pub fn append(&mut self, entry: EvidenceEntry) {
        self.entries.push(entry);
    }

    /// Number of entries in the chain.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` if no entries have been appended.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over entries in chronological order.
    pub fn iter(&self) -> impl Iterator<Item = &EvidenceEntry> {
        self.entries.iter()
    }

    /// Serialize the chain to a pretty-printed JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Serialization`] if serialization fails.
    pub fn to_report(&self) -> Result<String, SdkError> {
        Ok(serde_json::to_string_pretty(&self.entries)?)
    }
}

// ── Engagement logging ──────────────────────────────────────────────────────

/// Write an [`EvidenceEntry`] to the evidence directory.
///
/// Creates `~/lightarchitects/seraph/evidence/{scope_id}/` if it does not exist, then writes
/// `{recorded_at}-{action}.json` with the entry serialized as pretty JSON.
///
/// When no `scope_id` is set on the entry, falls back to `"unscoped"`.
///
/// # Errors
///
/// Returns [`SdkError::Config`] if `$HOME` is not set.
/// Returns [`SdkError::Serialization`] if JSON serialization fails.
/// Returns a transport error wrapping I/O if file-system operations fail.
pub fn engagement_log(entry: &EvidenceEntry) -> Result<PathBuf, SdkError> {
    let scope_id = entry.scope_id.as_deref().unwrap_or("unscoped");
    let dir = evidence_dir(scope_id)?;
    std::fs::create_dir_all(&dir).map_err(|e| {
        SdkError::Config(format!(
            "failed to create evidence dir {}: {e}",
            dir.display()
        ))
    })?;

    // Sanitize the RFC 3339 timestamp for use in a filename (colons -> hyphens).
    let ts = entry.recorded_at.format("%Y-%m-%dT%H-%M-%SZ").to_string();
    let filename = format!("{ts}-{}.json", entry.action);
    let path = dir.join(&filename);

    let json = serde_json::to_string_pretty(entry)?;
    std::fs::write(&path, json).map_err(|e| {
        SdkError::Config(format!(
            "failed to write evidence file {}: {e}",
            path.display()
        ))
    })?;
    Ok(path)
}

/// Resolve `~/lightarchitects/seraph/evidence/{scope_id}/`.
fn evidence_dir(scope_id: &str) -> Result<PathBuf, SdkError> {
    crate::core::paths::seraph()
        .map(|p| p.join("evidence").join(scope_id))
        .ok_or_else(|| SdkError::Config("HOME environment variable not set".to_owned()))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[allow(unsafe_code)]
mod tests {
    use super::*;

    #[test]
    fn evidence_entry_new() {
        let entry = EvidenceEntry::new("scan", "192.168.1.1", "found 3 hosts");
        assert_eq!(entry.action, "scan");
        assert_eq!(entry.target, "192.168.1.1");
        assert_eq!(entry.output, "found 3 hosts");
        assert!(entry.scope_id.is_none());
    }

    #[test]
    fn evidence_entry_with_scope() {
        let entry =
            EvidenceEntry::new("osint", "example.com", "DNS records").with_scope_id("ENG-001");
        assert_eq!(entry.scope_id.as_deref(), Some("ENG-001"));
    }

    #[test]
    fn chain_append_and_len() {
        let mut chain = EvidenceChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);

        chain.append(EvidenceEntry::new("scan", "target", "output1"));
        chain.append(EvidenceEntry::new("osint", "target", "output2"));
        assert_eq!(chain.len(), 2);
        assert!(!chain.is_empty());
    }

    #[test]
    fn chain_iter() {
        let mut chain = EvidenceChain::new();
        chain.append(EvidenceEntry::new("scan", "t1", "o1"));
        chain.append(EvidenceEntry::new("capture", "t2", "o2"));

        let actions: Vec<&str> = chain.iter().map(|e| e.action.as_str()).collect();
        assert_eq!(actions, vec!["scan", "capture"]);
    }

    #[test]
    fn chain_to_report() {
        let mut chain = EvidenceChain::new();
        chain.append(EvidenceEntry::new("scan", "target", "output"));
        let report = chain.to_report().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 1);
        assert_eq!(parsed[0]["action"], "scan");
    }

    #[test]
    fn engagement_log_creates_file() {
        let temp = tempfile::tempdir().unwrap();
        // Override HOME so evidence lands in a temp directory.
        // SAFETY: test-only; tests run with `--test-threads=1` or accept the race.
        unsafe { std::env::set_var("HOME", temp.path()) };

        let entry =
            EvidenceEntry::new("scan", "192.168.1.1", "scan output").with_scope_id("ENG-TEST-001");
        let path = engagement_log(&entry).unwrap();
        assert!(path.exists(), "evidence file should exist at {path:?}");

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["action"], "scan");
        assert_eq!(parsed["scope_id"], "ENG-TEST-001");
    }

    #[test]
    fn engagement_log_filename_contains_action() {
        let temp = tempfile::tempdir().unwrap();
        // SAFETY: test-only; tests run with `--test-threads=1` or accept the race.
        unsafe { std::env::set_var("HOME", temp.path()) };

        let entry = EvidenceEntry::new("monitor", "iface", "output").with_scope_id("ENG-TEST-002");
        let path = engagement_log(&entry).unwrap();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        assert!(
            name.ends_with("-monitor.json"),
            "filename should end with action: {name}"
        );
    }

    #[test]
    fn engagement_log_unscoped_fallback() {
        let temp = tempfile::tempdir().unwrap();
        // SAFETY: test-only; tests run with `--test-threads=1` or accept the race.
        unsafe { std::env::set_var("HOME", temp.path()) };

        let entry = EvidenceEntry::new("analyze", "/tmp/file", "output");
        let path = engagement_log(&entry).unwrap();
        // Should use "unscoped" directory.
        assert!(
            path.to_string_lossy().contains("unscoped"),
            "path should contain 'unscoped': {path:?}"
        );
    }
}
