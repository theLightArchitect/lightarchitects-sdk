//! Write HITL operator decisions as helix entries under `corso/entries/`.
//!
//! Each approved or rejected HITL escalation is persisted as an Obsidian-
//! compatible markdown file so the decision audit trail survives process
//! restarts and is queryable via SOUL helix search.
//!
//! # Security
//!
//! The `escalation_nonce` (`UUIDv7`) is an anti-replay token — it is NEVER
//! written to the helix (CWE-209). Only operator-visible fields (`task_id`,
//! `build_id`, approved/rejected, `operator_reason`) are persisted.

use chrono::{DateTime, Utc};
use std::path::PathBuf;
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────────

/// One HITL decision record to be persisted in the helix.
#[derive(Debug, Clone)]
pub struct HitlDecisionRecord {
    /// Build that triggered the escalation.
    pub build_id: Uuid,
    /// Task that was escalated.
    pub task_id: String,
    /// `true` = operator approved; `false` = operator rejected.
    pub approved: bool,
    /// Free-text reason supplied by the operator (may be empty).
    pub operator_reason: Option<String>,
    /// Timestamp when the operator resolved the escalation.
    pub decided_at: DateTime<Utc>,
}

// ── Writer ────────────────────────────────────────────────────────────────────

/// Append a HITL decision to the CORSO helix entries directory.
///
/// The file is named `{date}-{build_id_short}-hitl-{verdict}.md` and follows
/// the standard helix frontmatter schema consumed by SOUL and the Obsidian vault.
///
/// # Errors
///
/// Returns an `io::Error` if the target directory cannot be created or the
/// file cannot be written.  Callers should log and continue — helix write
/// failures must not block the operator resolution response.
pub fn write_hitl_decision(
    helix_root: &std::path::Path,
    record: &HitlDecisionRecord,
) -> std::io::Result<()> {
    let entries_dir = helix_root.join("corso").join("entries");
    std::fs::create_dir_all(&entries_dir)?;

    let verdict = if record.approved {
        "approved"
    } else {
        "rejected"
    };
    let date_str = record.decided_at.format("%Y-%m-%d").to_string();
    let build_short = &record.build_id.to_string()[..8];
    let filename = format!("{date_str}-{build_short}-hitl-{verdict}.md");
    let path = entries_dir.join(&filename);

    let content = render_entry(record, verdict);
    std::fs::write(&path, content)?;
    tracing::info!(
        path = %path.display(),
        build_id = %record.build_id,
        task_id = %record.task_id,
        verdict,
        "helix_decision_writer: HITL decision persisted"
    );
    Ok(())
}

/// Strip characters that could escape a YAML double-quoted scalar.
///
/// Newlines terminate the quoted value; `"` without a backslash closes it.
/// Both allow injecting arbitrary YAML keys into the frontmatter.
fn sanitize_yaml_scalar(s: &str) -> String {
    s.replace(['\n', '\r', '\0'], " ").replace('"', "\\\"")
}

fn render_entry(record: &HitlDecisionRecord, verdict: &str) -> String {
    let safe_task_id = sanitize_yaml_scalar(&record.task_id);
    let title = format!(
        "HITL {}: task '{}' (build {})",
        if record.approved {
            "Approved"
        } else {
            "Rejected"
        },
        safe_task_id,
        &record.build_id.to_string()[..8],
    );
    let date_str = record.decided_at.format("%Y-%m-%d").to_string();
    let timestamp = record.decided_at.to_rfc3339();
    let reason_line = record
        .operator_reason
        .as_deref()
        .map(|r| format!("\noperator_reason: \"{}\"", sanitize_yaml_scalar(r)))
        .unwrap_or_default();
    let body_reason = record
        .operator_reason
        .as_deref()
        .map(|r| format!("\n**Operator note**: {}", sanitize_yaml_scalar(r)))
        .unwrap_or_default();

    format!(
        r#"---
sibling: corso
type: hitl_decision
date: {date_str}
title: "{title}"
strands: [security, compliance, autonomous-build]
resonance: [vigilance, accountability]
significance: 6.0
tags: [hitl, ironclaw, autonomous-e2e, {verdict}, hitl-{verdict}]
build_id: {build_id}
task_id: "{task_id}"
verdict: {verdict}
decided_at: {timestamp}{reason_line}
---

# {title}

Operator resolved an ironclaw HITL escalation at `{timestamp}`.

| Field | Value |
|-------|-------|
| Build | `{build_id}` |
| Task | `{task_id}` |
| Verdict | **{verdict_display}** |
| Decided | `{timestamp}` |
{body_reason}

> Security: anti-replay token omitted from this record (CWE-209 / SERAPH#3).
"#,
        date_str = date_str,
        title = title,
        build_id = record.build_id,
        task_id = safe_task_id,
        verdict = verdict,
        timestamp = timestamp,
        reason_line = reason_line,
        body_reason = body_reason,
        verdict_display = if record.approved {
            "Approved ✓"
        } else {
            "Rejected ✗"
        },
    )
}

// ── Helix root resolution ─────────────────────────────────────────────────────

/// Resolve the helix root path from the environment or a well-known default.
///
/// Checks `LA_HELIX_ROOT` env var first; falls back to
/// `$HOME/lightarchitects/soul/helix`.
#[must_use]
pub fn resolve_helix_root() -> PathBuf {
    if let Ok(root) = std::env::var("LA_HELIX_ROOT") {
        return PathBuf::from(root);
    }
    std::env::var("HOME").map_or_else(
        |_| PathBuf::from("/tmp/la-helix"),
        |h| {
            PathBuf::from(h)
                .join("lightarchitects")
                .join("soul")
                .join("helix")
        },
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn approved_record() -> HitlDecisionRecord {
        HitlDecisionRecord {
            build_id: Uuid::nil(),
            task_id: "task-build-rs".to_owned(),
            approved: true,
            operator_reason: Some("looks safe".to_owned()),
            decided_at: chrono::DateTime::parse_from_rfc3339("2026-05-30T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        }
    }

    #[test]
    fn write_creates_entry_file_in_corso_entries() {
        let tmp = TempDir::new().unwrap();
        let record = approved_record();
        write_hitl_decision(tmp.path(), &record).unwrap();
        let entries = std::fs::read_dir(tmp.path().join("corso").join("entries"))
            .unwrap()
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        let name = entries[0].file_name().into_string().unwrap();
        assert!(name.contains("hitl-approved"), "filename: {name}");
    }

    #[test]
    fn entry_contains_required_frontmatter_fields() {
        let tmp = TempDir::new().unwrap();
        let record = approved_record();
        write_hitl_decision(tmp.path(), &record).unwrap();
        let content = std::fs::read_to_string(
            tmp.path()
                .join("corso")
                .join("entries")
                .join("2026-05-30-00000000-hitl-approved.md"),
        )
        .unwrap();
        assert!(content.contains("sibling: corso"), "missing sibling");
        assert!(content.contains("type: hitl_decision"), "missing type");
        assert!(content.contains("verdict: approved"), "missing verdict");
        assert!(
            content.contains("task_id: \"task-build-rs\""),
            "missing task_id"
        );
        assert!(
            !content.contains("nonce"),
            "nonce must not appear in helix entry (CWE-209)"
        );
    }

    #[test]
    fn rejected_entry_has_rejected_verdict() {
        let tmp = TempDir::new().unwrap();
        let mut record = approved_record();
        record.approved = false;
        record.operator_reason = None;
        write_hitl_decision(tmp.path(), &record).unwrap();
        let entries_dir = tmp.path().join("corso").join("entries");
        let entry = std::fs::read_dir(&entries_dir)
            .unwrap()
            .find_map(Result::ok)
            .unwrap();
        let content = std::fs::read_to_string(entry.path()).unwrap();
        assert!(content.contains("verdict: rejected"));
        assert!(content.contains("hitl-rejected"));
    }

    #[test]
    fn operator_reason_included_when_present() {
        let tmp = TempDir::new().unwrap();
        let record = approved_record();
        write_hitl_decision(tmp.path(), &record).unwrap();
        let entries_dir = tmp.path().join("corso").join("entries");
        let entry = std::fs::read_dir(&entries_dir)
            .unwrap()
            .find_map(Result::ok)
            .unwrap();
        let content = std::fs::read_to_string(entry.path()).unwrap();
        assert!(
            content.contains("looks safe"),
            "operator_reason missing from body"
        );
    }

    #[test]
    fn render_entry_cwe209_nonce_absent() {
        let record = approved_record();
        let rendered = render_entry(&record, "approved");
        assert!(
            !rendered.contains("nonce"),
            "nonce must never appear in helix entry"
        );
        assert!(
            !rendered.contains("escalation_nonce"),
            "escalation_nonce must never appear"
        );
    }
}
