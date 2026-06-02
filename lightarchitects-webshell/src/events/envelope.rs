//! Structured-topic SSE envelope for the webshell event broadcast.
//!
//! [`WebEventV2`] wraps every [`WebEvent`] with a `topic` field (dot-path,
//! `v1.` version prefix, NATS wildcard semantics) and provenance fields
//! (`timestamp`, `agent_id`, `build_id`, `severity`).
//!
//! Topic taxonomy: `v1.<domain>.<entity>.<event>` per Phase 0 D0.2 research
//! (`docs/research/topic-taxonomy.md`, 2026-05-20).
//!
//! ## Security invariant
//!
//! `topic` and `agent_id` are **always gateway-computed** — never derived from
//! client input. This prevents topic-spoofing per CWE-345 and maintains parity
//! with the SOUL FTS5 server-set provenance pattern (OWASP LLM02 nonce-wrap).
//!
//! ## Wire format
//!
//! Serialises as a flat JSON object via `#[serde(flatten)]` on the inner
//! [`WebEvent`]. Browser consumers still receive the `"type"` discriminant
//! alongside the new `"topic"` field — zero contract breakage.
//!
//! ```json
//! {
//!   "topic": "v1.agent.claude.activity",
//!   "timestamp": "2026-05-20T18:00:00Z",
//!   "agent_id": "gateway",
//!   "severity": "info",
//!   "type": "copilot_activity",
//!   "build_id": "...",
//!   "kind": "tool_use",
//!   ...
//! }
//! ```

use chrono::{DateTime, Utc};
use lightarchitects::lightsquad::agent_role::AgentRole;
use serde::Serialize;
use uuid::Uuid;

use crate::events::types::{AyinStatus, WebEvent};

/// UI routing classification for alert styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Informational — no action required.
    Info,
    /// Operator attention warranted (e.g. HITL gate, agent reconnect).
    Warn,
    /// Action required immediately (e.g. security escalation).
    Error,
}

/// Structured-topic SSE envelope wrapping [`WebEvent`].
///
/// Adds `topic` (dot-path, `v1.` version prefix), `timestamp`, `agent_id`,
/// `build_id`, and `severity` to the existing event payload.
///
/// Field naming matches A2A §3.2 (`timestamp`, `agent_id`) per D0.3 research
/// soft recommendations (`docs/research/envelope-non-contradiction.md`,
/// 2026-05-20).
#[derive(Debug, Clone, Serialize)]
pub struct WebEventV2 {
    /// Dot-path topic string with `v1.` version prefix.
    ///
    /// Gateway-computed; never client-controlled.
    /// Wildcards: `*` matches a single segment, `>` matches tail.
    /// Example: `v1.agent.claude.activity`, `v1.conductor.tick`.
    pub topic: String,

    /// Event timestamp (UTC). Matches A2A §3.2 field name.
    pub timestamp: DateTime<Utc>,

    /// Emitting agent identifier. Always [`AgentRole::Gateway`] for
    /// gateway-emitted events. Matches A2A §3.2 field name.
    pub agent_id: String,

    /// Denormalized build UUID for indexed filtering. `None` for global events
    /// that are not scoped to a build session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<Uuid>,

    /// UI routing classification.
    pub severity: Severity,

    /// Inner event — flattened so the `"type"` discriminant from
    /// `#[serde(tag = "type")]` reaches the browser alongside new fields.
    #[serde(flatten)]
    pub inner: WebEvent,
}

impl WebEventV2 {
    /// Wrap a [`WebEvent`] in an envelope, computing `topic` and `severity`.
    #[must_use]
    pub fn from_event(inner: WebEvent, build_id: Option<Uuid>) -> Self {
        let topic = topic_for(&inner);
        let severity = severity_for(&inner);
        Self {
            topic,
            timestamp: Utc::now(),
            agent_id: AgentRole::Gateway.to_string(),
            build_id,
            severity,
            inner,
        }
    }
}

/// Derive the `v1.*` topic string for a [`WebEvent`] variant.
///
/// All topics use the `v1.` version prefix per D0.2 research. This function
/// is exhaustive over [`WebEvent`] variants — the compiler will require updates
/// when new variants are added, preventing topic-less events.
fn topic_for(event: &WebEvent) -> String {
    match event {
        // ── AYIN / observability ────────────────────────────────────────
        WebEvent::AyinSpan(_) => "v1.agent.ayin.span",
        WebEvent::AyinStatus(s) => match s {
            AyinStatus::Connected => "v1.agent.ayin.connected",
            AyinStatus::Disconnected => "v1.agent.ayin.disconnected",
            AyinStatus::Reconnecting { .. } => "v1.agent.ayin.reconnecting",
        },

        // ── Helix / knowledge ───────────────────────────────────────────
        WebEvent::HelixEntry(_) => "v1.helix.entry.changed",
        WebEvent::BuildUpdate(_) => "v1.build.update",
        WebEvent::SoulPromotion(_) => "v1.helix.entry.promoted",

        // ── Control ─────────────────────────────────────────────────────
        WebEvent::Control(_) => "v1.control.command",
        WebEvent::GatewayNotify { .. } => "v1.gateway.notify",

        // ── Agent / strand ───────────────────────────────────────────────
        WebEvent::StrandActivation(_) => "v1.agent.strand.activated",
        WebEvent::StrandConvergence(_) => "v1.agent.strand.convergence",
        WebEvent::AgentFleetUpdate(_) => "v1.agent.fleet.update",

        // ── Copilot / Claude ─────────────────────────────────────────────
        WebEvent::CopilotActivity(_) => "v1.agent.claude.activity",
        WebEvent::CopilotResponse { .. } => "v1.agent.claude.response",
        WebEvent::ContextStatus(_) => "v1.agent.claude.context",

        // ── Build / pillar ───────────────────────────────────────────────
        WebEvent::PillarUpdate(_) => "v1.build.pillar.update",
        WebEvent::SupervisorUpdate(_) => "v1.build.supervisor.update",

        // ── Exec ─────────────────────────────────────────────────────────
        WebEvent::ExecOutput { .. } => "v1.exec.output",
        WebEvent::ExecDone { .. } => "v1.exec.done",

        // ── Worktree / gitforest ─────────────────────────────────────────
        WebEvent::GitForestUpdate { .. } => "v1.worktree.update",

        // ── Conductor / orchestration ────────────────────────────────────
        WebEvent::PermissionRequest { .. } => "v1.conductor.permission.requested",
        WebEvent::Escalation(_) => "v1.conductor.escalation",
        WebEvent::WorkerSlotGauge(_) => "v1.conductor.slot.gauge",
        WebEvent::ConductorTick(_) => "v1.conductor.tick",
        WebEvent::MergeAgentStatus(_) => "v1.conductor.merge.status",
        WebEvent::FixAgentIteration(_) => "v1.conductor.fix.iteration",

        // ── Project identity (webshell-project-ingestion Phase 3) ───────────
        WebEvent::ProjectUpdate(_) => "v1.project.update",

        // ── IronClaw HITL (ironclaw-autonomous-e2e Phase 4) ─────────────────
        WebEvent::IronclawHitlEscalation(_) => "v1.ironclaw.hitl.escalation",
        WebEvent::IronclawHitlResolution(_) => "v1.ironclaw.hitl.resolution",

        // ── webshell-hitl-bridge question tool (Phase 1) ─────────────────────
        WebEvent::QuestionPrompt(_) => "v1.conductor.question.prompt",
        WebEvent::QuestionAnswered(_) => "v1.conductor.question.answered",

        // ── IronClaw budget enforcement (litellm-platform-integration W3.4) ──
        WebEvent::BudgetExhausted(_) => "v1.ironclaw.budget.exhausted",
        WebEvent::BudgetWarning(_) => "v1.ironclaw.budget.warning",

        // ── webshell-agent-comms-display (Agents Playbook §3.5) ─────────────
        WebEvent::ImplComplete(_) => "v1.build.attestation.impl_complete",
    }
    .to_owned()
}

/// Classify the severity of a [`WebEvent`] for UI routing.
fn severity_for(event: &WebEvent) -> Severity {
    match event {
        WebEvent::Escalation(_)
        | WebEvent::PermissionRequest { .. }
        | WebEvent::QuestionPrompt(_)
        | WebEvent::AyinStatus(AyinStatus::Disconnected | AyinStatus::Reconnecting { .. }) => {
            Severity::Warn
        }
        _ => Severity::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `WebEvent` variant must produce a non-empty topic with "v1." prefix.
    ///
    /// This test is the Phase 1 D1.3 parity gate. When new variants are added
    /// to `WebEvent`, this test will fail to compile (exhaustive match in
    /// `topic_for`), forcing the developer to assign a topic.
    #[allow(clippy::too_many_lines)]
    #[test]
    fn all_variants_produce_v1_topics() {
        use crate::events::types::{
            AyinStatus, BuildEventKind, BuildUpdateEvent, ConductorTickEvent, ContextStatusEvent,
            ControlCommand, CopilotActivityEvent, EscalationEvent, FixAgentIterationEvent,
            HelixEntrySummary, HelixEventKind, MergeAgentStatusEvent, NorthstarEvaluationEvent,
            PillarUpdateEvent, ProjectUpdateKind, ProjectUpdatePayload, RiskTier,
            StrandActivationEvent, StrandConvergenceEvent, TraceSpanSummary, WorkerSlotGaugeEvent,
        };
        use crate::gitforest::{
            BranchKind, BranchLifecycle, BranchOverlayMeta, CiStatus, HitlState,
        };
        use crate::memory::types::{MemoryTier, PromotionEvent};

        // Build a representative instance of each variant and verify topic.
        let cases: &[WebEvent] = &[
            WebEvent::AyinSpan(TraceSpanSummary {
                id: "x".into(),
                parent_id: None,
                session_id: None,
                actor: "gateway".into(),
                action: "test".into(),
                timestamp: Utc::now().to_rfc3339(),
                duration_ms: 1,
                outcome: serde_json::Value::Null,
                metadata: serde_json::Value::Null,
                strand_activations: vec![],
                decision_points: vec![],
            }),
            WebEvent::AyinStatus(AyinStatus::Connected),
            WebEvent::AyinStatus(AyinStatus::Disconnected),
            WebEvent::AyinStatus(AyinStatus::Reconnecting { attempt: 1 }),
            WebEvent::HelixEntry(HelixEntrySummary {
                path: "eva/entries/test.md".into(),
                event_kind: HelixEventKind::Created,
                sibling: None,
                significance: None,
                strands: vec![],
                content_excerpt: None,
                created_at: None,
                kind: None,
            }),
            WebEvent::BuildUpdate(BuildUpdateEvent {
                path: "corso/builds/active.yaml".into(),
                event_kind: BuildEventKind::Modified,
            }),
            WebEvent::Control(ControlCommand::Notify {
                message: "test notification".into(),
                level: "info".into(),
            }),
            WebEvent::StrandActivation(StrandActivationEvent {
                sibling: "claude".into(),
                strand: "analytical".into(),
                weight: 1.0,
                timestamp: Utc::now().to_rfc3339(),
            }),
            WebEvent::SoulPromotion(PromotionEvent {
                memo_id: "m".into(),
                from: MemoryTier::Hot,
                to: MemoryTier::Cold,
                path: "soul/entries/test.md".into(),
                sibling: "soul".into(),
                significance: 7.5,
                promoted_at: Utc::now().to_rfc3339(),
            }),
            WebEvent::GatewayNotify {
                payload: serde_json::Value::Null,
            },
            WebEvent::PillarUpdate(PillarUpdateEvent {
                build_id: "b".into(),
                pillar: "arch".into(),
                phase: "started".into(),
                line: None,
                exit_code: None,
                artifact: None,
            }),
            WebEvent::StrandConvergence(StrandConvergenceEvent {
                strand: "analytical".into(),
                siblings: vec!["corso".into(), "eva".into(), "soul".into()],
                memo_ids: vec![],
                detected_at: Utc::now().to_rfc3339(),
            }),
            WebEvent::CopilotActivity(CopilotActivityEvent {
                build_id: "b".into(),
                kind: "assistant".into(),
                summary: None,
                raw: serde_json::Value::Null,
                timestamp: Utc::now().to_rfc3339(),
            }),
            WebEvent::CopilotResponse {
                chunk: "hello".into(),
                done: false,
                sibling: None,
                turn_span_id: None,
            },
            WebEvent::PermissionRequest {
                call_id: "c".into(),
                input_preview: "test: {}".into(),
                risk_tier: RiskTier::Low,
            },
            WebEvent::ContextStatus(ContextStatusEvent {
                usage_pct: 0.5,
                level: None,
                budget: 200_000,
                used: 1_000,
            }),
            WebEvent::SupervisorUpdate(NorthstarEvaluationEvent {
                build_id: "b".into(),
                wave_num: 0,
                status: "advancing".into(),
                confidence: 0.9,
                recommended_next: "continue".into(),
                proposal_pending: false,
            }),
            WebEvent::ExecOutput {
                handle: "h".into(),
                seq: 0,
                stream: "stdout".into(),
                line: "output".into(),
            },
            WebEvent::GitForestUpdate {
                repo: "lightarchitects-sdk".into(),
                root: crate::gitforest::BranchNode {
                    id: "main".into(),
                    name: "main".into(),
                    kind: BranchKind::Main,
                    parent_id: None,
                    depth: 0,
                    fork_commit_sha: None,
                    fork_position: 0.0,
                    children: vec![],
                    overlay: BranchOverlayMeta {
                        phase: None,
                        gate_score: None,
                        age_days: 0,
                        ci_status: CiStatus::Success,
                        hitl_state: HitlState::None,
                        model_attribution: vec![],
                        lifecycle: BranchLifecycle::LiveActive,
                        merged_at: None,
                        merged_to: None,
                        fade_level: 1.0,
                    },
                    build_progress: None,
                    worktrees: vec![],
                },
            },
            WebEvent::ExecDone {
                handle: "h".into(),
                exit_code: Some(0),
                killed: false,
            },
            WebEvent::Escalation(EscalationEvent {
                build_id: "b".into(),
                wave_index: 0,
                worker_slot: 0,
                reason: "test escalation".into(),
                call_id: "c".into(),
            }),
            WebEvent::WorkerSlotGauge(WorkerSlotGaugeEvent {
                build_id: "b".into(),
                wave_index: 0,
                active: 3,
                capacity: 7,
            }),
            WebEvent::ConductorTick(ConductorTickEvent {
                build_id: "b".into(),
                tick_seq: 1,
                queue_depth: 0,
                active_workers: 0,
            }),
            WebEvent::MergeAgentStatus(MergeAgentStatusEvent {
                build_id: "b".into(),
                wave_index: 0,
                phase: "started".into(),
                commit_sha: None,
            }),
            WebEvent::FixAgentIteration(FixAgentIterationEvent {
                build_id: "b".into(),
                wave_index: 0,
                worker_slot: 0,
                iteration: 1,
                issue_summary: "test issue".into(),
            }),
            WebEvent::ProjectUpdate(ProjectUpdatePayload {
                project_id: Uuid::nil(),
                slug: "test-project".into(),
                kind: ProjectUpdateKind::Created,
            }),
            // webshell-hitl-bridge (Phase 1)
            WebEvent::QuestionPrompt(crate::events::types::QuestionPromptEvent {
                tool_use_id: Uuid::nil(),
                questions: vec![crate::events::types::QuestionItem {
                    question: "Pick one".into(),
                    header: "Choice".into(),
                    multi_select: false,
                    options: vec![crate::events::types::QuestionOptionItem {
                        label: "A".into(),
                        description: "Option A".into(),
                    }],
                }],
                headless_policy: None,
            }),
            WebEvent::QuestionAnswered(crate::events::types::QuestionAnsweredEvent {
                tool_use_id: Uuid::nil(),
                answers: vec![vec!["A".into()]],
            }),
            // litellm-platform-integration W3.4 — budget enforcement events
            WebEvent::BudgetExhausted(crate::events::types::BudgetExhaustedEvent {
                build_id: "b".into(),
                spent_usd: 1.5,
                limit_usd: 1.0,
            }),
            WebEvent::BudgetWarning(crate::events::types::BudgetWarningEvent {
                build_id: "b".into(),
                spent_usd: 0.8,
                limit_usd: 1.0,
                fraction: 0.8,
            }),
            // webshell-agent-comms-display (Agents Playbook §3.5)
            WebEvent::ImplComplete(crate::events::types::ImplCompleteEvent {
                build_id: Uuid::nil(),
                wave: 0,
                task_id: "t".into(),
                agent_id: "claude-code".into(),
                commit_sha: "abc1234".into(),
                gates_passed: vec![],
                gates_skipped: vec![],
                file_content_span_id: None,
                ayin_spans_dropped_total: 0,
                trust_boundary: "unverified_pre_2.10".into(),
                spec_compliance_claim: None,
                confidence: 1.0,
                timestamp: Utc::now(),
            }),
        ];

        // AgentFleetUpdate requires lightarchitects::fleet::FleetSnapshot which
        // is not easily constructed in tests; topic is verified structurally.
        // The match in topic_for is exhaustive — compiler catches missing variants.

        for ev in cases {
            let topic = topic_for(ev);
            assert!(
                topic.starts_with("v1."),
                "topic for {:?} does not start with 'v1.': {topic}",
                std::mem::discriminant(ev)
            );
            assert!(!topic.is_empty(), "topic must not be empty");
            let env = WebEventV2::from_event(ev.clone(), None);
            assert_eq!(env.topic, topic);
            assert_eq!(env.agent_id, "gateway");
        }
    }

    #[test]
    fn severity_escalation_is_warn() {
        use crate::events::types::EscalationEvent;
        let ev = WebEvent::Escalation(EscalationEvent {
            build_id: "b".into(),
            wave_index: 0,
            worker_slot: 0,
            reason: "risk threshold exceeded".into(),
            call_id: "c".into(),
        });
        assert_eq!(severity_for(&ev), Severity::Warn);
    }

    #[test]
    fn severity_copilot_is_info() {
        use crate::events::types::CopilotActivityEvent;
        let ev = WebEvent::CopilotActivity(CopilotActivityEvent {
            build_id: "b".into(),
            kind: "assistant".into(),
            summary: None,
            raw: serde_json::Value::Null,
            timestamp: "2026-05-20T00:00:00Z".into(),
        });
        assert_eq!(severity_for(&ev), Severity::Info);
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn envelope_serialises_with_type_field() {
        use crate::events::types::ConductorTickEvent;
        let ev = WebEvent::ConductorTick(ConductorTickEvent {
            build_id: "b".into(),
            tick_seq: 42,
            queue_depth: 3,
            active_workers: 1,
        });
        let envelope = WebEventV2::from_event(ev, None);
        let json = serde_json::to_string(&envelope).unwrap();
        let obj: serde_json::Value = serde_json::from_str(&json).unwrap();
        // legacy "type" discriminant preserved via flatten
        assert_eq!(obj["type"], "conductor_tick");
        // new topic field present
        assert_eq!(obj["topic"], "v1.conductor.tick");
        assert_eq!(obj["agent_id"], "gateway");
        assert_eq!(obj["severity"], "info");
        // inner field preserved
        assert_eq!(obj["tick_seq"], 42);
    }
}
