//! Build lifecycle → chat-inject formatter for the LASDLC phase-handoff feature.
//!
//! Build `bridging-whistling-loom`'s killer feature is automatic chat posts
//! when a build's phase transitions, so the next phase's claimable agent
//! sees the handoff in the Squad Comms thread without manual relay.
//!
//! The actual lifecycle event source is in
//! `lightarchitects-gateway` (which already POSTs to
//! `/api/builds/:id/notify`). This module provides the **formatting half**:
//! given a phase transition payload, produce the chat-message body the
//! webshell should inject. Wiring the gateway emitter to call this formatter
//! is a private-crate change tracked separately.
//!
//! TODO(crate-boundary): the gateway emitter is in
//! `lightarchitects-gateway/src/handlers/`. Belongs in:
//!   [x] private (lightarchitects-gateway) — emit `phase_transition` payloads
//!   [ ] public  (SDK `lightarchitects::xxx` as API client)
//! The formatter here is webshell-local — it is a UI presentation concern,
//! not business logic, so its placement is intentional.

/// A LASDLC phase transition observed by the gateway.
///
/// The wire format is intentionally permissive — the gateway-side emitter
/// fills only the fields it knows.
#[derive(Debug, Clone)]
pub struct PhaseTransition<'a> {
    /// Build codename (e.g. `bridging-whistling-loom`).
    pub build_codename: &'a str,
    /// Phase that just completed (e.g. `Phase 3 — IMPLEMENT`).
    pub from_phase: &'a str,
    /// Phase becoming claimable (e.g. `Phase 4 — VERIFY`).
    pub to_phase: &'a str,
    /// Optional next-claimable agent label.
    pub next_agent: Option<&'a str>,
}

/// Format a phase-handoff message body for injection into the chat session.
///
/// The output is plain text (no markdown), capped at 480 chars so it fits
/// in a single chat bubble without scrolling.
#[must_use]
pub fn format_phase_handoff(transition: &PhaseTransition<'_>) -> String {
    let agent = transition
        .next_agent
        .map_or_else(|| "next agent".to_owned(), |a| format!("@{a}"));
    let body = format!(
        "[{build}] {from} complete → {to} claimable. {agent} pick this up.",
        build = transition.build_codename,
        from = transition.from_phase,
        to = transition.to_phase,
        agent = agent,
    );
    truncate_chars(&body, 480)
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    s.chars().take(max).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn format_phase_handoff_includes_build_and_phases() {
        let t = PhaseTransition {
            build_codename: "bridging-whistling-loom",
            from_phase: "Phase 3 — IMPLEMENT",
            to_phase: "Phase 4 — VERIFY",
            next_agent: Some("corso"),
        };
        let msg = format_phase_handoff(&t);
        assert!(msg.contains("bridging-whistling-loom"));
        assert!(msg.contains("Phase 3"));
        assert!(msg.contains("Phase 4"));
        assert!(msg.contains("@corso"));
    }

    #[test]
    fn format_phase_handoff_omits_agent_when_none() {
        let t = PhaseTransition {
            build_codename: "bwl",
            from_phase: "P3",
            to_phase: "P4",
            next_agent: None,
        };
        let msg = format_phase_handoff(&t);
        assert!(msg.contains("next agent"));
        assert!(!msg.contains('@'));
    }

    #[test]
    fn format_phase_handoff_truncates_long_inputs() {
        let long = "x".repeat(600);
        let t = PhaseTransition {
            build_codename: &long,
            from_phase: "f",
            to_phase: "t",
            next_agent: None,
        };
        let msg = format_phase_handoff(&t);
        assert!(msg.chars().count() <= 480);
    }
}
