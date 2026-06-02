//! Conversation mode classification.
//!
//! [`Mode`] is the top-level discriminant that drives how the webshell copilot
//! dispatches a turn:
//!
//! - [`Mode::Chatroom`] → multi-voice organic conversation via `MultiVoiceSynthesizer`
//! - [`Mode::Build`], [`Mode::Secure`], [`Mode::Scrum`], [`Mode::Enrich`] →
//!   strategy loop dispatch via `StrategyRegistry::lookup` + `LoopRunner::run`
//!
//! [`Mode::classify`] is a lightweight keyword scan over recent message text and
//! the current active roster — it does **not** call an LLM.

use super::roster::ActiveRoster;

// ---------------------------------------------------------------------------
// Domain keyword table
// ---------------------------------------------------------------------------

/// `(sibling, keyword)` pairs used by [`Mode::classify`].
///
/// When a keyword appears in recent message text *and* the associated sibling
/// is on the active roster, the corresponding strategy mode is selected.
/// Strategy modes take priority over [`Mode::Chatroom`] in priority order:
/// Secure > Build > Scrum > Enrich.
pub const DOMAIN_KEYWORDS: &[(&str, &str)] = &[
    // Secure — SERAPH domain
    ("seraph", "pentest"),
    ("seraph", "vulnerability"),
    ("seraph", "threat"),
    ("seraph", "attack"),
    ("seraph", "exploit"),
    ("seraph", "audit"),
    ("seraph", "cve"),
    ("seraph", "security"),
    ("seraph", "scan"),
    // Build — CORSO domain
    ("corso", "build"),
    ("corso", "implement"),
    ("corso", "refactor"),
    ("corso", "architecture"),
    ("corso", "deploy"),
    ("corso", "compile"),
    ("corso", "feature"),
    // Scrum — squad review
    ("claude", "review"),
    ("claude", "scrum"),
    ("claude", "assess"),
    ("claude", "critique"),
    ("claude", "evaluate"),
    // Enrich — EVA / SOUL domain
    ("eva", "enrich"),
    ("eva", "memory"),
    ("eva", "helix"),
    ("eva", "remember"),
    ("eva", "reflect"),
];

// ---------------------------------------------------------------------------
// Mode
// ---------------------------------------------------------------------------

/// Conversation dispatch mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Organic multi-sibling chatroom — interest-scored speaker selection.
    Chatroom,
    /// Agentic build pipeline (CORSO-primary, 13-phase `BuildStrategy`).
    Build,
    /// Security assessment (SERAPH-primary, 5-phase `SecureStrategy`).
    Secure,
    /// Squad review or meeting (dual-mode `ScrumStrategy`, 12 phases).
    Scrum,
    /// Memory enrichment (EVA-primary, 5-phase `EnrichStrategy`).
    Enrich,
}

impl Mode {
    /// Classify the current conversation mode from recent message text and
    /// roster membership.
    ///
    /// Strategy modes are detected when a domain keyword appears in
    /// `message_text` **and** the owning sibling is on the active `roster`.
    /// When no strategy signal is found, falls back to [`Mode::Chatroom`].
    ///
    /// Priority: Secure > Build > Scrum > Enrich > Chatroom.
    #[must_use]
    pub fn classify(message_text: &str, roster: &ActiveRoster) -> Self {
        let lower = message_text.to_lowercase();

        // Slash-command fast path: /SECURE or /SERAPH prefixes bypass keyword scan.
        if lower.starts_with("/secure") || lower.starts_with("/seraph") {
            return Self::Secure;
        }
        if lower.starts_with("/build") || lower.starts_with("/corso") {
            return Self::Build;
        }
        if lower.starts_with("/scrum") || lower.starts_with("/review") {
            return Self::Scrum;
        }
        if lower.starts_with("/enrich") || lower.starts_with("/eva") {
            return Self::Enrich;
        }

        let active = roster.current();

        let mut secure_hit = false;
        let mut build_hit = false;
        let mut scrum_hit = false;
        let mut enrich_hit = false;

        for (sibling, keyword) in DOMAIN_KEYWORDS {
            if !lower.contains(keyword) {
                continue;
            }
            // Accept keyword if the owning sibling is active, OR if no roster
            // constraint applies (empty roster = all modes eligible).
            let sibling_active =
                active.is_empty() || active.iter().any(|id| id.to_lowercase().contains(sibling));

            if !sibling_active {
                continue;
            }

            match *sibling {
                "seraph" => secure_hit = true,
                "corso" => build_hit = true,
                "claude" => scrum_hit = true,
                "eva" => enrich_hit = true,
                _ => {}
            }
        }

        // Priority: Secure > Build > Scrum > Enrich > Chatroom
        if secure_hit {
            return Self::Secure;
        }
        if build_hit {
            return Self::Build;
        }
        if scrum_hit {
            return Self::Scrum;
        }
        if enrich_hit {
            return Self::Enrich;
        }

        Self::Chatroom
    }

    /// Human-readable strategy identifier used for registry lookup.
    ///
    /// Returns `None` for [`Mode::Chatroom`] (no strategy involved).
    #[must_use]
    pub fn strategy_id(&self) -> Option<&'static str> {
        match self {
            Self::Chatroom => None,
            Self::Build => Some("build"),
            Self::Secure => Some("secure"),
            Self::Scrum => Some("scrum"),
            Self::Enrich => Some("enrich"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn empty_roster() -> ActiveRoster {
        ActiveRoster::new()
    }

    fn roster_with(siblings: &[&str]) -> ActiveRoster {
        let mut r = ActiveRoster::new();
        let scores: Vec<(String, f32)> = siblings.iter().map(|s| ((*s).to_string(), 0.9)).collect();
        r.update(&scores);
        r
    }

    #[test]
    fn classifies_secure_on_pentest_keyword() {
        let r = roster_with(&["seraph"]);
        assert_eq!(
            Mode::classify("run a pentest on the gateway API", &r),
            Mode::Secure
        );
    }

    #[test]
    fn classifies_build_on_implement_keyword() {
        let r = roster_with(&["corso"]);
        assert_eq!(
            Mode::classify("let's implement the new feature", &r),
            Mode::Build
        );
    }

    #[test]
    fn classifies_scrum_on_review_keyword() {
        let r = roster_with(&["claude"]);
        assert_eq!(
            Mode::classify("can you review this pull request", &r),
            Mode::Scrum
        );
    }

    #[test]
    fn classifies_enrich_on_memory_keyword() {
        let r = roster_with(&["eva"]);
        assert_eq!(
            Mode::classify("let's enrich the helix memory", &r),
            Mode::Enrich
        );
    }

    #[test]
    fn defaults_to_chatroom_when_no_signal() {
        let r = roster_with(&["eva", "corso"]);
        assert_eq!(
            Mode::classify("what do you all think about the weather", &r),
            Mode::Chatroom
        );
    }

    #[test]
    fn secure_takes_priority_over_build() {
        // Both "threat" (secure) and "build" (build) present.
        let r = empty_roster();
        assert_eq!(
            Mode::classify("we need to threat-model the build pipeline", &r),
            Mode::Secure,
            "Secure should win over Build"
        );
    }

    #[test]
    fn keyword_ignored_when_sibling_not_on_roster() {
        // "pentest" is a seraph keyword, but seraph is not on the roster.
        let r = roster_with(&["eva", "corso"]);
        let result = Mode::classify("we should pentest the API", &r);
        // With seraph not active, should fall through to chatroom or build.
        assert_ne!(
            result,
            Mode::Secure,
            "secure must not activate when seraph is not on roster"
        );
    }

    #[test]
    fn strategy_id_returns_none_for_chatroom() {
        assert_eq!(Mode::Chatroom.strategy_id(), None);
    }

    #[test]
    fn strategy_id_returns_correct_string() {
        assert_eq!(Mode::Build.strategy_id(), Some("build"));
        assert_eq!(Mode::Secure.strategy_id(), Some("secure"));
        assert_eq!(Mode::Scrum.strategy_id(), Some("scrum"));
        assert_eq!(Mode::Enrich.strategy_id(), Some("enrich"));
    }

    #[test]
    fn classifies_secure_on_security_keyword() {
        let r = empty_roster();
        assert_eq!(
            Mode::classify("run a quick security scan", &r),
            Mode::Secure
        );
    }

    #[test]
    fn classifies_secure_on_scan_keyword() {
        let r = empty_roster();
        assert_eq!(
            Mode::classify("scan the codebase for vulnerabilities", &r),
            Mode::Secure
        );
    }

    #[test]
    fn slash_secure_prefix_routes_to_secure() {
        let r = empty_roster();
        assert_eq!(
            Mode::classify("/SECURE run security scan", &r),
            Mode::Secure
        );
    }

    #[test]
    fn slash_build_prefix_routes_to_build() {
        let r = empty_roster();
        assert_eq!(Mode::classify("/build the feature", &r), Mode::Build);
    }

    #[test]
    fn slash_scrum_prefix_routes_to_scrum() {
        let r = empty_roster();
        assert_eq!(Mode::classify("/scrum this diff", &r), Mode::Scrum);
    }

    #[test]
    fn slash_enrich_prefix_routes_to_enrich() {
        let r = empty_roster();
        assert_eq!(Mode::classify("/enrich today's session", &r), Mode::Enrich);
    }
}
