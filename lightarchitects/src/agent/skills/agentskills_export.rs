//! Export Light Architects skills to the agentskills.io JSON format.
//!
//! Parses a `SKILL.md` document (YAML frontmatter + markdown body) and emits
//! a JSON object compatible with the agentskills.io registry schema.
//!
//! # Format
//!
//! Input (SKILL.md):
//! ```markdown
//! ---
//! name: my-skill
//! description: Does something useful
//! when_to_use: When user needs X
//! ---
//! Body content here...
//! ```
//!
//! Output (`AgentSkillJson`):
//! ```json
//! {
//!   "name": "my-skill",
//!   "description": "Does something useful",
//!   "version": "1.0.0",
//!   "triggers": ["When user needs X"],
//!   "body": "Body content here..."
//! }
//! ```

use serde::{Deserialize, Serialize};

/// agentskills.io JSON export format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSkillJson {
    /// Skill name — from SKILL.md frontmatter `name:`.
    pub name: String,
    /// Human-readable description — from frontmatter `description:`.
    pub description: String,
    /// Semantic version string (defaults to `"1.0.0"` when absent from frontmatter).
    pub version: String,
    /// Trigger phrases that activate this skill.
    ///
    /// Sourced from `when_to_use`, `triggers`, or `description` (fallback).
    pub triggers: Vec<String>,
    /// Raw markdown body following the closing `---`.
    pub body: String,
}

/// Parsed SKILL.md frontmatter fields (only what we need for export).
#[derive(Debug, Default, Deserialize)]
struct SkillFrontmatter {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: Option<String>,
    /// Accepted as `when_to_use` (LA convention).
    #[serde(default)]
    when_to_use: Option<String>,
    /// Accepted as `triggers` (agentskills.io convention).
    #[serde(default)]
    triggers: Option<serde_json::Value>,
}

/// Export a `SKILL.md` document to the agentskills.io JSON format.
///
/// # Errors
///
/// Returns an error string when the document has no YAML frontmatter delimiters
/// or when the frontmatter cannot be parsed as YAML.
pub fn export_skill_to_agentskills_format(skill_md: &str) -> Result<AgentSkillJson, String> {
    let (frontmatter_str, body) = split_frontmatter(skill_md)?;

    let fm: SkillFrontmatter = serde_yaml::from_str(frontmatter_str)
        .map_err(|e| format!("SKILL.md frontmatter parse error: {e}"))?;

    let name = if fm.name.is_empty() {
        return Err("SKILL.md frontmatter missing required field 'name'".to_owned());
    } else {
        fm.name
    };

    let description = fm.description;
    let version = fm.version.unwrap_or_else(|| "1.0.0".to_owned());
    let triggers = extract_triggers(
        fm.when_to_use.as_deref(),
        fm.triggers.as_ref(),
        &description,
    );

    Ok(AgentSkillJson {
        name,
        description,
        version,
        triggers,
        body: body.trim().to_owned(),
    })
}

/// Split a SKILL.md document into `(frontmatter_str, body)`.
///
/// Expects the document to start with `---`, followed by YAML content,
/// followed by a closing `---` on its own line.
fn split_frontmatter(skill_md: &str) -> Result<(&str, &str), String> {
    let content = skill_md.trim_start();
    let content = content
        .strip_prefix("---")
        .ok_or("SKILL.md must start with '---' frontmatter delimiter")?;

    // Find the closing ---
    let close = content
        .find("\n---")
        .ok_or("SKILL.md frontmatter not closed — missing closing '---'")?;

    let frontmatter = content[..close].trim();
    let body = &content[close + 4..]; // skip "\n---"

    Ok((frontmatter, body))
}

/// Build the `triggers` list from available frontmatter fields.
///
/// Priority: explicit `triggers` array > `when_to_use` string > `description` fallback.
fn extract_triggers(
    when_to_use: Option<&str>,
    triggers: Option<&serde_json::Value>,
    description: &str,
) -> Vec<String> {
    if let Some(t) = triggers {
        // triggers may be a YAML sequence or a single string
        match t {
            serde_json::Value::Array(arr) => {
                let v: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                if !v.is_empty() {
                    return v;
                }
            }
            serde_json::Value::String(s) if !s.is_empty() => {
                return vec![s.clone()];
            }
            _ => {}
        }
    }

    if let Some(wtu) = when_to_use {
        if !wtu.is_empty() {
            return vec![wtu.to_owned()];
        }
    }

    // Last resort: use description as a single trigger
    if description.is_empty() {
        Vec::new()
    } else {
        vec![description.to_owned()]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const SAMPLE_SKILL: &str = r#"---
name: test-skill
description: A skill for testing agentskills export
version: "2.1.0"
when_to_use: When user needs test-skill functionality
---
# Test Skill

This skill does things.
"#;

    #[test]
    fn test_export_roundtrip_preserves_name_and_description() {
        let result = export_skill_to_agentskills_format(SAMPLE_SKILL).unwrap();
        assert_eq!(result.name, "test-skill");
        assert_eq!(result.description, "A skill for testing agentskills export");
        assert_eq!(result.version, "2.1.0");
        assert_eq!(
            result.triggers,
            vec!["When user needs test-skill functionality"]
        );
        assert!(result.body.contains("This skill does things."));
    }

    #[test]
    fn test_export_default_version_when_absent() {
        let skill = "---\nname: no-version\ndescription: desc\n---\nbody";
        let result = export_skill_to_agentskills_format(skill).unwrap();
        assert_eq!(result.version, "1.0.0");
    }

    #[test]
    fn test_export_falls_back_to_description_for_triggers() {
        let skill = "---\nname: fallback-skill\ndescription: Use for XYZ\n---\nbody";
        let result = export_skill_to_agentskills_format(skill).unwrap();
        assert_eq!(result.triggers, vec!["Use for XYZ"]);
    }

    #[test]
    fn test_export_errors_on_missing_frontmatter() {
        let result = export_skill_to_agentskills_format("no frontmatter here");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with"));
    }

    #[test]
    fn test_export_errors_on_missing_name() {
        let skill = "---\ndescription: desc\n---\nbody";
        let result = export_skill_to_agentskills_format(skill);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("missing required field 'name'")
        );
    }
}
