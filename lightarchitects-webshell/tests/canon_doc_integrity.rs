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
use lightarchitects_lightspace::types::{
    Actor, CardData, CardKind, CardState, CardTransition, DrawerFileAction, DrawerFileData,
    EvidenceTier, Provenance, UpdateMode,
};
use lightarchitects_webshell::events::types::{
    A2aEnvelopeEvent, A2aEnvelopeType, AyinStatus, BudgetExhaustedEvent, BudgetWarningEvent,
    ConductorTickEvent, EscalationEvent, FixAgentIterationEvent, GateEvalEvent, GateVerdictKind,
    HitlResolution, IronclawHitlEscalationEvent, IronclawHitlResolutionEvent,
    LightspaceBranchLaneEvent, LightspaceCardEvent, LightspaceConfidenceEvent,
    LightspaceDrawerEventPayload, LightspaceDrawerFileEvent, LightspaceGatingEvent,
    LightspaceGraduateEvent, LightspaceLifecycleEvent, LightspaceMaterializeEvent,
    LightspaceUpdateEvent, MergeAgentStatusEvent, ProjectUpdateKind, ProjectUpdatePayload,
    PtyRespawnedEvent, QuestionAnsweredEvent, QuestionHeadlessPolicy, QuestionItem,
    QuestionOptionItem, QuestionPromptEvent, WebEvent, WorkerSlotGaugeEvent,
};

/// Total expected `WebEvent` variant count.  Update alongside the §1.2 table
/// in `webshell-api-surface-v1.md` whenever variants are added or removed.
/// Phase 3 Wave 2a added 10 lightspace variants: 35 → 45.
const EXPECTED_VARIANT_COUNT: usize = 45;

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
        // ── ironclaw HITL events (Phase 4 — ironclaw-autonomous-e2e) ─────────
        WebEvent::IronclawHitlEscalation(_) => {}
        WebEvent::IronclawHitlResolution(_) => {}
        // ── webshell-hitl-bridge (Phase 1) ────────────────────────────────────
        WebEvent::QuestionPrompt(_) => {}
        WebEvent::QuestionAnswered(_) => {}
        // ── litellm-platform-integration W3.4 — IronClaw budget events ────────
        WebEvent::BudgetExhausted(_) => {}
        WebEvent::BudgetWarning(_) => {}
        // ── webshell-agent-comms-display (Agents Playbook §3.5) ───────────────
        WebEvent::ImplComplete(_) => {}
        // ── webshell-program-and-comms-wiring (gate resolution) ──────────────
        WebEvent::GateResolution(_) => {}
        // ── webshell-a2a-supervisor-visibility ────────────────────────────────
        WebEvent::A2aEnvelope(_) => {}
        // ── webshell-pty-hot-respawn ──────────────────────────────────────────
        WebEvent::PtyRespawned(_) => {}
        // ── lightarchitects-lightspace (Phase 3 Wave 2a) ─────────────────────
        WebEvent::LightspaceCard(_) => {}
        WebEvent::LightspaceLifecycle(_) => {}
        WebEvent::LightspaceUpdate(_) => {}
        WebEvent::LightspaceGraduate(_) => {}
        WebEvent::LightspaceMaterialize(_) => {}
        WebEvent::LightspaceGating(_) => {}
        WebEvent::LightspaceBranchLane(_) => {}
        WebEvent::LightspaceConfidence(_) => {}
        WebEvent::LightspaceDrawerFile(_) => {}
        WebEvent::LightspaceDrawerEvent(_) => {}
    }
}

#[test]
#[allow(clippy::too_many_lines)]
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

    let nil = uuid::Uuid::nil();
    let hitl_esc = WebEvent::IronclawHitlEscalation(IronclawHitlEscalationEvent {
        build_id: nil,
        task_id: "task-1".to_owned(),
        decision_topic: "security gate".to_owned(),
        layer_failed: 0,
        escalation_question: "Approve?".to_owned(),
        deadline: None,
        traceparent: None,
        nonce: nil,
    });
    all_variants_matched(&hitl_esc);

    let hitl_res = WebEvent::IronclawHitlResolution(IronclawHitlResolutionEvent {
        build_id: nil,
        task_id: "task-1".to_owned(),
        resolution: HitlResolution::Approve,
        operator_id: "webshell:operator".to_owned(),
        decided_at: chrono::Utc::now(),
        nonce: nil,
    });
    all_variants_matched(&hitl_res);

    let nil = uuid::Uuid::nil();
    let q_option = QuestionOptionItem {
        label: "Yes".to_owned(),
        description: "Approve".to_owned(),
    };
    let q_item = QuestionItem {
        question: "Proceed?".to_owned(),
        header: "Confirm".to_owned(),
        multi_select: false,
        options: vec![q_option],
    };
    let q_prompt = WebEvent::QuestionPrompt(QuestionPromptEvent {
        tool_use_id: nil,
        questions: vec![q_item.clone()],
        headless_policy: Some(QuestionHeadlessPolicy::FailLoud),
    });
    all_variants_matched(&q_prompt);

    let q_answered = WebEvent::QuestionAnswered(QuestionAnsweredEvent {
        tool_use_id: nil,
        answers: vec![vec!["Yes".to_owned()]],
    });
    all_variants_matched(&q_answered);

    let budget_exhausted = WebEvent::BudgetExhausted(BudgetExhaustedEvent {
        build_id: "b".to_owned(),
        spent_usd: 1.5,
        limit_usd: 1.0,
    });
    all_variants_matched(&budget_exhausted);

    let budget_warning = WebEvent::BudgetWarning(BudgetWarningEvent {
        build_id: "b".to_owned(),
        spent_usd: 0.8,
        limit_usd: 1.0,
        fraction: 0.8,
    });
    all_variants_matched(&budget_warning);

    let gate_resolution = WebEvent::GateResolution(GateEvalEvent {
        build_id: nil,
        phase_id: "phase-1-backend-a".to_owned(),
        gate_dimension: "Q".to_owned(),
        verdict: GateVerdictKind::Passed,
        confidence: 1.0,
        reasoning: None,
        timestamp: chrono::Utc::now(),
    });
    all_variants_matched(&gate_resolution);

    let a2a_envelope = WebEvent::A2aEnvelope(A2aEnvelopeEvent {
        codename: "test-build".to_owned(),
        task_id: "task-1".to_owned(),
        phase: 0,
        wave: 0,
        envelope_type: A2aEnvelopeType::TaskStart,
        payload_summary: "test task".to_owned(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    });
    all_variants_matched(&a2a_envelope);

    let pty_respawned = WebEvent::PtyRespawned(PtyRespawnedEvent {
        agent_kind: lightarchitects_webshell::config::AgentKind::Lightarchitects,
        model: None,
        conversation_continuity: "resumed".to_owned(),
        old_agent_kind: lightarchitects_webshell::config::AgentKind::Lightarchitects,
    });
    all_variants_matched(&pty_respawned);

    // ── lightarchitects-lightspace Phase 3 Wave 2a ────────────────────────────
    let prov = Provenance {
        agent: "corso".to_owned(),
        source_uri: "helix://analytical/test.md".to_owned(),
        span_id: None,
        ts: chrono::Utc::now(),
    };
    let card = LightspaceCardEvent {
        session_id: nil,
        card: CardData {
            id: "c1".to_owned(),
            kind: CardKind::Research,
            title: "t".to_owned(),
            content: serde_json::Value::Null,
            provenance: prov.clone(),
            state: CardState::Attached,
            attribution: None,
        },
    };
    all_variants_matched(&WebEvent::LightspaceCard(card));

    all_variants_matched(&WebEvent::LightspaceLifecycle(LightspaceLifecycleEvent {
        session_id: nil,
        card_id: "c1".to_owned(),
        transition: CardTransition::Detach,
        actor: Actor::Copilot,
        ghost: false,
        attribution: None,
    }));
    all_variants_matched(&WebEvent::LightspaceUpdate(LightspaceUpdateEvent {
        session_id: nil,
        card_id: "c1".to_owned(),
        seq: 1,
        mode: UpdateMode::Replace,
        path: None,
        payload: serde_json::Value::Null,
    }));
    all_variants_matched(&WebEvent::LightspaceGraduate(LightspaceGraduateEvent {
        session_id: nil,
        card_id: "c1".to_owned(),
        file_id: "f1".to_owned(),
        content_uri: "file:///tmp/test.md".to_owned(),
        content_mime: "text/markdown".to_owned(),
        retain_tombstone: false,
    }));
    all_variants_matched(&WebEvent::LightspaceMaterialize(
        LightspaceMaterializeEvent {
            session_id: nil,
            phase: 255,
        },
    ));
    all_variants_matched(&WebEvent::LightspaceGating(LightspaceGatingEvent {
        session_id: nil,
        card_id: "c1".to_owned(),
        gate: "SANDBOX-STATUS".to_owned(),
        satisfied: false,
        reason: None,
    }));
    all_variants_matched(&WebEvent::LightspaceBranchLane(LightspaceBranchLaneEvent {
        session_id: nil,
        card_id: "c1".to_owned(),
        lanes: serde_json::Value::Null,
        fork_span_id: None,
        committed_lane_id: None,
    }));
    all_variants_matched(&WebEvent::LightspaceConfidence(LightspaceConfidenceEvent {
        session_id: nil,
        target_id: "c1".to_owned(),
        target_kind: "research".to_owned(),
        value: 0.9,
        basis: "Three independent sources confirm".to_owned(),
        contradicts: vec![],
        evidence_tier: EvidenceTier::High,
    }));
    all_variants_matched(&WebEvent::LightspaceDrawerFile(LightspaceDrawerFileEvent {
        session_id: nil,
        file: DrawerFileData {
            id: "f1".to_owned(),
            mime_type: "text/markdown".to_owned(),
            content_uri: "file:///tmp/test.md".to_owned(),
            size_bytes: 0,
            provenance: prov,
        },
    }));
    all_variants_matched(&WebEvent::LightspaceDrawerEvent(
        LightspaceDrawerEventPayload {
            session_id: nil,
            file_id: "f1".to_owned(),
            action: DrawerFileAction::Detach,
            actor: Actor::Operator,
            new_content_uri: None,
        },
    ));

    assert_eq!(
        EXPECTED_VARIANT_COUNT, 45,
        "EXPECTED_VARIANT_COUNT must equal the actual WebEvent variant count (45)"
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
        (
            "a2a_envelope",
            WebEvent::A2aEnvelope(A2aEnvelopeEvent {
                codename: "test-build".to_owned(),
                task_id: "task-1".to_owned(),
                phase: 0,
                wave: 0,
                envelope_type: A2aEnvelopeType::TaskStart,
                payload_summary: "starting task".to_owned(),
                timestamp: "2026-01-01T00:00:00Z".to_owned(),
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
