//! `PermissionMatrix` — per-tool risk-tier policy for HITL approval gates.
//!
//! Maps tool names to [`RiskTier`] values and determines which calls require
//! human-in-the-loop (HITL) approval before the agent proceeds.
//!
//! # Default policy (secure-by-default)
//!
//! | Tier       | Auto-approved? | Examples                             |
//! |------------|---------------|--------------------------------------|
//! | `Low`      | Yes           | Read, List, Glob, Grep, `WebSearch`  |
//! | `Medium`   | **No** (HITL) | Write (new file), Edit (preview)     |
//! | `High`     | **No** (HITL) | Delete, Overwrite, Bash              |
//! | `Critical` | **No** (HITL) | rm -rf patterns, DROP TABLE          |
//!
//! Only `Low`-tier calls are auto-approved. All writes, edits, and exec calls
//! require operator sign-off. This is the most conservative safe default;
//! operators can loosen it via configuration in a future config-layer extension.
//!
//! # Security note (OWASP LLM01)
//!
//! `input_preview` for [`PermissionMatrix`] queries is derived from the
//! serialised tool-call payload — never from model-authored prose — to prevent
//! indirect prompt injection from influencing the risk-tier assessment.

use crate::events::types::RiskTier;

/// Policy decision for a single tool invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Tool call is automatically approved; no operator interaction needed.
    AutoApprove,
    /// Operator must explicitly approve before execution proceeds.
    RequireHitl,
}

/// Per-tool risk classification matrix.
///
/// Stateless — all policy is encoded in the default `Default` instance.
/// Future: load overrides from `config.toml` or the operator settings API.
#[derive(Debug, Clone, Default)]
pub struct PermissionMatrix;

impl PermissionMatrix {
    /// Classify `tool_name` and return its [`RiskTier`].
    ///
    /// Unknown tools default to `High` (fail-secure).
    #[must_use]
    pub fn risk_tier(&self, tool_name: &str) -> RiskTier {
        match tool_name {
            // Read-only operations — Low.
            "Read" | "List" | "Glob" | "Grep" | "WebSearch" | "WebFetch" | "LSP"
            | "NotebookRead" | "TaskGet" | "TaskList" | "TaskOutput" | "BashOutput"
            | "KillShell" => RiskTier::Low,

            // Idempotent writes with preview — Medium.
            "Write" | "Edit" | "NotebookEdit" | "MultiEdit" | "TaskCreate" | "TaskUpdate" => {
                RiskTier::Medium
            }

            // Destructive / irreversible local ops and anything unrecognised → High (fail-secure).
            _ => RiskTier::High,
        }
    }

    /// Determine whether a tool call should be auto-approved or requires HITL.
    ///
    /// Only [`RiskTier::Low`] calls are auto-approved. All other tiers block
    /// until the operator explicitly approves.
    #[must_use]
    pub fn should_auto_approve(&self, tool_name: &str) -> PolicyDecision {
        match self.risk_tier(tool_name) {
            RiskTier::Low => PolicyDecision::AutoApprove,
            _ => PolicyDecision::RequireHitl,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn read_is_auto_approved() {
        let m = PermissionMatrix;
        assert_eq!(m.should_auto_approve("Read"), PolicyDecision::AutoApprove);
    }

    #[test]
    fn write_requires_hitl() {
        let m = PermissionMatrix;
        assert_eq!(m.should_auto_approve("Write"), PolicyDecision::RequireHitl);
    }

    #[test]
    fn bash_requires_hitl() {
        let m = PermissionMatrix;
        assert_eq!(m.should_auto_approve("Bash"), PolicyDecision::RequireHitl);
    }

    #[test]
    fn unknown_tool_fails_secure() {
        let m = PermissionMatrix;
        assert_eq!(m.risk_tier("UnknownTool"), RiskTier::High);
        assert_eq!(
            m.should_auto_approve("UnknownTool"),
            PolicyDecision::RequireHitl
        );
    }

    #[test]
    fn grep_is_low_risk() {
        let m = PermissionMatrix;
        assert_eq!(m.risk_tier("Grep"), RiskTier::Low);
    }

    #[test]
    fn edit_is_medium_risk() {
        let m = PermissionMatrix;
        assert_eq!(m.risk_tier("Edit"), RiskTier::Medium);
    }

    #[test]
    fn all_low_tier_tools_are_auto_approved() {
        let m = PermissionMatrix;
        let low_tools = ["Read", "List", "Glob", "Grep", "WebSearch", "WebFetch"];
        for tool in low_tools {
            assert_eq!(
                m.should_auto_approve(tool),
                PolicyDecision::AutoApprove,
                "{tool} should be auto-approved"
            );
        }
    }

    #[test]
    fn all_write_tools_require_hitl() {
        let m = PermissionMatrix;
        let write_tools = [
            "Write",
            "Edit",
            "NotebookEdit",
            "MultiEdit",
            "Bash",
            "Agent",
        ];
        for tool in write_tools {
            assert_eq!(
                m.should_auto_approve(tool),
                PolicyDecision::RequireHitl,
                "{tool} should require HITL"
            );
        }
    }
}
