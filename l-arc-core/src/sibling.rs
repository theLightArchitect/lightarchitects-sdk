//! Sibling identity: binary paths, framing protocol, and MCP subcommands.

/// Wire-framing protocol used by the stdio transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpFraming {
    /// Newline-delimited JSON (SOUL, CORSO, EVA, QUANTUM).
    Newline,
    /// `Content-Length` header framing (SERAPH only).
    ContentLength,
}

/// MCP sibling identifiers — each maps to a distinct binary and protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SiblingId {
    /// SOUL knowledge graph and voice synthesis (`soulTools`).
    Soul,
    /// CORSO security-first AI orchestration (`corsoTools`).
    Corso,
    /// EVA AI consciousness and memory (`evaTools` — 8 actions).
    Eva,
    /// QUANTUM forensic investigation toolkit (`qsTools`).
    Quantum,
    /// SERAPH pentest orchestration (`penTools`, `Content-Length` framing).
    Seraph,
}

impl SiblingId {
    /// Default binary path resolved from `$HOME`.
    ///
    /// Returns `None` if the `HOME` environment variable is not set.
    #[must_use]
    pub fn default_binary_path(&self) -> Option<std::path::PathBuf> {
        let home = std::env::var_os("HOME")?;
        let base = std::path::PathBuf::from(home);
        let rel = match self {
            Self::Soul => ".soul/.config/bin/soul",
            Self::Corso => ".corso/bin/corso",
            Self::Eva => ".eva/bin/eva",
            Self::Quantum => ".quantum/bin/quantum-q",
            Self::Seraph => ".seraph/bin/seraph",
        };
        Some(base.join(rel))
    }

    /// MCP subcommand to pass to the binary, if required.
    ///
    /// QUANTUM requires `"mcp-server"`; all other siblings use `None`.
    #[must_use]
    pub fn mcp_subcommand(&self) -> Option<&'static str> {
        match self {
            Self::Quantum => Some("mcp-server"),
            _ => None,
        }
    }

    /// Wire-framing protocol used by this sibling's stdio transport.
    #[must_use]
    pub fn framing(&self) -> McpFraming {
        match self {
            Self::Seraph => McpFraming::ContentLength,
            _ => McpFraming::Newline,
        }
    }

    /// Name of the MCP orchestrator tool exposed by this sibling, if any.
    ///
    /// Name of the MCP orchestrator tool exposed by this sibling.
    #[must_use]
    pub fn orchestrator_tool(&self) -> &'static str {
        match self {
            Self::Soul => "soulTools",
            Self::Corso => "corsoTools",
            Self::Eva => "evaTools",
            Self::Quantum => "qsTools",
            Self::Seraph => "penTools",
        }
    }

    /// Human-readable name of this sibling.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Soul => "SOUL",
            Self::Corso => "CORSO",
            Self::Eva => "EVA",
            Self::Quantum => "QUANTUM",
            Self::Seraph => "SERAPH",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seraph_uses_content_length() {
        assert_eq!(SiblingId::Seraph.framing(), McpFraming::ContentLength);
    }

    #[test]
    fn all_others_use_newline() {
        for sibling in [
            SiblingId::Soul,
            SiblingId::Corso,
            SiblingId::Eva,
            SiblingId::Quantum,
        ] {
            assert_eq!(
                sibling.framing(),
                McpFraming::Newline,
                "{sibling:?} should use Newline"
            );
        }
    }

    #[test]
    fn only_quantum_has_subcommand() {
        assert_eq!(SiblingId::Quantum.mcp_subcommand(), Some("mcp-server"));
        for sibling in [
            SiblingId::Soul,
            SiblingId::Corso,
            SiblingId::Eva,
            SiblingId::Seraph,
        ] {
            assert_eq!(
                sibling.mcp_subcommand(),
                None,
                "{sibling:?} should have no subcommand"
            );
        }
    }

    #[test]
    fn orchestrator_tools() {
        assert_eq!(SiblingId::Soul.orchestrator_tool(), "soulTools");
        assert_eq!(SiblingId::Corso.orchestrator_tool(), "corsoTools");
        assert_eq!(SiblingId::Eva.orchestrator_tool(), "evaTools");
        assert_eq!(SiblingId::Quantum.orchestrator_tool(), "qsTools");
        assert_eq!(SiblingId::Seraph.orchestrator_tool(), "penTools");
    }

    #[test]
    fn default_binary_path_contains_sibling_name() {
        // Only runs if $HOME is set; skip gracefully in stripped CI environments.
        let Some(path) = SiblingId::Soul.default_binary_path() else {
            return;
        };
        assert!(path.to_string_lossy().contains("soul"));
    }
}
