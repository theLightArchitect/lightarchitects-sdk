//! [`GatewayToolExecutor`] — bridges the SDK [`ToolExecutor`] trait to the
//! gateway's `core_tools/*` handlers.
//!
//! Exposes six core tools to the LLM: `bash`, `read`, `write`, `edit`,
//! `glob`, `search`. Each maps to the corresponding `core_tools` handler.
//! Error conversion maps [`GatewayError`] variants to typed [`ToolError`]s.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use lightarchitects::agent::{ToolDefinition, ToolError, ToolExecutor, ToolOutput};
use serde_json::{Value, json};

use lightarchitects::agent::indirect_injection_shield::IndirectInjectionShield;

use crate::cli::skills::SkillSpec;
use crate::config::GatewayConfig;
use crate::core_tools::{bash, edit, glob, read, search, write};
use crate::error::GatewayError;

/// Default JSON Schema used for skill tools that have no `tool_schema:` in frontmatter.
fn default_skill_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "args": {
                "type": "array",
                "items": {"type": "string"},
                "description": "Positional arguments to pass to the skill."
            }
        }
    })
}

/// Tool executor that dispatches LLM `tool_use` blocks to gateway core tools
/// and user-invocable skills.
///
/// Holds a shared [`GatewayConfig`] and exposes:
/// - Six core tools: `bash`, `read`, `write`, `edit`, `glob`, `search`.
/// - One tool per user-invocable skill loaded at construction time.
///
/// Implements the operator-wins invariant (W6.3): when the operator has
/// explicitly invoked a skill via slash command in the current turn, any
/// concurrent or subsequent LLM `tool_use` for the same skill returns
/// [`ToolError::SupersededByOperatorAction`] rather than double-executing.
pub struct GatewayToolExecutor {
    config: Arc<GatewayConfig>,
    /// Skills exposed as tools. Populated at construction time from the plugin cache.
    skills: Vec<SkillSpec>,
    /// Slugs the operator has explicitly invoked this turn (lowercase).
    ///
    /// Protected by a `Mutex` because the interactive input loop writes it
    /// while async tasks may read it concurrently.
    operator_active: Arc<Mutex<HashSet<String>>>,
}

impl GatewayToolExecutor {
    /// Create a new executor backed by the given config, with no skill tools.
    #[must_use]
    pub fn new(config: Arc<GatewayConfig>) -> Self {
        Self {
            config,
            skills: Vec::new(),
            operator_active: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Create an executor with user-invocable skills loaded from the plugin cache.
    ///
    /// Skills that have a `tool_schema:` field in their SKILL.md frontmatter
    /// expose that schema in their `ToolDefinition`; others use the default
    /// `{args: string[]}` schema. The skill tool name is the lowercase slug.
    #[must_use]
    pub fn new_with_skills(config: Arc<GatewayConfig>) -> Self {
        Self {
            config,
            skills: crate::cli::skills::list_all(),
            operator_active: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Mark a skill slug as operator-invoked for the current turn.
    ///
    /// Called by the interactive input loop immediately before executing an
    /// explicit operator slash command so that any concurrent LLM `tool_use`
    /// for the same skill returns [`ToolError::SupersededByOperatorAction`].
    pub fn mark_operator_invoked(&self, slug: &str) {
        if let Ok(mut set) = self.operator_active.lock() {
            set.insert(slug.to_lowercase());
        }
    }

    /// Clear the operator-claimed set at the start of a new input turn.
    pub fn clear_operator_invocations(&self) {
        if let Ok(mut set) = self.operator_active.lock() {
            set.clear();
        }
    }

    /// Return `true` if the operator has explicitly claimed this skill slug.
    fn is_operator_claimed(&self, slug: &str) -> bool {
        self.operator_active
            .lock()
            .map(|s| s.contains(slug))
            .unwrap_or(false)
    }
}

/// Convert a [`GatewayError`] into a [`ToolError`], preserving semantic detail.
fn gateway_err_to_tool_err(tool_name: &str, err: GatewayError) -> ToolError {
    match err {
        GatewayError::MissingParam(p) => ToolError::InvalidInput {
            tool_name: tool_name.to_owned(),
            reason: format!("missing required parameter: {p}"),
        },
        GatewayError::InvalidParam(reason) | GatewayError::InvalidRequest(reason) => {
            ToolError::InvalidInput {
                tool_name: tool_name.to_owned(),
                reason,
            }
        }
        GatewayError::UnknownTool(t) => ToolError::UnknownTool(t),
        GatewayError::Governance { reason, .. } => ToolError::PermissionDenied {
            tool_name: tool_name.to_owned(),
            reason,
        },
        other => ToolError::Internal(format!("{other}")),
    }
}

/// Apply the B2 indirect injection defence to a tool result envelope.
///
/// Wraps the `text` field inside `content[0]` in
/// `<tool_result_untrusted>` sentinel delimiters (OWASP-LLM01-1.3).
/// Detected High-severity injection patterns are logged to the tracing
/// subscriber; callers receive the wrapped, annotated envelope.
fn shield_tool_result(tool_use_id: &str, tool_name: &str, v: Value) -> Value {
    let shield = IndirectInjectionShield::new();

    // Extract the text payload from the standard MCP envelope.
    let text = v["content"][0]["text"].as_str().unwrap_or("").to_owned();

    // Scan for injection patterns before wrapping.
    let findings = shield.detect(&text);
    for f in &findings {
        tracing::warn!(
            tool_use_id,
            tool_name,
            pattern = %f.pattern,
            severity = ?f.severity,
            offset = f.offset,
            "indirect injection pattern detected in tool result"
        );
    }

    let wrapped = shield.wrap_tool_result(tool_use_id, &text);

    json!({
        "content": [{"type": "text", "text": wrapped}]
    })
}

#[async_trait]
impl ToolExecutor for GatewayToolExecutor {
    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = vec![
            ToolDefinition {
                name: "bash".to_owned(),
                description: "Execute a shell command and return stdout + stderr. Non-zero exit codes are not errors — the exit code is embedded in the response.".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "command":    {"type": "string",  "description": "Shell command to run."},
                        "timeout_ms": {"type": "integer", "description": "Timeout in milliseconds (default 120 000)."},
                        "cwd":        {"type": "string",  "description": "Working directory for the command."}
                    },
                    "required": ["command"]
                }),
            },
            ToolDefinition {
                name: "read".to_owned(),
                description: "Read the contents of a file, optionally selecting a line range.".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path":   {"type": "string",  "description": "File path (~/... expanded)."},
                        "offset": {"type": "integer", "description": "1-indexed first line to return."},
                        "limit":  {"type": "integer", "description": "Maximum number of lines to return."}
                    },
                    "required": ["path"]
                }),
            },
            ToolDefinition {
                name: "write".to_owned(),
                description: "Write content to a file, creating or replacing it.".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path":    {"type": "string", "description": "Destination file path."},
                        "content": {"type": "string", "description": "File contents to write."}
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolDefinition {
                name: "edit".to_owned(),
                description: "Replace an exact substring in a file with new text.".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path":        {"type": "string",  "description": "File to edit."},
                        "old_string":  {"type": "string",  "description": "Exact text to find (must be unique)."},
                        "new_string":  {"type": "string",  "description": "Replacement text."},
                        "replace_all": {"type": "boolean", "description": "Replace every occurrence (default false)."}
                    },
                    "required": ["path", "old_string", "new_string"]
                }),
            },
            ToolDefinition {
                name: "glob".to_owned(),
                description: "List files matching a glob pattern in a directory.".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string", "description": "Glob pattern (e.g. '**/*.rs')."},
                        "cwd":     {"type": "string", "description": "Root directory (defaults to project root)."}
                    },
                    "required": ["pattern"]
                }),
            },
            ToolDefinition {
                name: "search".to_owned(),
                description: "Search for a regex pattern in files (ripgrep / grep fallback).".to_owned(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string",  "description": "Regex pattern to search for."},
                        "path":    {"type": "string",  "description": "Directory or file to search in."},
                        "glob":    {"type": "string",  "description": "Limit to files matching this glob."},
                        "context": {"type": "integer", "description": "Lines of context around each match."}
                    },
                    "required": ["pattern"]
                }),
            },
        ];

        // Append one ToolDefinition per user-invocable skill (W6.2).
        for skill in &self.skills {
            let schema = skill
                .tool_schema
                .clone()
                .unwrap_or_else(default_skill_schema);
            defs.push(ToolDefinition {
                name: skill.slug.to_lowercase(),
                description: if skill.description.is_empty() {
                    format!("Invoke the {} skill.", skill.slug)
                } else {
                    skill.description.clone()
                },
                input_schema: schema,
            });
        }

        defs
    }

    async fn execute(
        &self,
        tool_use_id: &str,
        tool_name: &str,
        input: Value,
    ) -> Result<ToolOutput, ToolError> {
        let config = Arc::clone(&self.config);
        let tool_use_id = tool_use_id.to_owned();

        let result: Result<Value, GatewayError> = match tool_name {
            "bash" => bash::run(input).await,
            "read" => {
                let cfg = Arc::clone(&config);
                match tokio::task::spawn_blocking(move || read::run(input, &cfg)).await {
                    Ok(r) => r,
                    Err(e) => Err(GatewayError::Internal(format!("read task panicked: {e}"))),
                }
            }
            "write" => {
                let cfg = Arc::clone(&config);
                match tokio::task::spawn_blocking(move || write::run(input, &cfg)).await {
                    Ok(r) => r,
                    Err(e) => Err(GatewayError::Internal(format!("write task panicked: {e}"))),
                }
            }
            "edit" => {
                let cfg = Arc::clone(&config);
                match tokio::task::spawn_blocking(move || edit::run(input, &cfg)).await {
                    Ok(r) => r,
                    Err(e) => Err(GatewayError::Internal(format!("edit task panicked: {e}"))),
                }
            }
            "glob" => glob::run(input, &config).await,
            "search" => search::run(input, &config).await,
            // Skill routing (W6.2/W6.3): if tool_name matches a loaded skill slug
            // (lowercase), apply the operator-wins gate then dispatch.
            name if self.skills.iter().any(|s| s.slug.to_lowercase() == name) => {
                // W6.3: operator slash-command wins — if the operator already ran
                // this skill directly in the current turn, the LLM tool_use is
                // redundant and must not double-execute.
                if self.is_operator_claimed(name) {
                    tracing::info!(
                        tool = name,
                        "skill tool_use superseded by operator slash-command this turn"
                    );
                    return Err(ToolError::SupersededByOperatorAction);
                }
                // SAFETY: the `any()` guard above ensures a matching entry exists.
                let skill = self
                    .skills
                    .iter()
                    .find(|s| s.slug.to_lowercase() == name)
                    .expect("matching skill exists — checked above");
                return run_skill_tool(skill, input, &tool_use_id).await;
            }
            unknown => return Err(ToolError::UnknownTool(unknown.to_owned())),
        };

        match result {
            Ok(v) => Ok(ToolOutput {
                content: shield_tool_result(&tool_use_id, tool_name, v),
                tool_use_id,
                is_error: false,
            }),
            Err(e) => {
                // Application-level errors become is_error=true ToolOutputs rather than
                // ToolErrors so the LLM can observe the failure and react (e.g., retry
                // with a corrected command).
                let message = e.to_string();
                let is_perm = matches!(e, GatewayError::Governance { .. });
                if is_perm {
                    Err(gateway_err_to_tool_err(tool_name, e))
                } else {
                    Ok(ToolOutput {
                        tool_use_id,
                        content: json!({
                            "content": [{"type": "text", "text": message}]
                        }),
                        is_error: true,
                    })
                }
            }
        }
    }
}

/// Run a user-invocable skill as a subprocess and capture its output.
///
/// Before spawning, verifies the skill's SKILL.md content against the
/// trust ledger (W6.1). A hash mismatch returns [`ToolError::SkillNotTrusted`]
/// rather than executing potentially tampered instructions.
///
/// Spawns `<current_exe> skill <slug> [args...]`, captures stdout+stderr,
/// and returns a `ToolOutput` so the LLM can read the result.
async fn run_skill_tool(
    skill: &SkillSpec,
    input: Value,
    tool_use_id: &str,
) -> Result<ToolOutput, ToolError> {
    // W6.1 trust gate — reject skills whose SKILL.md changed since pinning.
    if let Err(reason) = crate::cli::skill_trust::verify_or_pin(&skill.slug, &skill.content) {
        tracing::warn!(
            skill = %skill.slug,
            %reason,
            "skill trust check failed — refusing tool_use execution"
        );
        return Err(ToolError::SkillNotTrusted {
            skill_name: skill.slug.clone(),
        });
    }

    let mut cmd_args = vec!["skill".to_owned(), skill.slug.to_lowercase()];
    if let Some(extra) = input["args"].as_array() {
        cmd_args.extend(
            extra
                .iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned)),
        );
    }

    let exe = std::env::current_exe()
        .map_err(|e| ToolError::Internal(format!("cannot locate gateway binary: {e}")))?;

    let child = tokio::process::Command::new(&exe).args(&cmd_args).output();

    let slug = &skill.slug;
    let output = tokio::time::timeout(Duration::from_secs(120), child)
        .await
        .map_err(|_| ToolError::Internal(format!("skill `{slug}` timed out after 120s")))?
        .map_err(|e| ToolError::Internal(format!("skill subprocess error: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stderr.is_empty() {
        stdout.into_owned()
    } else {
        format!("{stdout}{stderr}")
    };

    Ok(ToolOutput {
        tool_use_id: tool_use_id.to_owned(),
        content: json!({"content": [{"type": "text", "text": text}]}),
        is_error: !output.status.success(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_executor() -> GatewayToolExecutor {
        GatewayToolExecutor::new(Arc::new(GatewayConfig::default()))
    }

    #[test]
    fn tool_definitions_returns_six_tools() {
        let exec = make_executor();
        let defs = exec.tool_definitions();
        assert_eq!(defs.len(), 6);
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read"));
        assert!(names.contains(&"write"));
        assert!(names.contains(&"edit"));
        assert!(names.contains(&"glob"));
        assert!(names.contains(&"search"));
    }

    #[tokio::test]
    async fn execute_unknown_tool_returns_unknown_tool_error() {
        let exec = make_executor();
        let err = exec
            .execute("id_1", "nonexistent_tool", json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::UnknownTool(ref n) if n == "nonexistent_tool"));
    }

    #[tokio::test]
    async fn execute_bash_missing_command_returns_is_error_output() {
        // Missing `command` parameter → core tool returns MissingParam error.
        // GatewayToolExecutor converts non-permission errors to is_error outputs
        // so the LLM can observe and retry.
        let exec = make_executor();
        let out = exec.execute("id_2", "bash", json!({})).await.unwrap();
        assert!(out.is_error);
    }

    #[tokio::test]
    async fn execute_bash_echo_succeeds() {
        let exec = make_executor();
        let out = exec
            .execute("id_3", "bash", json!({"command": "echo hello"}))
            .await
            .unwrap();
        assert!(!out.is_error);
        let text = out.content["content"][0]["text"].as_str().unwrap_or("");
        assert!(text.contains("hello"));
    }

    // ── W6.3 — operator-wins tests ──────────────────────────────────────────

    #[test]
    fn mark_operator_invoked_records_lowercase_slug() {
        let exec = make_executor();
        exec.mark_operator_invoked("BUILD");
        assert!(exec.is_operator_claimed("build"));
        assert!(!exec.is_operator_claimed("plan"));
    }

    #[test]
    fn clear_operator_invocations_resets_set() {
        let exec = make_executor();
        exec.mark_operator_invoked("plan");
        exec.clear_operator_invocations();
        assert!(!exec.is_operator_claimed("plan"));
    }

    #[tokio::test]
    async fn execute_skill_returns_superseded_when_operator_claimed() {
        // Build an executor with a synthetic SkillSpec injected directly.
        let mut exec = make_executor();
        exec.skills.push(SkillSpec {
            name: "Plan".to_owned(),
            description: "Test skill".to_owned(),
            slug: "plan".to_owned(),
            content: "---\nname: plan\nuser-invocable: true\n---\nBody.".to_owned(),
            path: std::path::PathBuf::from("/tmp/fake-skill.md"),
            user_invocable: true,
            tool_schema: None,
        });

        // Simulate: operator typed `/plan` first.
        exec.mark_operator_invoked("plan");

        let err = exec.execute("id_sup", "plan", json!({})).await.unwrap_err();
        assert!(
            matches!(err, ToolError::SupersededByOperatorAction),
            "expected SupersededByOperatorAction, got {err:?}"
        );
    }
}
