//! Canonical CORSO action enum — code quality, security, ops, verification.
//!
//! Every action that the CORSO MCP server (`corsoTools`) supports is represented
//! here. The enum is split into three tiers:
//!
//! - **PUBLIC** — gateway-routable, available to any SDK consumer.
//! - **WORKFLOW** — gateway-routable, but orchestration-only (not ad-hoc).
//! - **INTERNAL** — filesystem primitives that are never routed through the gateway.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Canonical CORSO actions — code quality, security, ops, verification.
///
/// The 19 gateway-routable actions cover AI-assisted code intelligence
/// (sniff/guard/fetch/chase), planning (scout), code review, formal
/// verification, deployment, and cross-sibling workflow triggers.
///
/// Three internal filesystem actions (`read_file`, `write_file`,
/// `list_directory`) exist in the MCP server but are never exposed through
/// the Light Architects gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorsoAction {
    // ── PUBLIC (17) ─────────────────────────────────────────────────────────
    /// Code generation with CORSO Protocol compliance.
    Sniff,
    /// Security scanning (vuln + secrets + SEC pillar).
    Guard,
    /// Knowledge retrieval + Context7 (supports depth: quick/deep).
    Fetch,
    /// Performance analysis + bottleneck identification.
    Chase,
    /// Strategic planning — implementation plan generation.
    Scout,
    /// Code review against 7 Protocol pillars.
    CodeReview,
    /// Quick code generation (lighter than sniff).
    GenerateCode,
    /// Semantic code search via tree-sitter.
    SearchCode,
    /// Symbol definition lookup via tree-sitter.
    FindSymbol,
    /// File structure extraction via tree-sitter.
    GetOutline,
    /// Symbol reference lookup via tree-sitter.
    GetReferences,
    /// Codebase architecture analysis.
    AnalyzeArchitecture,
    /// Formal verification via multi-model oracle.
    Prove,
    /// Algorithm optimization with proven dominance.
    Optimize,
    /// Deployment planning (playbook generation).
    Deploy,
    /// Rollback procedure generation.
    Rollback,
    /// Log query and analysis.
    ManageLogs,

    // ── WORKFLOW (2) ────────────────────────────────────────────────────────
    /// Active pentesting via SERAPH (scope-gated).
    Strike,
    /// Continuous monitoring (CVE/exposure/patch via SERAPH).
    Watch,

    // ── INTERNAL (3) — not gateway-routed ───────────────────────────────────
    /// Read a file from the filesystem.
    #[doc(hidden)]
    ReadFile,
    /// Write a file to the filesystem.
    #[doc(hidden)]
    WriteFile,
    /// List a directory on the filesystem.
    #[doc(hidden)]
    ListDirectory,
}

impl CorsoAction {
    /// All gateway-routable actions (PUBLIC + WORKFLOW).
    pub const ALL_ROUTABLE: &[Self] = &[
        Self::Sniff,
        Self::Guard,
        Self::Fetch,
        Self::Chase,
        Self::Scout,
        Self::CodeReview,
        Self::GenerateCode,
        Self::SearchCode,
        Self::FindSymbol,
        Self::GetOutline,
        Self::GetReferences,
        Self::AnalyzeArchitecture,
        Self::Prove,
        Self::Optimize,
        Self::Deploy,
        Self::Rollback,
        Self::ManageLogs,
        Self::Strike,
        Self::Watch,
    ];

    /// Returns `true` for PUBLIC and WORKFLOW actions that are routed through
    /// the Light Architects gateway. Returns `false` for INTERNAL filesystem
    /// primitives.
    #[must_use]
    pub const fn is_gateway_routable(&self) -> bool {
        !matches!(self, Self::ReadFile | Self::WriteFile | Self::ListDirectory)
    }

    /// Returns the canonical snake\_case string used in MCP tool calls.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Sniff => "sniff",
            Self::Guard => "guard",
            Self::Fetch => "fetch",
            Self::Chase => "chase",
            Self::Scout => "scout",
            Self::CodeReview => "code_review",
            Self::GenerateCode => "generate_code",
            Self::SearchCode => "search_code",
            Self::FindSymbol => "find_symbol",
            Self::GetOutline => "get_outline",
            Self::GetReferences => "get_references",
            Self::AnalyzeArchitecture => "analyze_architecture",
            Self::Prove => "prove",
            Self::Optimize => "optimize",
            Self::Deploy => "deploy",
            Self::Rollback => "rollback",
            Self::ManageLogs => "manage_logs",
            Self::Strike => "strike",
            Self::Watch => "watch",
            Self::ReadFile => "read_file",
            Self::WriteFile => "write_file",
            Self::ListDirectory => "list_directory",
        }
    }
}

impl fmt::Display for CorsoAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for CorsoAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sniff" => Ok(Self::Sniff),
            "guard" => Ok(Self::Guard),
            "fetch" => Ok(Self::Fetch),
            "chase" => Ok(Self::Chase),
            "scout" => Ok(Self::Scout),
            "code_review" => Ok(Self::CodeReview),
            "generate_code" => Ok(Self::GenerateCode),
            "search_code" => Ok(Self::SearchCode),
            "find_symbol" => Ok(Self::FindSymbol),
            "get_outline" => Ok(Self::GetOutline),
            "get_references" => Ok(Self::GetReferences),
            "analyze_architecture" => Ok(Self::AnalyzeArchitecture),
            "prove" => Ok(Self::Prove),
            "optimize" => Ok(Self::Optimize),
            "deploy" => Ok(Self::Deploy),
            "rollback" => Ok(Self::Rollback),
            "manage_logs" => Ok(Self::ManageLogs),
            "strike" => Ok(Self::Strike),
            "watch" => Ok(Self::Watch),
            "read_file" => Ok(Self::ReadFile),
            "write_file" => Ok(Self::WriteFile),
            "list_directory" => Ok(Self::ListDirectory),
            other => Err(format!("unknown CORSO action: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_routable_count() {
        assert_eq!(CorsoAction::ALL_ROUTABLE.len(), 19);
    }

    #[test]
    fn test_as_str_roundtrip() {
        for &action in CorsoAction::ALL_ROUTABLE {
            let s = action.as_str();
            let parsed: CorsoAction = s.parse().unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(parsed, action, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn test_internal_not_routable() {
        assert!(!CorsoAction::ReadFile.is_gateway_routable());
        assert!(!CorsoAction::WriteFile.is_gateway_routable());
        assert!(!CorsoAction::ListDirectory.is_gateway_routable());
    }

    #[test]
    fn test_from_str() {
        assert_eq!("sniff".parse::<CorsoAction>(), Ok(CorsoAction::Sniff));
        assert_eq!("guard".parse::<CorsoAction>(), Ok(CorsoAction::Guard));
        assert_eq!(
            "code_review".parse::<CorsoAction>(),
            Ok(CorsoAction::CodeReview)
        );
        assert_eq!(
            "read_file".parse::<CorsoAction>(),
            Ok(CorsoAction::ReadFile)
        );
        assert!("nonexistent".parse::<CorsoAction>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(
            CorsoAction::AnalyzeArchitecture.to_string(),
            "analyze_architecture"
        );
        assert_eq!(CorsoAction::ManageLogs.to_string(), "manage_logs");
    }

    #[test]
    fn test_serde_roundtrip() {
        let action = CorsoAction::CodeReview;
        let json = serde_json::to_string(&action).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, "\"code_review\"");
        let parsed: CorsoAction = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed, action);
    }
}
