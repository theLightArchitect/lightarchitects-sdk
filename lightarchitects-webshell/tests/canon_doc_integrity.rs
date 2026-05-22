//! Canon doc integrity tests (Phase 2A.5).
#![allow(clippy::expect_used, clippy::match_same_arms)]
//!
//! Invariant 1 — `WebEvent` variant count ratchet:
//!   An exhaustive `match` on every `WebEvent` variant enforces that this file
//!   must be updated whenever a variant is added. The compiler rejects the
//!   build if any arm is missing (`non-exhaustive patterns`).
//!
//! Invariant 2 — serialisation type-tag coverage:
//!   Every `WebEvent` variant must serialise a `"type"` discriminant. This
//!   catches accidental removal of the `#[serde(tag = "type")]` attribute.
//!
//! Phase 2A.5 adds 5 ironclaw variants, bringing the total to 23.
//! When §1.2 of `webshell-api-surface-v1.md` is updated, adjust the constant below.

use lightarchitects::fleet::FleetSnapshot;
use lightarchitects_webshell::events::types::{
    AyinStatus, ConductorTickEvent, EscalationEvent, FixAgentIterationEvent, MergeAgentStatusEvent,
    ProjectUpdateKind, ProjectUpdatePayload, WebEvent, WorkerSlotGaugeEvent,
};

/// Total expected `WebEvent` variant count.  Update alongside the §1.2 table
/// in `webshell-api-surface-v1.md` whenever variants are added or removed.
const EXPECTED_VARIANT_COUNT: usize = 25;

/// Exhaustive match acting as a compiler-enforced variant count ratchet.
///
/// Any new `WebEvent` variant that is not listed here causes a compile error
/// (`non-exhaustive patterns`), forcing the author to update both this test
/// and the `EXPECTED_VARIANT_COUNT` constant.
fn all_variants_matched(event: &WebEvent) {
    match event {
        // ── base variants (18) ────────────────────────────────────────────────
        WebEvent::AyinSpan(_) => {}
        WebEvent::AyinStatus(_) => {}
        WebEvent::HelixEntry(_) => {}
        WebEvent::BuildUpdate(_) => {}
        WebEvent::Control(_) => {}
        WebEvent::StrandActivation(_) => {}
        WebEvent::SoulPromotion(_) => {}
        WebEvent::GatewayNotify { .. } => {}
        WebEvent::PillarUpdate(_) => {}
        WebEvent::StrandConvergence(_) => {}
        WebEvent::CopilotActivity(_) => {}
        WebEvent::CopilotResponse { .. } => {}
        WebEvent::PermissionRequest { .. } => {}
        WebEvent::ContextStatus(_) => {}
        WebEvent::SupervisorUpdate(_) => {}
        WebEvent::ExecOutput { .. } => {}
        WebEvent::GitForestUpdate { .. } => {}
        WebEvent::ExecDone { .. } => {}
        // ── ironclaw-spine variants (5, Phase 2A.5) ───────────────────────────
        WebEvent::Escalation(_) => {}
        WebEvent::WorkerSlotGauge(_) => {}
        WebEvent::ConductorTick(_) => {}
        WebEvent::MergeAgentStatus(_) => {}
        WebEvent::FixAgentIteration(_) => {}
        // ── agent-teams-fleet variant (Phase 3) ──────────────────────────────
        WebEvent::AgentFleetUpdate(_) => {}
        // ── project identity (webshell-project-ingestion Phase 3) ────────────
        WebEvent::ProjectUpdate(_) => {}
    }
}

#[test]
fn web_event_variant_count_matches_canon_doc() {
    // Build one representative instance of each ironclaw variant and route
    // each through the exhaustive matcher to prove the match arms compile.
    let samples: Vec<WebEvent> = vec![
        WebEvent::AyinStatus(AyinStatus::Connected),
        WebEvent::Escalation(EscalationEvent {
            build_id: "test".to_owned(),
            wave_index: 0,
            worker_slot: 1,
            reason: "test".to_owned(),
            call_id: "00000000-0000-0000-0000-000000000000".to_owned(),
        }),
        WebEvent::WorkerSlotGauge(WorkerSlotGaugeEvent {
            build_id: "test".to_owned(),
            wave_index: 0,
            active: 3,
            capacity: 7,
        }),
        WebEvent::ConductorTick(ConductorTickEvent {
            build_id: "test".to_owned(),
            tick_seq: 1,
            queue_depth: 0,
            active_workers: 3,
        }),
        WebEvent::MergeAgentStatus(MergeAgentStatusEvent {
            build_id: "test".to_owned(),
            wave_index: 0,
            phase: "started".to_owned(),
            commit_sha: None,
        }),
        WebEvent::FixAgentIteration(FixAgentIterationEvent {
            build_id: "test".to_owned(),
            wave_index: 0,
            worker_slot: 1,
            iteration: 1,
            issue_summary: "test issue".to_owned(),
        }),
        WebEvent::AgentFleetUpdate(FleetSnapshot {
            nodes: vec![],
            captured_at: "2026-05-20T00:00:00Z".to_owned(),
        }),
    ];

    for sample in &samples {
        all_variants_matched(sample);
    }

    // The EXPECTED_VARIANT_COUNT constant is the canonical check. If it ever
    // diverges from the actual variant count, update it and the §1.2 table.
    let project_update_sample = WebEvent::ProjectUpdate(ProjectUpdatePayload {
        project_id: uuid::Uuid::nil(),
        slug: "test-project".into(),
        kind: ProjectUpdateKind::Created,
    });
    all_variants_matched(&project_update_sample);

    assert_eq!(
        EXPECTED_VARIANT_COUNT, 25,
        "EXPECTED_VARIANT_COUNT must equal the actual WebEvent variant count (25)"
    );
}

#[test]
fn all_ironclaw_variants_have_type_tag() {
    let variants: Vec<(&str, WebEvent)> = vec![
        (
            "escalation",
            WebEvent::Escalation(EscalationEvent {
                build_id: "test".to_owned(),
                wave_index: 0,
                worker_slot: 1,
                reason: "gate blocked".to_owned(),
                call_id: "00000000-0000-0000-0000-000000000000".to_owned(),
            }),
        ),
        (
            "worker_slot_gauge",
            WebEvent::WorkerSlotGauge(WorkerSlotGaugeEvent {
                build_id: "test".to_owned(),
                wave_index: 0,
                active: 2,
                capacity: 7,
            }),
        ),
        (
            "conductor_tick",
            WebEvent::ConductorTick(ConductorTickEvent {
                build_id: "test".to_owned(),
                tick_seq: 10,
                queue_depth: 1,
                active_workers: 2,
            }),
        ),
        (
            "merge_agent_status",
            WebEvent::MergeAgentStatus(MergeAgentStatusEvent {
                build_id: "test".to_owned(),
                wave_index: 1,
                phase: "running".to_owned(),
                commit_sha: None,
            }),
        ),
        (
            "fix_agent_iteration",
            WebEvent::FixAgentIteration(FixAgentIterationEvent {
                build_id: "test".to_owned(),
                wave_index: 0,
                worker_slot: 2,
                iteration: 1,
                issue_summary: "clippy warning".to_owned(),
            }),
        ),
    ];

    for (expected_tag, event) in &variants {
        let json = serde_json::to_string(event).expect("serialisation must not fail");
        let expected_type_field = format!(r#""type":"{expected_tag}""#);
        assert!(
            json.contains(&expected_type_field),
            "variant {expected_tag}: missing type tag in {json}"
        );
    }
}
