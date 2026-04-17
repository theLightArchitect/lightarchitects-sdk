//! Guardrails — inter-iteration learning from failures.
//!
//! After each task failure, the conductor appends a "Sign" to `guardrails.md`.
//! Before each task execution, guardrails are read and injected into the prompt.
//! This prevents the same mistake across context rotations.
//!
//! Gutter detection: if the last N error signatures match (same error repeated),
//! the task is "guttered" — stuck in a loop. The conductor should skip it.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Maximum error signatures to track per task for gutter detection.
const GUTTER_THRESHOLD: usize = 3;

/// A guardrail "Sign" — a learned lesson from a failure.
#[derive(Debug)]
pub struct Sign {
    /// Which task produced this sign.
    pub task_id: String,
    /// What went wrong.
    pub failure: String,
    /// What to do differently next time.
    pub instruction: String,
    /// When this sign was added.
    pub added: String,
}

/// Read the current guardrails file. Returns empty string if absent.
///
/// # Errors
///
/// Returns an error only on non-`NotFound` IO errors.
pub fn read_guardrails(path: &Path) -> Result<String, std::io::Error> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

/// Append a new Sign to the guardrails file.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub fn append_sign(path: &Path, sign: &Sign) -> Result<(), std::io::Error> {
    use std::io::Write as _;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    writeln!(file)?;
    writeln!(file, "### Sign: {}", sign.instruction)?;
    writeln!(file, "- **Task**: {}", sign.task_id)?;
    writeln!(file, "- **Failure**: {}", sign.failure)?;
    writeln!(file, "- **Added**: {}", sign.added)?;
    Ok(())
}

/// Extract an error signature from the last N lines of a log file.
///
/// The signature is a hash of the last few non-empty lines, used for
/// gutter detection (same error repeating = stuck).
pub fn error_signature(log_path: &Path, tail_lines: usize) -> u64 {
    let content = std::fs::read_to_string(log_path).unwrap_or_default();
    let lines: Vec<&str> = content
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(tail_lines)
        .collect();

    let mut hasher = DefaultHasher::new();
    for line in &lines {
        line.hash(&mut hasher);
    }
    hasher.finish()
}

/// Check if a task is guttered (stuck repeating the same error).
///
/// Returns `true` if the last `GUTTER_THRESHOLD` error signatures are identical.
pub fn is_guttered(signatures: &[u64]) -> bool {
    if signatures.len() < GUTTER_THRESHOLD {
        return false;
    }
    let last = signatures.last().copied().unwrap_or(0);
    signatures
        .iter()
        .rev()
        .take(GUTTER_THRESHOLD)
        .all(|&s| s == last)
}

/// Extract a concise failure summary from the last lines of a log file.
///
/// Returns the last `n` non-empty lines joined, capped at 500 chars.
pub fn failure_summary(log_path: &Path, n: usize) -> String {
    let content = std::fs::read_to_string(log_path).unwrap_or_default();
    let lines: Vec<&str> = content
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(n)
        .collect();

    let mut summary: String = lines.into_iter().rev().collect::<Vec<_>>().join("\n");
    summary.truncate(500);
    summary
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn gutter_detected_when_same_signature_repeats() {
        let sigs = vec![42, 42, 42];
        assert!(is_guttered(&sigs));
    }

    #[test]
    fn gutter_not_detected_when_signatures_differ() {
        let sigs = vec![1, 2, 3];
        assert!(!is_guttered(&sigs));
    }

    #[test]
    fn gutter_not_detected_with_insufficient_data() {
        let sigs = vec![42, 42];
        assert!(!is_guttered(&sigs));
    }

    #[test]
    fn read_guardrails_returns_empty_for_missing_file() {
        let result = read_guardrails(Path::new("/nonexistent/guardrails.md"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn append_sign_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("guardrails.md");

        let sign = Sign {
            task_id: "test-001".into(),
            failure: "compilation error".into(),
            instruction: "Check types first".into(),
            added: "2026-03-29".into(),
        };

        append_sign(&path, &sign).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Check types first"));
        assert!(content.contains("test-001"));
    }

    #[test]
    fn append_sign_appends_to_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("guardrails.md");
        std::fs::write(&path, "# Existing guardrails\n").unwrap();

        let sign = Sign {
            task_id: "test-002".into(),
            failure: "timeout".into(),
            instruction: "Reduce scope".into(),
            added: "2026-03-29".into(),
        };

        append_sign(&path, &sign).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("# Existing guardrails"));
        assert!(content.contains("Reduce scope"));
    }

    #[test]
    fn read_guardrails_reads_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("guardrails.md");
        std::fs::write(&path, "### Sign: Do not use unwrap").unwrap();

        let content = read_guardrails(&path).unwrap();
        assert!(content.contains("Do not use unwrap"));
    }

    #[test]
    fn error_signature_produces_consistent_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.txt");
        std::fs::write(&path, "line 1\nline 2\nerror: something failed\n").unwrap();

        let sig1 = error_signature(&path, 3);
        let sig2 = error_signature(&path, 3);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn error_signature_differs_for_different_logs() {
        let dir = tempfile::tempdir().unwrap();

        let path1 = dir.path().join("log1.txt");
        std::fs::write(&path1, "error: type mismatch\n").unwrap();

        let path2 = dir.path().join("log2.txt");
        std::fs::write(&path2, "error: not found\n").unwrap();

        assert_ne!(error_signature(&path1, 3), error_signature(&path2, 3));
    }

    #[test]
    fn error_signature_handles_missing_file() {
        let sig = error_signature(Path::new("/nonexistent/log.txt"), 5);
        // Should not panic — returns a hash of empty content
        assert_eq!(sig, error_signature(Path::new("/other/missing.txt"), 5));
    }

    #[test]
    fn failure_summary_truncates_at_500_chars() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.txt");
        let long_line = "x".repeat(600);
        std::fs::write(&path, &long_line).unwrap();

        let summary = failure_summary(&path, 3);
        assert!(summary.len() <= 500);
    }

    #[test]
    fn gutter_detected_with_more_than_threshold() {
        let sigs = vec![10, 20, 42, 42, 42];
        assert!(is_guttered(&sigs));
    }

    #[test]
    fn gutter_not_detected_when_only_last_two_match() {
        let sigs = vec![1, 42, 42];
        assert!(!is_guttered(&sigs));
    }
}
