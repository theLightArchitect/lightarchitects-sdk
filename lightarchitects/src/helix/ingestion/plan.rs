//! Plan ingestor — reads CORSO `plan.md` files into the helix graph.
//!
//! Maps plans to helixes with indexed ordering:
//! - Root step: the plan itself
//! - Sub-helix: plan phases as drill-down sub-steps

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::Utc;
use tracing::instrument;

use crate::helix::db::HelixDb;
use crate::helix::types::{HelixOrderingMode, ScopeTier, Step};

use super::frontmatter;
use super::{IngestionError, IngestionReport, IngestionSource};

// ============================================================================
// PlanIngester
// ============================================================================

/// Ingests CORSO build plan files into the helix graph.
///
/// Creates a helix per plan with indexed ordering (phase sequence).
pub struct PlanIngester {
    /// Path to the plan file.
    plan_path: PathBuf,
    /// Owner (defaults to "corso").
    owner: String,
}

impl PlanIngester {
    /// Create a new plan ingestor.
    #[must_use]
    pub fn new(plan_path: impl Into<PathBuf>) -> Self {
        Self {
            plan_path: plan_path.into(),
            owner: "corso".to_owned(),
        }
    }

    /// Set the owner for the plan helix.
    #[must_use]
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = owner.into();
        self
    }

    /// Extract phase titles and their content from a plan body.
    #[must_use]
    pub fn extract_phases(body: &str) -> Vec<(String, String)> {
        let mut phases = Vec::new();
        let mut current_title = String::new();
        let mut current_content = String::new();

        for line in body.lines() {
            if line.starts_with("## Phase") || line.starts_with("## phase") {
                if !current_title.is_empty() {
                    phases.push((current_title.clone(), current_content.trim().to_owned()));
                }
                line.trim_start_matches('#')
                    .trim()
                    .clone_into(&mut current_title);
                current_content.clear();
            } else if !current_title.is_empty() {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }
        if !current_title.is_empty() {
            phases.push((current_title, current_content.trim().to_owned()));
        }
        phases
    }

    /// Derive significance from plan tier.
    fn tier_significance(fm: &frontmatter::Frontmatter) -> f64 {
        let tier = fm
            .extra
            .get("tier")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        match tier.to_uppercase().as_str() {
            "XLARGE" | "CRITICAL" => 9.0,
            "LARGE" => 8.0,
            "MEDIUM" => 7.0,
            "SMALL" => 6.5,
            "HOTFIX" => 6.0,
            _ => 5.0,
        }
    }
}

#[async_trait]
impl IngestionSource for PlanIngester {
    fn name(&self) -> &'static str {
        "Plan"
    }

    #[instrument(skip(self, db))]
    async fn ingest(&self, db: &dyn HelixDb) -> Result<IngestionReport, IngestionError> {
        if !self.plan_path.exists() {
            return Err(IngestionError::SourceNotFound(
                self.plan_path.display().to_string(),
            ));
        }

        let content = tokio::fs::read_to_string(&self.plan_path).await?;
        let (fm, body) = frontmatter::parse(&content);
        let mut report = IngestionReport::default();

        let plan_name = fm
            .extra
            .get("plan_id")
            .and_then(serde_json::Value::as_str)
            .map_or_else(|| plan_name_from_path(&self.plan_path), str::to_owned);

        let helix_id = db
            .ensure_helix(
                &self.owner,
                &plan_name,
                HelixOrderingMode::Indexed,
                ScopeTier::User,
            )
            .await
            .map_err(|e| IngestionError::Parse(format!("ensure_helix: {e}")))?;

        // Root step: the plan itself
        let root_step = Step {
            id: uuid::Uuid::new_v4().to_string(),
            helix_id: helix_id.clone(),
            title: fm.title.clone().or_else(|| Some(plan_name.clone())),
            content: body.to_owned(),
            significance: Self::tier_significance(&fm),
            step_date: None,
            step_index: Some(0),
            community_id: None,
            expires: None,
            created_at: Utc::now(),
            metadata: serde_json::json!({
                "source_type": "plan",
                "plan_path": self.plan_path.display().to_string(),
            }),
            vault_path: None,
            graph_embedding: None,
        };
        let (_, was_created) = db
            .upsert_step(&root_step)
            .await
            .map_err(|e| IngestionError::Parse(format!("upsert root step: {e}")))?;
        if was_created {
            report.records_added += 1;
        } else {
            report.records_skipped += 1;
        }

        // Phase sub-steps
        let phases = Self::extract_phases(body);
        for (idx, (title, phase_content)) in phases.iter().enumerate() {
            let phase_step = Step {
                id: uuid::Uuid::new_v4().to_string(),
                helix_id: helix_id.clone(),
                title: Some(title.clone()),
                content: phase_content.clone(),
                significance: Self::tier_significance(&fm) - 1.0,
                step_date: None,
                step_index: i64::try_from(idx + 1).ok(),
                community_id: None,
                expires: None,
                created_at: Utc::now(),
                metadata: serde_json::json!({"phase_number": idx + 1}),
                vault_path: None,
                graph_embedding: None,
            };
            match db.upsert_step(&phase_step).await {
                Ok((_, true)) => report.records_added += 1,
                Ok((_, false)) => report.records_skipped += 1,
                Err(e) => report.errors.push(format!("phase {}: {e}", idx + 1)),
            }
        }

        Ok(report)
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn plan_name_from_path(path: &Path) -> String {
    path.file_stem().map_or_else(
        || "unknown-plan".to_owned(),
        |s| s.to_string_lossy().into_owned(),
    )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_phases() {
        let body = "Some intro.\n\n## Phase 1: Research\nResearch stuff.\n\n## Phase 2: Build\nBuild stuff.\n";
        let phases = PlanIngester::extract_phases(body);
        assert_eq!(phases.len(), 2);
        assert_eq!(phases[0].0, "Phase 1: Research");
        assert!(phases[0].1.contains("Research stuff."));
        assert_eq!(phases[1].0, "Phase 2: Build");
    }

    #[test]
    fn test_tier_significance() {
        let mut fm = frontmatter::Frontmatter::default();
        fm.extra.insert("tier".into(), serde_json::json!("XLARGE"));
        assert!((PlanIngester::tier_significance(&fm) - 9.0).abs() < f64::EPSILON);

        fm.extra.insert("tier".into(), serde_json::json!("SMALL"));
        assert!((PlanIngester::tier_significance(&fm) - 6.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_plan_name_from_path() {
        assert_eq!(
            plan_name_from_path(Path::new("/builds/luminous-weaving-spider/plan.md")),
            "plan"
        );
    }

    #[test]
    fn test_new_plan_ingestor() {
        let ing = PlanIngester::new("/path/to/plan.md").with_owner("kevin");
        assert_eq!(ing.owner, "kevin");
    }
}
