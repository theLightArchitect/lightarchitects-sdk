//! Tool execution surface for agentic loops (TS-2 §6.1.2).
//!
//! [`ToolExecutor`] is the contract every tool dispatch path must satisfy.
//! [`NullToolExecutor`] is the fail-closed default — systems that do not need
//! tools MUST use it rather than `Option<Arc<dyn ToolExecutor>>`.

use async_trait::async_trait;
use serde_json::Value;

/// Description of a single tool exposed to the LLM.
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    /// Tool name as it appears in the LLM's `tools` array.
    pub name: String,
    /// Human-readable description forwarded to the model.
    pub description: String,
    /// JSON Schema for the tool's `input` field.
    pub input_schema: Value,
}

/// Result of a successfully executed tool call.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// Tool call identifier from the originating `tool_use` block.
    pub tool_use_id: String,
    /// Serialisable result content.
    pub content: Value,
    /// Whether the tool call produced an application-level error.
    ///
    /// A `true` value here is still a *successful execution* from the executor's
    /// perspective — it means the tool ran but reported failure to the LLM.
    pub is_error: bool,
}

/// Errors that can occur during tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// No `ToolExecutor` was wired — the system was configured without tools.
    ///
    /// This is the fail-closed response from [`NullToolExecutor`].
    #[error("no tool executor is available; system was configured without tools")]
    ToolsNotAvailable,

    /// The tool name is not in the executor's registry.
    #[error("unknown tool: {0:?}")]
    UnknownTool(String),

    /// The `input` JSON failed schema validation.
    #[error("invalid input for tool {tool_name:?}: {reason}")]
    InvalidInput {
        /// Name of the tool that rejected the input.
        tool_name: String,
        /// Human-readable validation failure.
        reason: String,
    },

    /// A security policy blocked this tool call.
    #[error("permission denied for tool {tool_name:?}: {reason}")]
    PermissionDenied {
        /// Name of the tool that was blocked.
        tool_name: String,
        /// Policy rule that triggered the denial.
        reason: String,
    },

    /// A skill invocation was superseded because the operator issued the same
    /// command directly (L-EVA-11 conflict resolution).
    #[error("tool call superseded by operator action")]
    SupersededByOperatorAction,

    /// A skill `tool_use` arrived for a skill whose trust level is `new` or
    /// `unknown` — the operator must grant consent first.
    #[error("skill {skill_name:?} is not trusted; operator consent required")]
    SkillNotTrusted {
        /// Name of the skill that requires consent.
        skill_name: String,
    },

    /// An unexpected internal error occurred during execution.
    #[error("internal tool execution error: {0}")]
    Internal(String),
}

/// Contract for dispatching LLM-emitted `tool_use` blocks (TS-2 §6.1.2).
///
/// Implementors MUST:
/// - Return [`ToolError::ToolsNotAvailable`] as the fail-closed default when
///   no tools are configured (use [`NullToolExecutor`]).
/// - Check a `PermissionMatrix` before dispatching any call.
/// - Wrap all tool results in `IndirectInjectionShield` sentinel delimiters
///   before returning them to the LLM context.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// List all tools this executor exposes to the LLM.
    fn tool_definitions(&self) -> Vec<ToolDefinition>;

    /// Execute the named tool with the given `input` JSON payload.
    ///
    /// # Errors
    ///
    /// Returns [`ToolError`] if the tool is unknown, the input is invalid,
    /// a policy gate blocks execution, or an internal error occurs.
    async fn execute(
        &self,
        tool_use_id: &str,
        tool_name: &str,
        input: Value,
    ) -> Result<ToolOutput, ToolError>;
}

/// Fail-closed [`ToolExecutor`] for systems that do not need tools (TS-2 §6.1.2).
///
/// Every call returns [`ToolError::ToolsNotAvailable`]. Use this instead of
/// `Option<Arc<dyn ToolExecutor>>` so callers always receive a typed error
/// rather than a panic or silent no-op.
#[derive(Debug, Default, Clone)]
pub struct NullToolExecutor;

#[async_trait]
impl ToolExecutor for NullToolExecutor {
    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![]
    }

    async fn execute(
        &self,
        _tool_use_id: &str,
        _tool_name: &str,
        _input: Value,
    ) -> Result<ToolOutput, ToolError> {
        Err(ToolError::ToolsNotAvailable)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_executor_returns_tools_not_available() {
        let exec = NullToolExecutor;
        let err = exec
            .execute("id_1", "read_file", serde_json::json!({"path": "/tmp/x"}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::ToolsNotAvailable));
    }

    #[test]
    fn null_executor_has_no_tool_definitions() {
        let exec = NullToolExecutor;
        assert!(exec.tool_definitions().is_empty());
    }

    #[test]
    fn tool_error_display_unknown_tool() {
        let e = ToolError::UnknownTool("bash".to_owned());
        assert!(e.to_string().contains("bash"));
    }

    #[test]
    fn tool_error_display_skill_not_trusted() {
        let e = ToolError::SkillNotTrusted {
            skill_name: "plan".to_owned(),
        };
        assert!(e.to_string().contains("plan"));
    }

    #[test]
    fn tool_error_display_permission_denied() {
        let e = ToolError::PermissionDenied {
            tool_name: "bash".to_owned(),
            reason: "denylist".to_owned(),
        };
        assert!(e.to_string().contains("denylist"));
    }
}
