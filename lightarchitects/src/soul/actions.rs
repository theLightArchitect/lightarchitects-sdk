//! Canonical SOUL action enum — vault operations, queries, voice, research.
//!
//! Every action that the SOUL MCP server (`soulTools`) supports is represented
//! here. The enum is split into four tiers:
//!
//! - **PUBLIC** — gateway-routable, available to any SDK consumer.
//! - **PRIVATE** — voice aliases encapsulated behind the public `voice` action.
//! - **INTERNAL** — maintenance-only actions never routed through the gateway.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Canonical SOUL actions — vault operations, queries, voice, research, and
/// graph primitives.
///
/// Gateway-routable actions include vault CRUD (`read_note`, `write_note`,
/// `list_notes`, `manifest`, `ingest`), retrieval (`search`, `helix`, `query`,
/// `query_frontmatter`, `stats`), voice (`voice`, `converse`, `chat`), research
/// aggregation (`soul_search`), plus graph primitives (`convergences`, `relate`,
/// `links`, `validate`, `health`).
///
/// Two private aliases (`speak`, `dialogue`) are encapsulated behind the public
/// `voice` action. One internal action (`tag_sync`) is maintenance-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoulAction {
    // ── PUBLIC — Vault (5) ──────────────────────────────────────────────────
    /// Read markdown file from vault.
    ReadNote,
    /// Create new note (rejects overwrites).
    WriteNote,
    /// List files in vault directory.
    ListNotes,
    /// Read vault manifest.json.
    Manifest,
    /// Universal ingestion pipeline.
    Ingest,

    // ── PUBLIC — Query (5) ──────────────────────────────────────────────────
    /// Full-text regex search across vault.
    Search,
    /// Multi-dimensional consciousness entry query.
    Helix,
    /// 4-signal hybrid RAG retrieval.
    Query,
    /// Query by frontmatter field values.
    QueryFrontmatter,
    /// Live vault statistics.
    Stats,

    // ── PUBLIC — Voice (3 canonical) ────────────────────────────────────────
    /// Unified voice pipeline (prompt + TTS + batch).
    Voice,
    /// Personality prompt assembly for any sibling.
    Converse,
    /// Multi-sibling conversation engine.
    Chat,

    // ── PUBLIC — Research (1) ───────────────────────────────────────────────
    /// Research aggregation with trust pipeline.
    #[serde(alias = "research")]
    SoulSearch,

    // ── PUBLIC — Graph primitives (5) ──────────────────────────────────────
    /// Find convergent entries across siblings.
    Convergences,
    /// Relate entries across helix dimensions.
    Relate,
    /// Show backlinks for a vault entry.
    Links,
    /// Vault integrity validation.
    Validate,
    /// Health check for all SOUL components.
    Health,

    // ── PUBLIC — Enrichment bridge (1) ─────────────────────────────────────
    /// Commit an EVA enrichment checkpoint into a canonical helix entry.
    CommitEnrichment,

    // ── PRIVATE — voice aliases (2) ─────────────────────────────────────────
    /// Internal alias to `voice` (single-speaker TTS).
    #[doc(hidden)]
    Speak,
    /// Internal alias to `voice` (multi-speaker TTS).
    #[doc(hidden)]
    Dialogue,

    // ── INTERNAL (1) — not gateway-routed ───────────────────────────────────
    /// Tag synchronization across vault.
    #[doc(hidden)]
    TagSync,
}

impl SoulAction {
    /// All gateway-routable actions (PUBLIC only — excludes PRIVATE aliases
    /// and INTERNAL maintenance actions).
    pub const ALL_ROUTABLE: &[Self] = &[
        Self::ReadNote,
        Self::WriteNote,
        Self::ListNotes,
        Self::Manifest,
        Self::Ingest,
        Self::Search,
        Self::Helix,
        Self::Query,
        Self::QueryFrontmatter,
        Self::Stats,
        Self::Voice,
        Self::Converse,
        Self::Chat,
        Self::SoulSearch,
        Self::Convergences,
        Self::Relate,
        Self::Links,
        Self::Validate,
        Self::Health,
        Self::CommitEnrichment,
    ];

    /// Returns `true` for PUBLIC actions that are routed through the Light
    /// Architects gateway. Returns `false` for PRIVATE aliases and INTERNAL
    /// maintenance actions.
    #[must_use]
    pub const fn is_gateway_routable(&self) -> bool {
        !matches!(self, Self::Speak | Self::Dialogue | Self::TagSync)
    }

    /// Returns the canonical snake\_case string used in MCP tool calls.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ReadNote => "read_note",
            Self::WriteNote => "write_note",
            Self::ListNotes => "list_notes",
            Self::Manifest => "manifest",
            Self::Ingest => "ingest",
            Self::Search => "search",
            Self::Helix => "helix",
            Self::Query => "query",
            Self::QueryFrontmatter => "query_frontmatter",
            Self::Stats => "stats",
            Self::Voice => "voice",
            Self::Converse => "converse",
            Self::Chat => "chat",
            Self::SoulSearch => "soul_search",
            Self::Speak => "speak",
            Self::Dialogue => "dialogue",
            Self::Validate => "validate",
            Self::TagSync => "tag_sync",
            Self::Health => "health",
            Self::CommitEnrichment => "commit_enrichment",
            Self::Relate => "relate",
            Self::Links => "links",
            Self::Convergences => "convergences",
        }
    }
}

impl fmt::Display for SoulAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SoulAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read_note" => Ok(Self::ReadNote),
            "write_note" => Ok(Self::WriteNote),
            "list_notes" => Ok(Self::ListNotes),
            "manifest" => Ok(Self::Manifest),
            "ingest" => Ok(Self::Ingest),
            "search" => Ok(Self::Search),
            "helix" => Ok(Self::Helix),
            "query" => Ok(Self::Query),
            "query_frontmatter" => Ok(Self::QueryFrontmatter),
            "stats" => Ok(Self::Stats),
            "voice" => Ok(Self::Voice),
            "converse" => Ok(Self::Converse),
            "chat" => Ok(Self::Chat),
            "soul_search" | "research" => Ok(Self::SoulSearch),
            "speak" => Ok(Self::Speak),
            "dialogue" => Ok(Self::Dialogue),
            "validate" => Ok(Self::Validate),
            "tag_sync" => Ok(Self::TagSync),
            "health" => Ok(Self::Health),
            "commit_enrichment" => Ok(Self::CommitEnrichment),
            "relate" => Ok(Self::Relate),
            "links" => Ok(Self::Links),
            "convergences" => Ok(Self::Convergences),
            other => Err(format!("unknown SOUL action: {other}")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_all_routable_count() {
        assert_eq!(SoulAction::ALL_ROUTABLE.len(), 20);
    }

    #[test]
    fn test_as_str_roundtrip() {
        for &action in SoulAction::ALL_ROUTABLE {
            let s = action.as_str();
            let parsed: SoulAction = s.parse().unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(parsed, action, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn test_internal_not_routable() {
        assert!(!SoulAction::Speak.is_gateway_routable());
        assert!(!SoulAction::Dialogue.is_gateway_routable());
        assert!(!SoulAction::TagSync.is_gateway_routable());
    }

    #[test]
    fn test_from_str() {
        assert_eq!("read_note".parse::<SoulAction>(), Ok(SoulAction::ReadNote));
        assert_eq!("helix".parse::<SoulAction>(), Ok(SoulAction::Helix));
        assert_eq!("voice".parse::<SoulAction>(), Ok(SoulAction::Voice));
        assert_eq!("speak".parse::<SoulAction>(), Ok(SoulAction::Speak));
        assert_eq!("validate".parse::<SoulAction>(), Ok(SoulAction::Validate));
        assert!("nonexistent".parse::<SoulAction>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(
            SoulAction::QueryFrontmatter.to_string(),
            "query_frontmatter"
        );
        assert_eq!(SoulAction::TagSync.to_string(), "tag_sync");
    }

    #[test]
    fn test_serde_roundtrip() {
        let action = SoulAction::ReadNote;
        let json = serde_json::to_string(&action).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, "\"read_note\"");
        let parsed: SoulAction = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed, action);
    }
}
