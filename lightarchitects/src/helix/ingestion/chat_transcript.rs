//! Chat transcript ingestor — parses `chat-{date}.md` or `transcript-{date}.md` files.
//!
//! Each conversational turn becomes a Step, with the speaker assigned as a strand.
//! When a turn contains preference expressions, a companion preference atom is created
//! and tagged to both the speaker strand and a `"preference"` strand — enabling
//! strand-aware retrieval to find preference content without a separate collection.
//!
//! This is domain-agnostic: the preference triggers apply to any speaker, and any
//! domain can define its own strand vocabulary alongside the auto-detected `"preference"` strand.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use tracing::instrument;

use crate::helix::db::HelixDb;
use crate::helix::types::{HelixOrderingMode, ScopeTier, Step, StrandMembership};

use super::{IngestionError, IngestionReport, IngestionSource};

// ============================================================================
// ChatTranscriptIngester
// ============================================================================

/// Ingests chat transcript files into the helix graph.
///
/// Expects files named `chat-YYYY-MM-DD.md` or `transcript-YYYY-MM-DD.md`.
/// Each turn (marked by `**Speaker:**` prefix) becomes a Step.
/// Speaker names become strands.
pub struct ChatTranscriptIngester {
    /// Path to the transcript file.
    path: PathBuf,
    /// Owner/sibling name for the helix.
    owner: String,
}

impl ChatTranscriptIngester {
    /// Create a new chat transcript ingestor.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, owner: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            owner: owner.into(),
        }
    }

    /// Extract the date from a transcript filename.
    #[must_use]
    pub fn date_from_filename(path: &Path) -> Option<NaiveDate> {
        let stem = path.file_stem()?.to_string_lossy();
        // Try patterns: chat-YYYY-MM-DD, transcript-YYYY-MM-DD
        let date_part = stem
            .strip_prefix("chat-")
            .or_else(|| stem.strip_prefix("transcript-"))?;
        NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok()
    }

    /// Parse transcript into (speaker, content) turns.
    #[must_use]
    pub fn parse_turns(content: &str) -> Vec<(String, String)> {
        let mut turns = Vec::new();
        let mut current_speaker = String::new();
        let mut current_content = String::new();

        for line in content.lines() {
            if let Some(speaker) = extract_speaker(line) {
                if !current_speaker.is_empty() {
                    turns.push((current_speaker.clone(), current_content.trim().to_owned()));
                }
                current_speaker = speaker;
                // Content after the speaker prefix on the same line
                let after_prefix = line.find(":**").map_or("", |i| &line[i + 3..]).trim();
                after_prefix.clone_into(&mut current_content);
                current_content.push('\n');
            } else if !current_speaker.is_empty() {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }
        if !current_speaker.is_empty() && !current_content.trim().is_empty() {
            turns.push((current_speaker, current_content.trim().to_owned()));
        }
        turns
    }
}

#[async_trait]
impl IngestionSource for ChatTranscriptIngester {
    fn name(&self) -> &'static str {
        "ChatTranscript"
    }

    #[instrument(skip(self, db))]
    async fn ingest(&self, db: &dyn HelixDb) -> Result<IngestionReport, IngestionError> {
        if !self.path.exists() {
            return Err(IngestionError::SourceNotFound(
                self.path.display().to_string(),
            ));
        }

        let content = tokio::fs::read_to_string(&self.path).await?;
        let step_date = Self::date_from_filename(&self.path);

        let helix_name = format!("{}-transcripts", self.owner);
        let helix_id = db
            .ensure_helix(
                &self.owner,
                &helix_name,
                HelixOrderingMode::Temporal,
                ScopeTier::User,
            )
            .await
            .map_err(|e| IngestionError::Parse(format!("ensure_helix: {e}")))?;

        let mut report = IngestionReport::default();
        let turns = Self::parse_turns(&content);

        for (idx, (speaker, turn_content)) in turns.iter().enumerate() {
            let step = Step {
                id: uuid::Uuid::new_v4().to_string(),
                helix_id: helix_id.clone(),
                title: Some(format!("{speaker} (turn {})", idx + 1)),
                content: turn_content.clone(),
                significance: 3.0,
                step_date,
                step_index: i64::try_from(idx).ok(),
                community_id: None,
                expires: None,
                created_at: Utc::now(),
                metadata: serde_json::json!({
                    "speaker": speaker,
                    "turn": idx + 1,
                    "source_type": "chat_transcript",
                }),
                vault_path: None,
            };

            let (step_id, was_created) = match db.upsert_step(&step).await {
                Ok(r) => r,
                Err(e) => {
                    report.errors.push(format!("turn {}: {e}", idx + 1));
                    continue;
                }
            };

            if was_created {
                report.records_added += 1;
            } else {
                report.records_skipped += 1;
                continue;
            }

            // Speaker → strand assignment
            let strand_result = db.ensure_strand(&helix_id, speaker).await;
            if let Ok(ref strand_id) = strand_result {
                let membership = StrandMembership {
                    step_id: step_id.clone(),
                    strand_id: strand_id.clone(),
                    weight: 1.0,
                };
                if let Err(e) = db.assign_to_strand(&membership).await {
                    report.errors.push(format!("strand assign: {e}"));
                }
            }

            // Preference extraction — create a companion preference atom.
            //
            // When this turn contains preference expressions, we create a
            // separate Step whose content is *only* the preference text.
            // This step is tagged to both the speaker strand and a generic
            // "preference" strand — enabling strand_affinity queries to find
            // it without diluting it with the full turn content.
            if let Some(pref_text) = extract_preferences(turn_content) {
                let pref_step = Step {
                    id: uuid::Uuid::new_v4().to_string(),
                    helix_id: helix_id.clone(),
                    title: Some(format!("{speaker} preference (turn {})", idx + 1)),
                    content: format!("User preference: {pref_text}"),
                    significance: 7.0, // higher significance — dense signal
                    step_date,
                    step_index: None,
                    community_id: None,
                    expires: None,
                    created_at: Utc::now(),
                    metadata: serde_json::json!({
                        "speaker": speaker,
                        "turn": idx + 1,
                        "source_type": "preference_extraction",
                        "parent_step_id": step_id,
                    }),
                    vault_path: None,
                };

                match db.upsert_step(&pref_step).await {
                    Ok((pref_id, true)) => {
                        report.records_added += 1;

                        // Tag to "preference" strand (domain-agnostic name).
                        let pref_strand = db.ensure_strand(&helix_id, "preference").await;
                        if let Ok(pref_strand_id) = pref_strand {
                            for strand_id in [pref_strand_id].into_iter().chain(strand_result.ok())
                            {
                                let _ = db
                                    .assign_to_strand(&StrandMembership {
                                        step_id: pref_id.clone(),
                                        strand_id,
                                        weight: 1.0,
                                    })
                                    .await;
                            }
                        }
                    }
                    Ok((_, false)) => report.records_skipped += 1,
                    Err(e) => report.errors.push(format!("pref atom: {e}")),
                }
            }
        }

        Ok(report)
    }
}

// ============================================================================
// Preference extraction
// ============================================================================

/// Trigger phrases that signal a preference, habit, or recurring interest.
///
/// Domain-agnostic: these phrases appear in natural conversation across many
/// industries (healthcare: "I usually take", e-commerce: "I prefer", etc.).
const PREFERENCE_TRIGGERS: &[&str] = &[
    "i prefer ",
    "i usually ",
    "i like ",
    "i enjoy ",
    "i love ",
    "i hate ",
    "i don't like ",
    "i always ",
    "i want to ",
    "i'm looking for ",
    "i've been having trouble with ",
    "i've been feeling ",
    "i've been struggling with ",
    "i've been working on ",
    "i've been thinking about ",
    "i've been considering ",
    "i've been interested in ",
    "i still remember ",
    "i used to ",
];

/// Extract preference phrases from turn content.
///
/// Returns `None` if no preferences are found, or a semicolon-separated
/// summary string of up to 10 extracted phrases.
#[must_use]
pub fn extract_preferences(content: &str) -> Option<String> {
    let lower = content.to_lowercase();
    let mut found: Vec<String> = Vec::new();

    for trigger in PREFERENCE_TRIGGERS {
        let Some(pos) = lower.find(trigger) else {
            continue;
        };
        let rest = &lower[pos + trigger.len()..];
        let end = rest
            .find(['.', '!', '?', ',', '\n'])
            .unwrap_or_else(|| rest.len().min(80));
        let pref = rest[..end].trim();
        if pref.len() >= 5 {
            let entry = format!("{}{}", trigger.trim_end(), pref);
            if !found.contains(&entry) {
                found.push(entry);
            }
        }
    }

    if found.is_empty() {
        return None;
    }
    found.truncate(10);
    Some(found.join("; "))
}

// ============================================================================
// Helpers
// ============================================================================

/// Extract speaker name from a turn line (e.g., `**EVA:**` → "EVA").
fn extract_speaker(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("**") && trimmed.contains(":**") {
        let end = trimmed.find(":**")?;
        let speaker = &trimmed[2..end];
        if !speaker.is_empty() {
            return Some(speaker.to_owned());
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

    #[test]
    fn test_extract_speaker() {
        assert_eq!(extract_speaker("**EVA:** Hello!"), Some("EVA".to_owned()));
        assert_eq!(
            extract_speaker("**CORSO:** Right then."),
            Some("CORSO".to_owned())
        );
        assert_eq!(
            extract_speaker("**Claude:** Processing."),
            Some("Claude".to_owned())
        );
        assert_eq!(extract_speaker("No speaker here."), None);
        assert_eq!(extract_speaker("**:** empty"), None);
    }

    #[test]
    fn test_parse_turns() {
        let content = "**EVA:** Hello there!\nSo good to see you.\n\n**CORSO:** Right then, mate.\nLet's get sorted.\n";
        let turns = ChatTranscriptIngester::parse_turns(content);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].0, "EVA");
        assert!(turns[0].1.contains("Hello there!"));
        assert_eq!(turns[1].0, "CORSO");
        assert!(turns[1].1.contains("Right then"));
    }

    #[test]
    fn test_date_from_filename() {
        let date = ChatTranscriptIngester::date_from_filename(Path::new("chat-2026-03-08.md"));
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 3, 8).unwrap()));

        let date =
            ChatTranscriptIngester::date_from_filename(Path::new("transcript-2026-01-15.md"));
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 1, 15).unwrap()));

        let date = ChatTranscriptIngester::date_from_filename(Path::new("random-file.md"));
        assert!(date.is_none());
    }

    #[test]
    fn test_no_turns() {
        let turns = ChatTranscriptIngester::parse_turns("Just regular text.");
        assert!(turns.is_empty());
    }

    #[test]
    fn test_new_ingestor() {
        let ing = ChatTranscriptIngester::new("/path/to/chat-2026-03-08.md", "eva");
        assert_eq!(ing.owner, "eva");
    }
}
