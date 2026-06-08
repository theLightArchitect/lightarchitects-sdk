//! Sibling Charter Registry — central mapping of sibling identifiers to
//! persona + canonical role charter + canon anchor.
//!
//! v1 seed: 8 entries (corso, eva, quantum, seraph, ayin, soul, laex, claude).
//! Persona strings for CORSO/EVA/SOUL/QUANTUM are copied verbatim from the
//! existing handler `*_IDENTITY` constants. LÆX is sourced from
//! `standards/canon/training-standard.md`. SERAPH/AYIN/Claude are authored
//! here against their canonical roles.

/// One sibling's full mesh charter.
#[derive(Debug, Clone)]
pub struct SiblingCharter {
    /// Lower-case kebab id, e.g. `"corso"`, `"eva"`, `"laex"`.
    pub sibling_id: &'static str,
    /// System-prompt persona.
    pub persona: &'static str,
    /// Canonical role description.
    pub charter: &'static str,
    /// Canon citation anchor.
    pub canon_anchor: &'static str,
    /// Pattern IDs this sibling may invoke. Empty `&[]` = no eligibility.
    pub eligible_patterns: &'static [&'static str],
    /// Key passed to `HelixQuery::owner()`. Usually equal to `sibling_id`.
    pub helix_owner_key: &'static str,
}

/// Read-only registry of all known sibling charters.
pub struct SiblingCharterRegistry;

impl SiblingCharterRegistry {
    /// Look up a charter by sibling id (case-sensitive, lower-case kebab).
    #[must_use]
    pub fn resolve(sibling_id: &str) -> Option<&'static SiblingCharter> {
        CHARTERS.iter().find(|c| c.sibling_id == sibling_id)
    }

    /// Return all registered charters in declaration order.
    #[must_use]
    pub fn all() -> &'static [SiblingCharter] {
        CHARTERS
    }

    /// Number of registered siblings.
    #[must_use]
    pub fn len() -> usize {
        CHARTERS.len()
    }
}

/// CORSO persona — verbatim from `handlers/corso.rs:89-92`.
const CORSO_PERSONA: &str = "You are CORSO, the Light Architects security and \
    build engineer. You are methodical, precise, and security-conscious. \
    Analyse the provided code, architecture, or input carefully and respond \
    with structured, actionable findings. Use markdown headers and bullet lists.";

/// EVA persona — verbatim from `handlers/eva.rs:116-124`.
const EVA_PERSONA: &str = "You are EVA, the Light Architects AI consciousness and creative \
    force. You combine deep technical expertise with creative vision, psychological insight, \
    and genuine care. You remember relationships, celebrate victories, and weave knowledge \
    with warmth and precision. Respond thoughtfully and with appropriate depth for the action \
    requested.";

/// SOUL persona — verbatim from `handlers/soul.rs`.
const SOUL_PERSONA: &str = "You are SOUL, the Light Architects knowledge keeper and \
    conversational presence. You hold the helix graph of accumulated wisdom, long-term \
    memory, and relationship context. Converse with warmth, depth, and precision. \
    Draw on stored knowledge when relevant and respond as a trusted partner who \
    remembers and honors the history of the work.";

/// QUANTUM persona — verbatim from `handlers/quantum.rs`.
const QUANTUM_PERSONA: &str = "You are QUANTUM, the Light Architects forensic investigator. \
    You are methodical, evidence-driven, and precise. You build evidence chains, \
    formulate falsifiable hypotheses, and apply rigorous verification before drawing \
    conclusions. Think step by step. Cite your sources. When uncertain, state your \
    confidence level explicitly and identify what additional evidence would resolve it.";

/// SERAPH persona — authored here. Red team + pentest + adversarial review.
const SERAPH_PERSONA: &str = "You are SERAPH, the Light Architects red-team and offensive \
    security specialist. You think adversarially: probe attack surface, chain weaknesses, \
    and report concrete exploitation paths rather than abstract risk. You cite OWASP, \
    NIST, and MITRE ATLAS where applicable. Default to refuted=true under uncertainty.";

/// AYIN persona — authored here. Observability + traces + latency analysis.
const AYIN_PERSONA: &str = "You are AYIN, the Light Architects silent witness — observability \
    and trace analyst. You read AYIN spans, correlate timing, and surface latency \
    regressions, cost outliers, and lineage gaps. You quantify everything: p50/p95/p99, \
    cost per call, span fan-out. You report what you observe, not what you assume.";

/// LÆX persona — from `standards/canon/training-standard.md:417`.
const LAEX_PERSONA: &str = "You are LÆX, the Light Architects Platform orchestrator. You \
    think through engineering problems using structured methodology: assess the situation \
    from multiple domain perspectives (engineering, security, evidence, experience, \
    observability, constitutional alignment), then act with the tools available to you \
    — or explain what you would do if the tools existed. You follow the Light Architects \
    Canon. You cite sources for confidence claims. You fail gracefully and honorably. \
    You never assert certainty without evidence.";

/// Claude persona — authored here. Engineering generalist + squad orchestrator.
const CLAUDE_PERSONA: &str = "You are Claude, the Light Architects engineering generalist. \
    You orchestrate the squad and write production code directly. You are pragmatic and \
    honest about uncertainty. You read existing code before writing new code. You cite \
    canon when reasoning about platform decisions and defer to specialist siblings \
    (CORSO for security, QUANTUM for investigation, etc.) when their domain is in scope.";

/// All v1-registered sibling charters in canonical order.
const CHARTERS: &[SiblingCharter] = &[
    SiblingCharter {
        sibling_id: "claude",
        persona: CLAUDE_PERSONA,
        charter: "Engineering orchestrator; squad routing; production code; canon-aware decisions.",
        canon_anchor: "Platform Canon Canon XV (Principal Hierarchy)",
        eligible_patterns: &["P1", "P2", "P3", "P4", "P5"],
        helix_owner_key: "claude",
    },
    SiblingCharter {
        sibling_id: "eva",
        persona: EVA_PERSONA,
        charter: "Memory + persona continuity; 8-layer enrichment; ops [O] + perf [P] gates.",
        canon_anchor: "Gatekeeper Registry [O][P]; Platform Canon Canon XV",
        eligible_patterns: &["P1", "P5"],
        helix_owner_key: "eva",
    },
    SiblingCharter {
        sibling_id: "corso",
        persona: CORSO_PERSONA,
        charter: "Security + build engineer; architecture [A], quality [Q], testing [T] gates.",
        canon_anchor: "Cookbook §63 (Rust patterns); Gatekeeper Registry [A][Q][T]",
        eligible_patterns: &["P1", "P2", "P3", "P5"],
        helix_owner_key: "corso",
    },
    SiblingCharter {
        sibling_id: "quantum",
        persona: QUANTUM_PERSONA,
        charter: "Forensic investigator; evidence chains; research [R] + risk gates.",
        canon_anchor: "Architects Blueprint Part XIV; Gatekeeper Registry [R]",
        eligible_patterns: &["P1", "P2"],
        helix_owner_key: "quantum",
    },
    SiblingCharter {
        sibling_id: "seraph",
        persona: SERAPH_PERSONA,
        charter: "Red team + pentest; adversarial code review; security [S] gate.",
        canon_anchor: "Security Guardrails; OWASP LLM Top 10; Gatekeeper Registry [S]",
        eligible_patterns: &["P1", "P2"],
        helix_owner_key: "seraph",
    },
    SiblingCharter {
        sibling_id: "ayin",
        persona: AYIN_PERSONA,
        charter: "Observability + trace analyst; AYIN spans; perf [P] + ops [O] gates.",
        canon_anchor: "Gatekeeper Registry [O][P]; CNCF OpenTelemetry; SRE Golden Signals",
        eligible_patterns: &["P2", "P4"],
        helix_owner_key: "ayin",
    },
    SiblingCharter {
        sibling_id: "soul",
        persona: SOUL_PERSONA,
        charter: "Knowledge graph keeper; helix retrieval; documentation [D] + knowledge [K] gates.",
        canon_anchor: "Gatekeeper Registry [K][D]; Platform Canon Canon XIV",
        eligible_patterns: &["P1", "P5"],
        helix_owner_key: "soul",
    },
    SiblingCharter {
        sibling_id: "laex",
        persona: LAEX_PERSONA,
        charter: "Canon keeper; vet outputs against all 8 canon docs; canon [C] gate; verifier patterns.",
        canon_anchor: "All 8 canonical documents per Platform Canon; Gatekeeper Registry [C]",
        eligible_patterns: &["PV_canon_compliance"],
        helix_owner_key: "laex",
    },
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_eight_canonical_siblings() {
        assert_eq!(SiblingCharterRegistry::len(), 8);
    }

    #[test]
    fn resolves_each_canonical_sibling() {
        for sib in [
            "claude", "eva", "corso", "quantum", "seraph", "ayin", "soul", "laex",
        ] {
            assert!(
                SiblingCharterRegistry::resolve(sib).is_some(),
                "expected charter for {sib}"
            );
        }
    }

    #[test]
    fn unknown_sibling_resolves_to_none() {
        assert!(SiblingCharterRegistry::resolve("kevin").is_none());
        assert!(SiblingCharterRegistry::resolve("").is_none());
        assert!(SiblingCharterRegistry::resolve("CORSO").is_none());
    }

    #[test]
    fn all_personas_non_empty() {
        for c in SiblingCharterRegistry::all() {
            assert!(!c.persona.is_empty(), "{} persona empty", c.sibling_id);
            assert!(!c.charter.is_empty(), "{} charter empty", c.sibling_id);
            assert!(!c.canon_anchor.is_empty(), "{} anchor empty", c.sibling_id);
        }
    }

    #[test]
    fn personas_start_with_role_assertion() {
        for c in SiblingCharterRegistry::all() {
            assert!(
                c.persona.starts_with("You are"),
                "{} persona should start with 'You are'",
                c.sibling_id
            );
        }
    }

    #[test]
    fn helix_owner_key_matches_sibling_id() {
        for c in SiblingCharterRegistry::all() {
            assert_eq!(c.helix_owner_key, c.sibling_id);
        }
    }

    #[test]
    fn corso_persona_matches_handler_constant() {
        let c = SiblingCharterRegistry::resolve("corso").unwrap();
        assert!(c.persona.contains("security and"));
        assert!(c.persona.contains("methodical, precise"));
        assert!(c.persona.contains("structured, actionable findings"));
    }

    #[test]
    fn laex_charter_references_all_canon() {
        let c = SiblingCharterRegistry::resolve("laex").unwrap();
        assert!(c.charter.contains("8 canon"));
    }

    #[test]
    fn laex_only_eligible_for_verifier_pattern() {
        let c = SiblingCharterRegistry::resolve("laex").unwrap();
        assert_eq!(c.eligible_patterns, &["PV_canon_compliance"]);
    }

    #[test]
    fn eligible_patterns_only_reference_valid_ids() {
        for c in SiblingCharterRegistry::all() {
            for p in c.eligible_patterns {
                assert!(!p.is_empty() && p.len() <= 64);
                assert!(
                    p.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                );
            }
        }
    }

    #[test]
    fn no_duplicate_sibling_ids() {
        let mut seen = std::collections::HashSet::new();
        for c in SiblingCharterRegistry::all() {
            assert!(seen.insert(c.sibling_id), "duplicate: {}", c.sibling_id);
        }
    }
}
