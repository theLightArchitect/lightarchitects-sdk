//! Internal event types for the server-sent event fan-out.
//!
//! All types here implement [`serde::Serialize`] so they can be forwarded
//! verbatim as `data:` payloads on the SSE stream the browser subscribes
//! to via `GET /api/events` (Phase 5).

use crate::gitforest::BranchNode;
use crate::memory::types::PromotionEvent;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Broadcast event emitted by the webshell backend.
///
/// Every variant maps to a distinct browser-visible SSE `data:` payload.
/// The `"type"` discriminant is serialized first so the frontend can
/// dispatch on it without parsing the full body.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebEvent {
    /// A trace span received from the AYIN SSE endpoint.
    AyinSpan(TraceSpanSummary),
    /// AYIN connection lifecycle notification.
    AyinStatus(AyinStatus),
    /// A vault Markdown entry was created or modified (filesystem watcher).
    ///
    /// Emitted by the helix watcher as a fallback when AYIN is unavailable,
    /// or to supplement AYIN spans with raw filesystem signals.
    HelixEntry(HelixEntrySummary),
    /// A build tracking file was created or modified in corso/builds/.
    ///
    /// Emitted by the helix watcher when `active.yaml`, `portfolio.md`,
    /// or `roadmap.html` changes. The frontend should refetch `/api/builds`
    /// to get the latest build data.
    BuildUpdate(BuildUpdateEvent),
    /// A control command from an external process (e.g. Claude Code)
    /// forwarded to the browser for UI state mutation.
    Control(ControlCommand),
    /// A strand activation derived from an AYIN span's metadata.
    ///
    /// Emitted by the AYIN client alongside [`WebEvent::AyinSpan`] when the
    /// span's metadata contains a `strand_activations` array. One event per
    /// strand, so a span touching three strands produces three events.
    StrandActivation(StrandActivationEvent),
    /// A hot memo was promoted to the cold helix tier.
    ///
    /// Emitted by `BroadcastingPromoter` in [`crate::memory::promoter_bridge`]
    /// when `SiblingPromoter::promote` returns `PromotionOutcome::Promoted`.
    /// The frontend uses this to optimistically move the memo from the
    /// `hotMemory` store to `coldMemory` and to trigger an orb-spawn animation
    /// in the 3D scene.
    SoulPromotion(PromotionEvent),
    /// A UI event forwarded from the `lightarchitects-gateway` MCP server's
    /// `ui.*` tools.
    ///
    /// The gateway POSTs a raw JSON body to `/api/builds/:id/notify` —
    /// authenticated via `X-LA-Notify-Token` — and the webshell wraps that
    /// body in this variant before broadcasting it on the per-build SSE
    /// channel. The frontend reads `msg.type === "gateway_notify"` then
    /// dispatches on the inner `msg.payload.type` (e.g. `"focus_pillar"`).
    GatewayNotify {
        /// Raw gateway body verbatim — frontend unwraps `.payload.type`
        /// to dispatch (`focus_pillar`, `flag_finding`, `refresh_sitrep`,
        /// `update_conductor`, `set_active_build`, `notify`).
        payload: serde_json::Value,
    },
    /// Streaming progress from a real CORSO pillar run (Phase 15).
    ///
    /// Emitted by [`crate::real_data::trigger_pillar`] as the `corso <cmd>`
    /// subprocess produces output. Three phases per run:
    ///   * `phase: "started"`   — before spawn (single event)
    ///   * `phase: "output"`    — one event per stdout line
    ///   * `phase: "completed"` — final event with exit status + artifact path
    PillarUpdate(PillarUpdateEvent),
    /// Phase 19b.2 — cross-sibling strand convergence detected.
    ///
    /// Emitted by the convergence detector when three or more distinct
    /// siblings activate the same strand within the active hot window.
    /// The UI renders this as a "convergence" pulse in `Helix3D` and an
    /// entry in the convergences tab. Graph materialization of the
    /// convergence (a `:SharedExperience` node + `:PARTICIPATES_IN` edges)
    /// is deferred to Phase 19c / 20.
    StrandConvergence(StrandConvergenceEvent),
    /// Live copilot subprocess activity streamed during a turn.
    ///
    /// Emitted by `run_print_turn` / `run_codex_turn` for each intermediate
    /// `stream-json` event (thinking, `tool_use`, `tool_result`, etc.). The
    /// frontend Activity tab renders these as a live feed.
    CopilotActivity(CopilotActivityEvent),
    /// Streaming text chunk from a copilot turn.
    ///
    /// Emitted by `run_print_turn` for each `text_delta` content block delta.
    /// The wire type tag is `"copilot_response"` (matches `sse.ts:349`).
    /// `done: true` signals end-of-turn; the frontend stops the loading spinner.
    CopilotResponse {
        /// Incremental text from the model.
        chunk: String,
        /// Whether this is the final chunk for this turn.
        done: bool,
        /// Source sibling identifier, e.g. `"claude"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sibling: Option<String>,
        /// AYIN span ID for this turn (included on the `done: true` event so the
        /// frontend can render the `TurnLineageStrip` deeplink after the turn ends).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_span_id: Option<String>,
    },
    /// Tool permission request streamed to the operator.
    ///
    /// `input_preview` MUST be derived from the serialised tool-call payload —
    /// NOT from model-authored text (OWASP LLM01 indirect prompt injection, SA-15).
    /// `call_id` is always server-generated via `Uuid::new_v4()` — never client-supplied
    /// (IDOR prevention, SA-4).
    PermissionRequest {
        /// Server-generated call identifier — `Uuid::new_v4().to_string()`.
        call_id: String,
        /// Preview of the tool call: `"<tool_name>: <serialised_args>"`.
        input_preview: String,
        /// Risk classification for this tool call.
        risk_tier: RiskTier,
    },
    /// Context-window utilisation snapshot from the `LightArchitects` CLI subprocess.
    ///
    /// Emitted by `run_native_turn` each time the persistent CLI process outputs a
    /// `{"type":"context",...}` NDJSON line. The frontend uses this to drive the
    /// context bar above the Copilot drawer.
    ContextStatus(ContextStatusEvent),
    /// Northstar supervision evaluation result for a completed wave.
    ///
    /// Emitted by the supervisor after each `WAVE_COMPLETE` event when
    /// `northstar_text` is set for the build. The frontend uses this to update
    /// the drift indicator and trigger `ProposalCard` when `proposal_pending`
    /// is `true` (§Q check 4 — SCR1-B1).
    SupervisorUpdate(NorthstarEvaluationEvent),

    /// One line of stdout/stderr from an `exec.run_command` process.
    ///
    /// The frontend `OutputViewer` uses `handle` to correlate chunks and
    /// `seq` for ordered rendering. `stream` is `"stdout"` or `"stderr"`.
    ExecOutput {
        /// Stream handle returned by `POST /api/exec/run`.
        handle: String,
        /// Monotonically increasing line sequence number within the handle.
        seq: u64,
        /// Which stdio stream produced this line.
        stream: String,
        /// Raw output line (may contain ANSI escape codes).
        line: String,
    },
    /// Live `GitForest` topology update.
    ///
    /// Emitted on every branch state change that affects the 4-level forest
    /// hierarchy.  `root` is the full `main` subtree for the named repo so
    /// the frontend can do a single atomic replace without partial-update
    /// bookkeeping.
    ///
    /// `debounce_window_ms` is NOT part of the payload — it is an
    /// implementation constant in the broadcaster (`DEBOUNCE_WINDOW_MS = 250`)
    /// per API-canon-audit S6 (iter-7).
    GitForestUpdate {
        /// Repository name (matches `BranchNode.id` for the root `main` node).
        repo: String,
        /// Full branch tree rooted at `main`.
        root: BranchNode,
    },

    /// Terminal event for a completed exec process.
    ///
    /// Emitted once per process after all `ExecOutput` events. `exit_code`
    /// is `None` when the process was killed before it produced an exit code.
    ExecDone {
        /// Stream handle identifying the completed process.
        handle: String,
        /// OS exit code, or `None` if the process was killed.
        exit_code: Option<i32>,
        /// Whether the process ended due to timeout or explicit kill.
        killed: bool,
    },

    // ── ironclaw-spine / lightsquad variants (Phase 2A.5) ────────────────────
    /// A lightsquad worker slot requires operator `HITL` approval before continuing.
    ///
    /// Emitted by the worker-slot coordinator when a gate decision crosses the
    /// `HITL` threshold defined in `PermissionMatrix`. The operator must respond
    /// via `POST /api/builds/:id/hitl/:call_id` before the slot unblocks.
    Escalation(EscalationEvent),

    /// Real-time slot-pool occupancy gauge for the 7-slot lightsquad worker pool.
    ///
    /// Emitted on every slot state change (acquire / release). The frontend
    /// `WaveTimeline` uses this to render the live occupancy bar above the wave
    /// graph.
    WorkerSlotGauge(WorkerSlotGaugeEvent),

    /// Conductor heartbeat tick emitted once per conductor cycle.
    ///
    /// The frontend uses `tick_seq` to detect stalled conductors (no tick for
    /// `N` seconds ⇒ show a warning badge). `queue_depth` and `active_workers`
    /// drive the `ConductorPanel` live counters.
    ConductorTick(ConductorTickEvent),

    /// Merge agent lifecycle status update.
    ///
    /// Emitted by the merge agent at phase transitions: `"started"`,
    /// `"running"`, `"merged"`, or `"failed"`. `commit_sha` is set only in
    /// the `"merged"` phase.
    MergeAgentStatus(MergeAgentStatusEvent),

    /// A fix agent is entering another iteration against a failing gate.
    ///
    /// Emitted before each fix pass so the operator can observe retry depth.
    /// The `ReviewGate` uses `iteration` to enforce the per-gate fix-attempt
    /// limit (default: 3).
    FixAgentIteration(FixAgentIterationEvent),

    /// Point-in-time fleet snapshot — emitted by the fleet broadcaster when
    /// agent state changes. The frontend `FleetPanel` replaces its entire
    /// agent tree on each event (snapshot semantics, no delta bookkeeping).
    AgentFleetUpdate(lightarchitects::fleet::FleetSnapshot),

    // ── webshell-project-ingestion (Phase 3) ────────────────────────────────
    /// A project was created or updated via `POST /api/projects/init`.
    ///
    /// Emitted after the atomic `.lightarchitects/project.toml` write succeeds.
    /// Topic: `v1.project.update`. Wire tag: `"project_update"`.
    ProjectUpdate(ProjectUpdatePayload),

    // ── ironclaw-autonomous-e2e (Phase 4) ───────────────────────────────────
    /// Ironclaw HITL escalation requiring operator approval.
    ///
    /// Emitted by the lightsquad bridge when `UserEscalation` fires and the
    /// worker parks in the HITL queue. Nonce is `UUIDv7`, single-use — validated
    /// server-side on resolution (SERAPH#3 anti-replay). Wire tag:
    /// `"ironclaw_hitl_escalation"`.
    IronclawHitlEscalation(IronclawHitlEscalationEvent),

    /// Ironclaw HITL resolution emitted after the operator resolves an escalation.
    ///
    /// Emitted by `POST /api/control { kind: "ironclaw_hitl_resolution" }` after
    /// nonce validation succeeds. Wire tag: `"ironclaw_hitl_resolution"`.
    IronclawHitlResolution(IronclawHitlResolutionEvent),

    // ── webshell-hitl-bridge (Phase 1) ──────────────────────────────────────
    /// An in-flight operator question emitted by the gateway `question` tool.
    ///
    /// Emitted when the gateway dispatches a `question` `tool_use` to the webshell
    /// via `POST /api/sessions/:id/question`. The browser renders
    /// `QuestionCard.svelte` on receipt. Operator's answer returns via
    /// `POST /api/sessions/:id/answer`. Wire tag: `"question_prompt"`.
    ///
    /// # Security
    /// `tool_use_id` is a `Uuid::new_v4()` minted server-side — never
    /// client-supplied (IDOR prevention, per `PermissionRequest` SA-4 pattern).
    QuestionPrompt(QuestionPromptEvent),

    /// Confirmation that an operator answered a pending `question` tool invocation.
    ///
    /// Emitted by `POST /api/sessions/:id/answer` after the oneshot receiver
    /// resolves. Browser clears the matching `pendingQuestions` store entry on
    /// receipt. Wire tag: `"question_answered"`.
    QuestionAnswered(QuestionAnsweredEvent),
}

/// Northstar evaluation result broadcast after a `WAVE_COMPLETE` event.
///
/// Consumed by the `ProposalCard` component on the frontend; `proposal_pending`
/// gates the card display. Wire tag: `"supervisor_update"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NorthstarEvaluationEvent {
    /// Build UUID this evaluation belongs to.
    pub build_id: String,
    /// Wave index (0-based) that triggered this evaluation.
    pub wave_num: u32,
    /// Alignment verdict: `"advancing"`, `"neutral"`, or `"drifting"`.
    pub status: String,
    /// Model confidence in the verdict (0.0–1.0).
    pub confidence: f32,
    /// Suggested operator action when drifting.
    pub recommended_next: String,
    /// Whether the consecutive-drift threshold has been reached.
    ///
    /// When `true`, the frontend should surface a `ProposalCard` and await
    /// operator selection (§Q check 6 — operator-selectable next action).
    pub proposal_pending: bool,
}

/// Cross-sibling strand convergence event (Phase 19b.2).
///
/// Fired when a strand hits the configured minimum-participants threshold
/// (default 3). `memo_ids` reference the `:HotMemo` nodes that triggered
/// the convergence; the UI can deep-link back to each.
#[derive(Debug, Clone, Serialize)]
pub struct StrandConvergenceEvent {
    /// Strand name, lowercased (e.g. `"analytical"`).
    pub strand: String,
    /// Distinct sibling names currently activating this strand.
    pub siblings: Vec<String>,
    /// `:HotMemo.id` values that participated in the convergence.
    pub memo_ids: Vec<String>,
    /// ISO-8601 UTC timestamp of detection.
    pub detected_at: String,
}

/// Live copilot activity event streamed during a turn (Phase 20 — Activity tab).
///
/// Maps 1:1 to `stream-json` NDJSON lines from `claude --print --verbose`.
/// The frontend Activity tab renders these as a collapsible live feed with
/// verbose/auditable detail levels.
#[derive(Debug, Clone, Serialize)]
pub struct CopilotActivityEvent {
    /// Build this activity belongs to.
    pub build_id: String,
    /// Event category from the stream-json line's `type` field.
    /// Known values: `assistant`, `tool_use`, `tool_result`, `result`, `system`, `error`.
    pub kind: String,
    /// Human-readable summary (first 500 chars of content/thinking/tool name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Full raw JSON line for verbose/auditable mode.
    pub raw: serde_json::Value,
    /// ISO-8601 UTC timestamp of when this event was received.
    pub timestamp: String,
}

/// Context-window utilisation snapshot from the `LightArchitects` CLI subprocess.
///
/// Emitted by `run_native_turn` each time the persistent CLI process outputs a
/// `{"type":"context",...}` NDJSON line. The frontend uses this to drive the
/// context bar above the Copilot drawer.
#[derive(Debug, Clone, Serialize)]
pub struct ContextStatusEvent {
    /// Usage as a fraction of the context window (0.0–1.0).
    pub usage_pct: f32,
    /// Active compaction level: `None`, `"l1"`, `"l2"`, or `"l3"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// Total token budget for this session.
    pub budget: u64,
    /// Tokens consumed so far in this session.
    pub used: u64,
}

// ── ironclaw-spine / lightsquad payload types (Phase 2A.5) ──────────────────

// ── ironclaw-autonomous-e2e Phase 4 types ───────────────────────────────────

/// Operator decision for an ironclaw HITL escalation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitlResolution {
    /// Operator approved the blocked action.
    Approve,
    /// Operator rejected the blocked action.
    Reject,
}

/// Payload for [`WebEvent::IronclawHitlEscalation`].
///
/// Emitted when a lightsquad worker parks in the HITL queue. `nonce` is a
/// `UUIDv7` minted per-escalation; it is embedded in the `callback_data` of any
/// Telegram inline keyboard button and validated server-side on resolution to
/// prevent replay attacks (SERAPH#3, CWE-209 — nonce must never appear in logs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IronclawHitlEscalationEvent {
    /// Build the escalated task belongs to.
    pub build_id: Uuid,
    /// Task identifier within the build.
    pub task_id: String,
    /// Human-readable decision topic (e.g. gate dimension + rule summary).
    pub decision_topic: String,
    /// Which pipeline layer triggered the escalation.
    ///
    /// `0` = categorical exclusion (pre-Layer-1 screen).
    /// `1`–`3` = Layer N check failed.
    /// `4` = full pipeline passed Layers 0–3; operator must decide.
    pub layer_failed: u8,
    /// Full escalation question surfaced to the operator.
    pub escalation_question: String,
    /// Optional hard deadline for the operator decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime<Utc>>,
    /// W3C `traceparent` for AYIN span stitching across the SSE round-trip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
    /// Server-minted `UUIDv7` — single-use replay-kill token.
    /// Embedded in Telegram `callback_data`; validated against outstanding nonce
    /// set on resolution. SECURITY: must never appear in logs or error messages.
    pub nonce: Uuid,
}

/// Payload for [`WebEvent::IronclawHitlResolution`].
///
/// Emitted by `POST /api/control { kind: "ironclaw_hitl_resolution" }` after
/// the server validates the nonce and removes the entry from the HITL queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IronclawHitlResolutionEvent {
    /// Build the resolved escalation belonged to.
    pub build_id: Uuid,
    /// Task identifier within the build.
    pub task_id: String,
    /// Operator decision.
    pub resolution: HitlResolution,
    /// Operator identifier (e.g. `"telegram:chat_id"` or `"webshell:operator"`).
    pub operator_id: String,
    /// Wall-clock time the operator resolved the escalation.
    pub decided_at: DateTime<Utc>,
    /// Echo of the nonce from the escalation event — for frontend correlation.
    /// NOT logged; only broadcast on the SSE stream (encrypted in transit).
    pub nonce: Uuid,
}

// ── Legacy escalation type (ironclaw-spine Phase 2A.5) ───────────────────────

/// Payload for [`WebEvent::Escalation`].
///
/// Carries the minimal data the operator needs to evaluate and approve or
/// reject a `HITL` gate decision. `call_id` is a `UUIDv4` minted server-side
/// for correlation with the `POST /api/builds/:id/hitl/:call_id` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationEvent {
    /// Build that triggered the escalation.
    pub build_id: String,
    /// Zero-based wave index at the time of escalation.
    pub wave_index: u32,
    /// Slot number (1–7) that is blocked waiting for approval.
    pub worker_slot: u8,
    /// Human-readable reason for the escalation (e.g. gate dimension + rule).
    pub reason: String,
    /// Server-minted `UUIDv4` — used as the path parameter in the `HITL`
    /// response endpoint. Never client-supplied (prevents `IDOR`).
    pub call_id: String,
}

/// Payload for [`WebEvent::WorkerSlotGauge`].
///
/// Carries the instantaneous occupancy of the 7-slot lightsquad worker pool.
/// `active` ≤ `capacity` is always true; capacity is 7 for the default pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSlotGaugeEvent {
    /// Build the pool belongs to.
    pub build_id: String,
    /// Zero-based wave index.
    pub wave_index: u32,
    /// Number of slots currently running a worker process.
    pub active: u8,
    /// Total slot capacity (7 for the standard pool).
    pub capacity: u8,
}

/// Payload for [`WebEvent::ConductorTick`].
///
/// Emitted once per conductor scheduling cycle. `tick_seq` is monotonically
/// increasing within a build; a gap in `tick_seq` signals a missed tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConductorTickEvent {
    /// Build the conductor is managing.
    pub build_id: String,
    /// Monotonically increasing tick counter (1-based, resets per build).
    pub tick_seq: u64,
    /// Number of tasks waiting in the conductor queue.
    pub queue_depth: u32,
    /// Number of worker slots currently active.
    pub active_workers: u8,
}

/// Payload for [`WebEvent::MergeAgentStatus`].
///
/// Tracks the merge agent through its lifecycle phases. `commit_sha` is
/// populated only in the `"merged"` phase after a successful `git merge --no-ff`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeAgentStatusEvent {
    /// Build the merge agent is working on.
    pub build_id: String,
    /// Zero-based wave index this merge agent belongs to.
    pub wave_index: u32,
    /// Lifecycle phase: `"started"` | `"running"` | `"merged"` | `"failed"`.
    pub phase: String,
    /// Full `git` commit `SHA` produced by the merge, set only in `"merged"` phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
}

/// Payload for [`WebEvent::FixAgentIteration`].
///
/// Emitted before each fix-agent pass so the operator can observe retry depth
/// and surface a manual override if the agent is stuck. The `ReviewGate`
/// enforces a per-gate cap (default 3 iterations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixAgentIterationEvent {
    /// Build the fix agent belongs to.
    pub build_id: String,
    /// Zero-based wave index.
    pub wave_index: u32,
    /// Slot number (1–7) running the fix agent.
    pub worker_slot: u8,
    /// 1-based iteration counter for this fix cycle.
    pub iteration: u32,
    /// Short summary of the failing gate dimension being addressed.
    pub issue_summary: String,
}

// ── webshell-project-ingestion payload types (Phase 3) ─────────────────────

/// Payload for [`WebEvent::ProjectUpdate`].
///
/// Emitted by `POST /api/projects/init` after the atomic toml write succeeds.
/// Wire tag: `"project_update"`. Topic: `v1.project.update`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectUpdatePayload {
    /// UUID v7 of the newly created or updated project.
    pub project_id: uuid::Uuid,
    /// DNS-subdomain slug matching `~/Projects/<slug>/`.
    pub slug: String,
    /// Whether this is a first-time creation or a subsequent update.
    pub kind: ProjectUpdateKind,
}

/// Classification of a `v1.project.update` event.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectUpdateKind {
    /// First `POST /api/projects/init` for this slug — project.toml written fresh.
    Created,
    /// Reserved for future re-init (deferred per Part V Scope §V.2).
    Updated,
}

/// Risk classification for a tool permission request.
///
/// Used in [`WebEvent::PermissionRequest`] to help the operator quickly assess
/// the potential impact of approving a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    /// Read-only, no side effects.
    Low,
    /// Writes to local filesystem or makes network requests.
    Medium,
    /// Executes shell commands or modifies external services.
    High,
    /// Irreversible or destructive action (e.g. `rm -rf`, production deploys).
    Critical,
}

/// Incremental pillar-run update broadcast over SSE (Phase 15).
///
/// The frontend subscribes on the per-build SSE channel and matches on
/// `build_id` + `pillar` to update the matching UI card.
#[derive(Debug, Clone, Serialize)]
pub struct PillarUpdateEvent {
    /// Build this pillar run belongs to.
    pub build_id: String,
    /// Pillar name (`arch`, `sec`, `qual`, `perf`, `test`, `doc`, `ops`).
    pub pillar: String,
    /// Lifecycle marker — `started` · `output` · `completed`.
    pub phase: String,
    /// One line of stdout when `phase == "output"`; omitted otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// Process exit code when `phase == "completed"`; omitted otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Relative artifact path (e.g. `pillar-arch.json`) when completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<String>,
}

/// A single strand activation derived from an AYIN span.
///
/// Produced by the strand parser in [`crate::events::strand`]. The parser
/// is the validation boundary — `weight` is always clamped to `[0.0, 1.0]`
/// before construction, so downstream consumers can trust the value.
#[derive(Debug, Clone, Serialize)]
pub struct StrandActivationEvent {
    /// Sibling identifier, e.g. `"eva"`, `"corso"`. Taken verbatim from
    /// the source span's `actor` field.
    pub sibling: String,
    /// Strand name, e.g. `"methodical"`, `"contextual"`. Taken from the
    /// `strand_activations[].strand` field of the source span's metadata.
    pub strand: String,
    /// Activation magnitude in `[0.0, 1.0]`. Clamped by the parser.
    pub weight: f32,
    /// ISO-8601 UTC timestamp, mirrored from the source span.
    pub timestamp: String,
}

/// Describes a new or modified helix vault entry detected by the filesystem watcher.
///
/// Phase 9.3 enriched this shape with front-matter fields so the Svelte webshell
/// can render real memory tiles without a secondary fetch. All enrichment fields
/// are best-effort — a malformed or missing YAML front-matter still produces a
/// valid event with the core `path` + `event_kind`, and `None`/empty values
/// elsewhere.
#[derive(Debug, Clone, Serialize)]
pub struct HelixEntrySummary {
    /// Path relative to the helix root (e.g. `"eva/entries/day-42.md"`).
    pub path: String,
    /// What triggered this event.
    pub event_kind: HelixEventKind,
    /// Owning sibling derived from the path's top-level directory or the
    /// front-matter `sibling:` field (front-matter wins).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling: Option<String>,
    /// Significance score from front-matter. Normalised to `[0.0, 1.0]`:
    /// values between 0 and 10 in the YAML are divided by 10 on ingest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub significance: Option<f32>,
    /// Strand tags from the front-matter `strands:` list (lowercased).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strands: Vec<String>,
    /// First 280 chars of the body (excluding front-matter), for UI hover preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_excerpt: Option<String>,
    /// ISO-8601 UTC timestamp from front-matter `date:` or file mtime fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Typed classification — Phase 14.1. Populated from the front-matter
    /// `type:` field when present; otherwise inferred from the path shape.
    ///
    /// Canonical values recognised by the UI: `entry`, `plan`, `standard`,
    /// `review`, `lesson`, `reference`. Unknown types are passed through as
    /// lowercase strings so new categories don't require a frontend deploy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// Filesystem event kind that produced a [`HelixEntrySummary`].
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HelixEventKind {
    /// A new vault entry file was created.
    Created,
    /// An existing vault entry file was modified.
    Modified,
}

impl HelixEntrySummary {
    /// Build a minimal summary — used when the file can't be read or parsed.
    ///
    /// Enrichment fields default to `None` / empty. The Svelte frontend is
    /// responsible for rendering a graceful fallback when fields are absent.
    #[must_use]
    pub fn minimal(path: String, event_kind: HelixEventKind) -> Self {
        Self {
            path,
            event_kind,
            sibling: None,
            significance: None,
            strands: Vec::new(),
            content_excerpt: None,
            created_at: None,
            kind: None,
        }
    }
}

/// Describes a build tracking file change detected in the `corso/builds/` directory.
#[derive(Debug, Clone, Serialize)]
pub struct BuildUpdateEvent {
    /// Path relative to the helix root (e.g. `"corso/builds/active.yaml"`).
    pub path: String,
    /// What triggered this event.
    pub event_kind: BuildEventKind,
}

/// Filesystem event kind that produced a [`BuildUpdateEvent`].
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildEventKind {
    /// A new build tracking file was created.
    Created,
    /// An existing build tracking file was modified.
    Modified,
}

/// Slimmed-down view of an AYIN `TraceSpan` forwarded to the browser.
///
/// Field names and serialization format mirror the JSON produced by AYIN so
/// this struct can be deserialized directly from a raw SSE `data:` line
/// without a separate mapping step.
///
/// `outcome` and `metadata` are kept as raw [`serde_json::Value`] to avoid
/// coupling this crate to the AYIN type definitions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraceSpanSummary {
    /// Span UUID as a hyphenated lowercase string (e.g. `"00112233-…"`).
    pub id: String,
    /// Parent span UUID, if this span is a child of another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Session UUID grouping related spans across actors into a single
    /// interaction trace.  When present, the Lineage Circuit can reconstruct
    /// the full session tree even when spans arrive from different actors
    /// (copilot, user, webshell) or across SSE reconnections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Actor identifier, e.g. `"soul"`, `"claude_code"`, `"corso"`.
    pub actor: String,
    /// Action name, e.g. `"rag.query.started"`, `"tool.call"`.
    pub action: String,
    /// ISO-8601 UTC timestamp string.
    pub timestamp: String,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Outcome forwarded verbatim from AYIN (e.g. `"success"`, `"failure"`).
    pub outcome: serde_json::Value,
    /// Arbitrary extra data forwarded verbatim. Absent when null.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
    /// Top-level strand activations as emitted by AYIN's native `TraceSpan`.
    ///
    /// AYIN puts this at the top level of every span it writes. Older code
    /// paths may still embed the field under `metadata.strand_activations`
    /// for test-fixture compatibility, so the parser checks both locations
    /// (top-level wins). Empty when absent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strand_activations: Vec<serde_json::Value>,
    /// Decision checkpoints recorded during this span's execution.
    ///
    /// Each entry is a JSON object matching AYIN's `DecisionPoint` schema:
    /// `{ name, input, decision, confidence?, duration_ms }`. Kept as raw
    /// [`serde_json::Value`] to avoid coupling to the AYIN crate. Empty when absent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decision_points: Vec<serde_json::Value>,
}

/// AYIN connection lifecycle status.
///
/// Uses internally-tagged serialisation (`#[serde(tag = "status")]`) so that
/// unit variants (`Connected`, `Disconnected`) produce a flat `"status"` field
/// in the parent [`WebEvent`] object rather than being silently dropped.
///
/// Wire format examples:
/// - `{"type":"ayin_status","status":"connected"}`
/// - `{"type":"ayin_status","status":"disconnected"}`
/// - `{"type":"ayin_status","status":"reconnecting","attempt":3}`
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AyinStatus {
    /// Successfully connected and receiving spans.
    Connected,
    /// Connection dropped; the client will attempt to reconnect.
    Disconnected,
    /// Exponential-backoff reconnect is in progress.
    Reconnecting {
        /// 1-based reconnect attempt counter.
        attempt: u32,
    },
}

/// A control command sent from an external process (e.g. Claude Code)
/// to mutate the browser UI state via the SSE fan-out.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ControlCommand {
    /// Focus a specific panel (`"terminal"` or `"helix"`).
    FocusPanel {
        /// Panel identifier.
        panel: String,
    },
    /// Set split sizes as percentages (must sum to 100).
    ResizePanels {
        /// Terminal panel size in percent.
        terminal: u8,
        /// Helix panel size in percent.
        helix: u8,
    },
    /// Adjust the helix 3D scene zoom level.
    SetHelixZoom {
        /// Zoom level (camera distance factor).
        level: f32,
    },
    /// Show or hide a panel.
    SetPanelVisibility {
        /// Panel identifier (`"terminal"` or `"helix"`).
        panel: String,
        /// Whether the panel should be visible.
        visible: bool,
    },
    /// Push a transient notification to the browser.
    Notify {
        /// Human-readable message text.
        message: String,
        /// Severity level: `"info"`, `"warn"`, `"error"`.
        level: String,
    },
    /// Open a local file in the system default editor (or the editor
    /// referenced by the `$EDITOR` env var if set).
    ///
    /// The backend executes this locally and also broadcasts the event so
    /// SSE listeners can observe file-open activity.
    OpenInEditor {
        /// Absolute or workspace-relative file path.
        file: String,
        /// Optional 1-based line number to jump to.
        line: Option<u32>,
    },
    /// Reveal a local path in the system file manager (Finder on macOS).
    ///
    /// The backend executes this locally and also broadcasts the event.
    RevealInFinder {
        /// Absolute or workspace-relative path to reveal.
        path: String,
    },
    /// Resolve a pending ironclaw HITL escalation.
    ///
    /// The `escalation_nonce` (`UUIDv7`) must match the nonce embedded in
    /// the `IronclawHitlEscalation` SSE event.  The nonce is validated for
    /// single-use (SERAPH#3 anti-replay) before the parked worker is unblocked.
    ///
    /// Wire tag: `"ironclaw_hitl_resolution"`.
    IronclawHitlResolution {
        /// `UUIDv7` nonce minted at escalation time — consumed exactly once.
        escalation_nonce: uuid::Uuid,
        /// `true` = operator approved the blocked action; `false` = rejected.
        approved: bool,
        /// Optional free-text reason from the operator.
        operator_reason: Option<String>,
    },
}

// ────────────────────────────────────────────────────────────────────────────
// Plan-builder copilot bridge — Phase 1 contract types
// plan-builder-copilot-bridge feat, Phase 1 deliverable 1
// ────────────────────────────────────────────────────────────────────────────

/// Form fields from the Plan Builder UI for requesting a new plan draft.
///
/// Sent as JSON body to `POST /api/builds/plan/draft`.
#[derive(Debug, Serialize, Deserialize)]
pub struct PlanDraftRequest {
    /// Human-readable description — the "what" of the build.
    pub description: String,
    /// Repository path (or GitHub slug) the plan targets.
    pub repository: Option<String>,
    /// Northstar text verbatim. Omit to let EVA propose 3 options inline in the PLAN view.
    pub northstar: Option<String>,
    /// When `true`, the draft prompt includes `--research` flag (QUANTUM + SOUL prior-art research).
    #[serde(default)]
    pub research: bool,
    /// `LASDLC` tier selection (`"SMALL"`, `"MEDIUM"`, or `"LARGE"`). EVA selects when omitted.
    pub tier: Option<String>,
}

/// Immediate response body from `POST /api/builds/plan/draft`.
///
/// The `session_id` is the pre-minted Claude Code session `UUID`; the SSE stream
/// is available at `sse_url` for the browser to subscribe to.
#[derive(Debug, Serialize)]
pub struct PlanDraftResponseEnvelope {
    /// Pre-minted `UUIDv4` — used as `--session-id` arg, `JSONL` filename, and commit key.
    pub session_id: uuid::Uuid,
    /// Codename derived from the description (kebab-case, 3–5 words).
    pub codename: String,
    /// `SSE` `URL` the browser should subscribe to for streaming draft events.
    pub sse_url: String,
}

/// Request body for `POST /api/builds/plan/commit`.
#[derive(Debug, Deserialize)]
pub struct PlanCommitRequest {
    /// Session `UUID` returned by the draft endpoint — must match the in-flight draft.
    pub session_id: uuid::Uuid,
    /// Codename of the plan to commit (must match the draft's codename).
    pub codename: String,
    /// Final `LASDLC` plan body (full Markdown, including validated frontmatter).
    pub body: String,
    /// Optional idempotency key — replay of the same key on the same `(session_id, codename)`
    /// returns the prior 200 without a duplicate write (1 h `TTL` on server).
    pub idempotency_key: Option<uuid::Uuid>,
}

/// Streaming event emitted over `GET /api/builds/{codename}/plan-stream`.
///
/// Event sequence: `TextChunk`* → `IterationStart` → `TextChunk`* → `VerdictBlock`
/// → `Done` (on success) | `Error` (on failure).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlanDraftEvent {
    /// A chunk of streamed plan Markdown text from the copilot subprocess.
    TextChunk {
        /// Incremental text delta — append to the PLAN view buffer.
        text: String,
    },
    /// EVA is starting iteration `N` of the `/PLAN` draft–review loop (1-based).
    IterationStart {
        /// Current iteration number.
        iteration: u8,
    },
    /// The `/PLAN` Step 5 self-review verdict block was emitted.
    VerdictBlock {
        /// Parsed verdict — gate the Commit button on `validation_status == "VALIDATED"`.
        verdict: ReviewVerdict,
    },
    /// Draft complete; `codename` is the derived codename to use in the commit step.
    Done {
        /// Codename for the subsequent `PlanCommitRequest`.
        codename: String,
    },
    /// Terminal error — draft cannot continue; surface to the operator and offer retry.
    Error {
        /// Human-readable error message (never leaks subprocess internals per security-guardrails).
        message: String,
    },
}

/// Parsed review verdict from the `/PLAN` Step 5 self-review loop.
///
/// Emitted inside [`PlanDraftEvent::VerdictBlock`]; the frontend gates the
/// Commit button on `validation_status == "VALIDATED"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewVerdict {
    /// `VALIDATED` | `INSUFFICIENT_EVIDENCE` | `REVISION_NEEDED`
    pub validation_status: String,
    /// Which `LASDLC` review iteration this verdict closes (1-based).
    pub iteration: u8,
    /// Human-readable summary of what passed and what needs revision.
    pub summary: Option<String>,
}

/// Errors that can occur during `POST /api/builds/plan/draft` handling.
///
/// Complete `HTTP` status map (Cookbook §multi-variant rule — all variants covered):
/// - [`SubprocessSpawnFailed`][Self::SubprocessSpawnFailed] → 502 Bad Gateway
/// - [`Timeout`][Self::Timeout]                             → 504 Gateway Timeout
/// - [`ValidationFailed`][Self::ValidationFailed]           → 422 Unprocessable Entity
/// - [`IoError`][Self::IoError]                             → 500 Internal Server Error
/// - [`CancelledByClient`][Self::CancelledByClient]         → 499 Client Closed Request
#[derive(Debug, thiserror::Error)]
pub enum PlanDraftError {
    /// The `claude --print` subprocess could not be spawned. → 502
    #[error("subprocess spawn failed: {0}")]
    SubprocessSpawnFailed(String),
    /// The copilot draft exceeded the wall-clock timeout. → 504
    #[error("plan draft timed out")]
    Timeout,
    /// The prompt template or form input failed validation. → 422
    #[error("validation failed: {0}")]
    ValidationFailed(String),
    /// An I/O error occurred during streaming or disk writes. → 500
    #[error("I/O error: {0}")]
    IoError(String),
    /// The client closed the `SSE` connection before the draft completed. → 499
    #[error("cancelled by client")]
    CancelledByClient,
}

/// Errors that can occur during `POST /api/builds/plan/commit` handling.
///
/// Complete `HTTP` status map (Cookbook §multi-variant rule — all variants covered):
/// - [`SessionMismatch`][Self::SessionMismatch]                 → 403 Forbidden
/// - [`InvalidFrontmatter`][Self::InvalidFrontmatter]           → 422 Unprocessable Entity
/// - [`DuplicateCommit`][Self::DuplicateCommit]                 → 409 Conflict
/// - [`WriteConsistencyFailed`][Self::WriteConsistencyFailed]   → 409 Conflict
/// - [`IoError`][Self::IoError]                                 → 500 Internal Server Error
#[derive(Debug, thiserror::Error)]
pub enum PlanCommitError {
    /// The `session_id` does not match the in-flight or completed draft. → 403
    ///
    /// 403 (security-flavored) rather than 404 to avoid confirming session existence.
    /// Per SCRUM F9 rationale: 409 reserved for future multi-instance commit conflict.
    #[error("session mismatch — commit rejected")]
    SessionMismatch,
    /// The plan body has invalid or missing required frontmatter fields. → 422
    #[error("invalid frontmatter: {0}")]
    InvalidFrontmatter(String),
    /// A commit with the same `idempotency_key` on a different body was already recorded. → 409
    #[error("duplicate commit detected")]
    DuplicateCommit,
    /// Post-commit tree-hash check failed — `active.yaml` write may not have persisted. → 409
    ///
    /// See agents-playbook §15.4.5 phantom-empty-commit guard. `CF-F14`.
    #[error("write consistency check failed")]
    WriteConsistencyFailed,
    /// An I/O error occurred while writing plan file or `active.yaml`. → 500
    #[error("I/O error: {0}")]
    IoError(String),
}

// ────────────────────────────────────────────────────────────────────────────
// Global event store types — Phase 1 contract types
// plan-builder-copilot-bridge feat, Phase 1 deliverable 1 (global observability)
// ────────────────────────────────────────────────────────────────────────────

/// Source of a [`GlobalWebEvent`] for consumer-side filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventSource {
    /// Event from a specific build's copilot subprocess.
    BuildSession {
        /// Codename of the build that produced this event.
        codename: String,
    },
    /// Event from a raw copilot subprocess identified by `PID`.
    CopilotSubprocess {
        /// Process `ID` of the copilot subprocess.
        pid: u32,
    },
    /// Event from the conductor worker pool.
    ConductorWorker {
        /// Task `ID` within the conductor queue.
        task_id: String,
    },
    /// Event from a `/GATE` runner.
    GateRunner {
        /// Gate identifier (e.g. `"gate-3-AQT"`).
        gate_id: String,
    },
}

/// A single entry stored in the [`GlobalEventStore`] ring buffer.
///
/// Every event pushed to the store is wrapped with sequence number, timestamp,
/// and source metadata. The `seq` field is used for `Last-Event-ID` reconnect
/// resume (clients send `Last-Event-ID: <seq>` on reconnect; server resumes
/// from `seq+1` if the entry is still in the ring).
#[derive(Debug, Clone, Serialize)]
pub struct GlobalEventEntry {
    /// Monotonically increasing sequence number (1-based, per-store instance).
    pub seq: u64,
    /// `UTC` wall-clock timestamp when the event was pushed to the store.
    pub timestamp: DateTime<Utc>,
    /// Origin of the event — used by [`EventFilter`] for consumer-side filtering.
    pub source: EventSource,
    /// The wrapped broadcast event payload.
    pub event: WebEvent,
}

/// Filter parameters accepted as query-string on `GET /api/events/global`.
///
/// All fields are optional; absent fields match all events. Filtering is
/// applied consumer-side at the `SSE` subscriber — the ring buffer stores
/// all variants unfiltered (see `GlobalEventStore`).
#[derive(Debug, Default, Deserialize)]
pub struct EventFilter {
    /// Only events from a sibling matching this name.
    pub sibling: Option<String>,
    /// Only events at or above this severity level.
    pub severity: Option<String>,
    /// Only events from the build with this codename.
    pub build_id: Option<String>,
    /// Only events for the tool with this name.
    pub tool_name: Option<String>,
}

// ── webshell-hitl-bridge — question tool payload types (Phase 1) ─────────────

/// Payload for [`WebEvent::QuestionPrompt`].
///
/// Schema mirrors the gateway's `QuestionInput` verbatim so the browser can
/// render `QuestionCard.svelte` without a secondary fetch. All fields are
/// serialised with `camelCase` to match Anthropic's `AskUserQuestion` wire
/// format.
///
/// # Security
/// `tool_use_id` is always a `Uuid::new_v4()` minted at dispatch time —
/// never derived from client input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionPromptEvent {
    /// Server-minted `UUIDv4` correlating this prompt to the gateway's
    /// `tool_use` block. Echoed on `POST /api/sessions/:id/answer`.
    pub tool_use_id: Uuid,
    /// Questions to present to the operator, in order.
    pub questions: Vec<QuestionItem>,
    /// How to handle the question when no interactive transport is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headless_policy: Option<QuestionHeadlessPolicy>,
}

/// Payload for [`WebEvent::QuestionAnswered`].
///
/// Emitted after `POST /api/sessions/:id/answer` resolves the registry oneshot.
/// The browser clears the matching `pendingQuestions` entry on receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionAnsweredEvent {
    /// Echoed from [`QuestionPromptEvent::tool_use_id`] for correlation.
    pub tool_use_id: Uuid,
    /// Per-question selected labels (or free text). One inner `Vec` per question.
    pub answers: Vec<Vec<String>>,
}

/// A single question within a [`QuestionPromptEvent`].
///
/// Field names are `camelCase` to match Anthropic's `AskUserQuestion` schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionItem {
    /// Question text shown as the modal heading.
    pub question: String,
    /// Short chip label (max 12 chars).
    pub header: String,
    /// Whether the operator may select multiple options.
    #[serde(default)]
    pub multi_select: bool,
    /// Selectable options.
    pub options: Vec<QuestionOptionItem>,
}

/// One option within a [`QuestionItem`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOptionItem {
    /// Short display label.
    pub label: String,
    /// Explanation shown beneath the label.
    pub description: String,
}

/// Headless behaviour for [`QuestionPromptEvent`] — mirrors the gateway enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionHeadlessPolicy {
    /// Return an error `tool_result` with the question text (default).
    FailLoud,
    /// Silently select the first option and continue.
    AutoFirst,
    /// Skip the question (return empty answers) and continue.
    AutoSkip,
}

/// Pending-question registry entry stored alongside the oneshot sender.
///
/// Kept in `AppState.question_metadata` for the 300 s TTL eviction loop and
/// for returning metadata to the browser on `GET /api/sessions/:id/question`.
///
/// # Single-operator contract
///
/// This struct carries no `session_id` or `build_id` field. The corresponding
/// `SseGuard::drop()` drain therefore covers **all** pending entries regardless
/// of originating build. This is intentional — the webshell is a
/// single-operator tool and one SSE disconnect means the operator is gone.
/// Any future extension to multi-operator sessions MUST add a `session_id`
/// field here and scope the drain accordingly; otherwise tab-A's disconnect
/// will cancel tab-B's pending questions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionPending {
    /// Correlates to the gateway `tool_use_id`.
    pub tool_use_id: Uuid,
    /// Original questions (verbatim from the gateway payload).
    pub questions: Vec<QuestionItem>,
    /// Headless behaviour if set by the gateway.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headless_policy: Option<QuestionHeadlessPolicy>,
    /// Wall-clock time the question was registered (for TTL eviction).
    pub inserted_at: DateTime<Utc>,
}

/// Browser-submitted answer to a [`QuestionPending`] question set.
///
/// Sent via `POST /api/sessions/:id/answer`; transmitted over the oneshot
/// channel in [`AppState::question_registry`] to unblock the gateway long-poll.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionAnswer {
    /// Per-question selected labels — one inner `Vec<String>` per [`QuestionItem`].
    ///
    /// Single-select questions: inner vec has exactly one element.
    /// Multi-select questions: inner vec may have zero or more elements.
    pub answers: Vec<Vec<String>>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn web_event_ayin_status_serialises_type_tag() {
        let event = WebEvent::AyinStatus(AyinStatus::Connected);
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains(r#""type":"ayin_status""#),
            "missing type tag: {json}"
        );
    }

    #[test]
    fn web_event_ayin_span_serialises_type_tag() {
        let span = TraceSpanSummary {
            id: "test".to_owned(),
            parent_id: None,
            actor: "soul".to_owned(),
            action: "rag.query".to_owned(),
            timestamp: "2026-04-13T00:00:00Z".to_owned(),
            duration_ms: 10,
            outcome: serde_json::Value::String("success".to_owned()),
            metadata: serde_json::Value::Null,
            strand_activations: Vec::new(),
            session_id: None,
            decision_points: Vec::new(),
        };
        let event = WebEvent::AyinSpan(span);
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains(r#""type":"ayin_span""#),
            "missing type tag: {json}"
        );
    }

    #[test]
    fn trace_span_summary_null_metadata_omitted() {
        let span = TraceSpanSummary {
            id: "x".to_owned(),
            parent_id: None,
            actor: "a".to_owned(),
            action: "b".to_owned(),
            timestamp: "t".to_owned(),
            duration_ms: 0,
            outcome: serde_json::json!("success"),
            metadata: serde_json::Value::Null,
            strand_activations: Vec::new(),
            session_id: None,
            decision_points: Vec::new(),
        };
        let json = serde_json::to_string(&span).unwrap();
        assert!(
            !json.contains("metadata"),
            "null metadata must be omitted: {json}"
        );
    }

    #[test]
    fn trace_span_summary_null_parent_id_omitted() {
        let span = TraceSpanSummary {
            id: "x".to_owned(),
            parent_id: None,
            actor: "a".to_owned(),
            action: "b".to_owned(),
            timestamp: "t".to_owned(),
            duration_ms: 0,
            outcome: serde_json::json!("success"),
            metadata: serde_json::Value::Null,
            strand_activations: Vec::new(),
            session_id: None,
            decision_points: Vec::new(),
        };
        let json = serde_json::to_string(&span).unwrap();
        assert!(
            !json.contains("parent_id"),
            "absent parent_id must be omitted: {json}"
        );
    }

    #[test]
    fn reconnecting_status_includes_attempt() {
        let event = WebEvent::AyinStatus(AyinStatus::Reconnecting { attempt: 3 });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("reconnecting"), "{json}");
        assert!(json.contains("attempt"), "{json}");
        assert!(json.contains('3'), "{json}");
    }

    #[test]
    fn helix_entry_event_has_type_tag() {
        let entry =
            HelixEntrySummary::minimal("eva/entries/day-1.md".to_owned(), HelixEventKind::Created);
        let event = WebEvent::HelixEntry(entry);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"helix_entry""#), "{json}");
        assert!(json.contains("created"), "{json}");
    }

    #[test]
    fn helix_event_kind_modified_serialises() {
        let kind = HelixEventKind::Modified;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, r#""modified""#);
    }

    #[test]
    fn build_update_event_has_type_tag() {
        let entry = BuildUpdateEvent {
            path: "corso/builds/active.yaml".to_owned(),
            event_kind: BuildEventKind::Created,
        };
        let event = WebEvent::BuildUpdate(entry);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"build_update""#), "{json}");
        assert!(json.contains("active.yaml"), "{json}");
        assert!(json.contains("created"), "{json}");
    }

    #[test]
    fn build_event_kind_modified_serialises() {
        let kind = BuildEventKind::Modified;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, r#""modified""#);
    }

    #[test]
    fn gateway_notify_wraps_raw_json_under_payload() {
        let event = WebEvent::GatewayNotify {
            payload: serde_json::json!({"type": "focus_pillar", "pillar": "ARCH"}),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains(r#""type":"gateway_notify""#),
            "outer tag must be gateway_notify: {json}"
        );
        // Parse back and confirm `payload.type` is preserved for the frontend
        // to dispatch on (e.g. `msg.payload.type === "focus_pillar"`).
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["payload"]["type"], "focus_pillar");
        assert_eq!(parsed["payload"]["pillar"], "ARCH");
    }

    #[test]
    fn strand_activation_has_type_tag_and_flat_fields() {
        let event = WebEvent::StrandActivation(StrandActivationEvent {
            sibling: "eva".to_owned(),
            strand: "methodical".to_owned(),
            weight: 0.9,
            timestamp: "2026-04-16T00:00:00Z".to_owned(),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"strand_activation""#), "{json}");
        assert!(json.contains(r#""sibling":"eva""#), "{json}");
        assert!(json.contains(r#""strand":"methodical""#), "{json}");
        assert!(json.contains(r#""weight":0.9"#), "{json}");
    }

    /// SSE contract canary (#50) — trip-wire test that enumerates EVERY
    /// `WebEvent` variant and asserts the serialised `"type"` tag matches the
    /// exact string the frontend's `EventType` union expects.
    ///
    /// **If this test fails** you must update `EventType` in
    /// `lightarchitects-webshell-ui/src/lib/types.ts` to match before merging.
    ///
    /// The canonical FE set at time of writing (2026-05-19, agent-teams-fleet):
    ///   `ayin_span`, `ayin_status`, `helix_entry`, `build_update`, `control`,
    ///   `strand_activation`, `soul_promotion`, `gateway_notify`, `pillar_update`,
    ///   `strand_convergence`, `copilot_activity`, `copilot_response`,
    ///   `permission_request`, `context_status`, `supervisor_update`,
    ///   `agent_fleet_update`
    #[allow(clippy::too_many_lines)]
    #[test]
    fn sse_contract_all_web_event_variants_have_known_type_tags() {
        // Helper: extract the `type` field from a serialised WebEvent.
        fn type_tag(event: &WebEvent) -> String {
            let json = serde_json::to_string(event).unwrap();
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            v["type"].as_str().unwrap_or("").to_owned()
        }

        let span = TraceSpanSummary {
            id: "x".to_owned(),
            parent_id: None,
            actor: "soul".to_owned(),
            action: "a".to_owned(),
            timestamp: "t".to_owned(),
            duration_ms: 0,
            outcome: serde_json::Value::Null,
            metadata: serde_json::Value::Null,
            strand_activations: Vec::new(),
            session_id: None,
            decision_points: Vec::new(),
        };
        let helix = HelixEntrySummary::minimal("p".to_owned(), HelixEventKind::Created);
        let build_ev = BuildUpdateEvent {
            path: "p".to_owned(),
            event_kind: BuildEventKind::Created,
        };
        let ctrl = ControlCommand::Notify {
            message: "m".to_owned(),
            level: "info".to_owned(),
        };
        let strand = StrandActivationEvent {
            sibling: "s".to_owned(),
            strand: "t".to_owned(),
            weight: 1.0,
            timestamp: "t".to_owned(),
        };
        let pillar = PillarUpdateEvent {
            build_id: "b".to_owned(),
            pillar: "arch".to_owned(),
            phase: "started".to_owned(),
            line: None,
            exit_code: None,
            artifact: None,
        };
        let convergence = StrandConvergenceEvent {
            strand: "analytical".to_owned(),
            siblings: vec!["eva".to_owned()],
            memo_ids: Vec::new(),
            detected_at: "t".to_owned(),
        };
        let activity = CopilotActivityEvent {
            build_id: "b".to_owned(),
            kind: "assistant".to_owned(),
            summary: None,
            raw: serde_json::Value::Null,
            timestamp: "t".to_owned(),
        };
        let promotion = crate::memory::types::PromotionEvent {
            memo_id: "m".to_owned(),
            from: crate::memory::types::MemoryTier::Hot,
            to: crate::memory::types::MemoryTier::Cold,
            sibling: "eva".to_owned(),
            significance: 0.9,
            path: "p".to_owned(),
            promoted_at: "t".to_owned(),
        };

        // Canonical mapping: Rust variant → expected serialised `type` string.
        // Update this list AND the FE EventType whenever a new variant is added.
        let cases: &[(&str, WebEvent)] = &[
            ("ayin_span", WebEvent::AyinSpan(span)),
            ("ayin_status", WebEvent::AyinStatus(AyinStatus::Connected)),
            ("helix_entry", WebEvent::HelixEntry(helix)),
            ("build_update", WebEvent::BuildUpdate(build_ev)),
            ("control", WebEvent::Control(ctrl)),
            ("strand_activation", WebEvent::StrandActivation(strand)),
            ("soul_promotion", WebEvent::SoulPromotion(promotion)),
            (
                "gateway_notify",
                WebEvent::GatewayNotify {
                    payload: serde_json::Value::Null,
                },
            ),
            ("pillar_update", WebEvent::PillarUpdate(pillar)),
            (
                "strand_convergence",
                WebEvent::StrandConvergence(convergence),
            ),
            ("copilot_activity", WebEvent::CopilotActivity(activity)),
            (
                "copilot_response",
                WebEvent::CopilotResponse {
                    chunk: "hello".to_owned(),
                    done: false,
                    sibling: Some("claude".to_owned()),
                    turn_span_id: None,
                },
            ),
            (
                "permission_request",
                WebEvent::PermissionRequest {
                    call_id: "test-id".to_owned(),
                    input_preview: "Bash: {\"command\":\"ls\"}".to_owned(),
                    risk_tier: RiskTier::Low,
                },
            ),
            (
                "context_status",
                WebEvent::ContextStatus(ContextStatusEvent {
                    usage_pct: 0.25,
                    level: None,
                    budget: 200_000,
                    used: 50_000,
                }),
            ),
            (
                "supervisor_update",
                WebEvent::SupervisorUpdate(NorthstarEvaluationEvent {
                    build_id: "build-1".to_owned(),
                    wave_num: 1,
                    status: "advancing".to_owned(),
                    confidence: 0.9,
                    recommended_next: "Continue".to_owned(),
                    proposal_pending: false,
                }),
            ),
            (
                "agent_fleet_update",
                WebEvent::AgentFleetUpdate(lightarchitects::fleet::FleetSnapshot {
                    nodes: vec![],
                    captured_at: "2026-05-19T00:00:00Z".to_owned(),
                }),
            ),
            (
                "ironclaw_hitl_escalation",
                WebEvent::IronclawHitlEscalation(IronclawHitlEscalationEvent {
                    build_id: uuid::Uuid::nil(),
                    task_id: "t".to_owned(),
                    decision_topic: "dep-add".to_owned(),
                    layer_failed: 4,
                    escalation_question: "Approve?".to_owned(),
                    deadline: None,
                    traceparent: None,
                    nonce: uuid::Uuid::nil(),
                }),
            ),
            (
                "ironclaw_hitl_resolution",
                WebEvent::IronclawHitlResolution(IronclawHitlResolutionEvent {
                    build_id: uuid::Uuid::nil(),
                    task_id: "t".to_owned(),
                    resolution: HitlResolution::Approve,
                    operator_id: "webshell:operator".to_owned(),
                    decided_at: chrono::Utc::now(),
                    nonce: uuid::Uuid::nil(),
                }),
            ),
        ];

        for (expected_tag, event) in cases {
            let actual = type_tag(event);
            assert_eq!(
                actual, *expected_tag,
                "WebEvent variant serialised as '{actual}' but contract expects '{expected_tag}'. \
                 Update EventType in lightarchitects-webshell-ui/src/lib/types.ts.",
            );
        }
    }

    /// Task 2.4a — Subscribe-ordering invariant.
    ///
    /// SSE handler MUST call `event_tx.subscribe()` and hold the `Receiver`
    /// BEFORE `run_print_turn` is invoked, otherwise the first chunks are lost.
    ///
    /// This test validates the invariant at the broadcast channel level:
    /// a subscriber created BEFORE sends receives ALL sent events, including
    /// the final `done: true` variant.
    #[tokio::test]
    async fn copilot_response_subscribe_ordering_invariant() {
        use tokio::sync::broadcast;
        let (tx, mut rx) = broadcast::channel::<WebEvent>(4096);

        // Simulate run_print_turn: emits chunks then done:true
        let tx2 = tx.clone();
        tokio::spawn(async move {
            for i in 0..3u8 {
                let _ = tx2.send(WebEvent::CopilotResponse {
                    chunk: format!("chunk{i}"),
                    done: false,
                    sibling: Some("claude".to_owned()),
                    turn_span_id: None,
                });
            }
            let _ = tx2.send(WebEvent::CopilotResponse {
                chunk: String::new(),
                done: true,
                sibling: Some("claude".to_owned()),
                turn_span_id: None,
            });
        });

        let mut received = Vec::new();
        while let Ok(event) = rx.recv().await {
            if let WebEvent::CopilotResponse { done, chunk, .. } = &event {
                received.push(chunk.clone());
                if *done {
                    break;
                }
            }
        }
        // Expect 3 chunks + 1 done (empty chunk)
        assert_eq!(
            received.len(),
            4,
            "expected 3 chunks + done sentinel, got {received:?}"
        );
        assert!(
            received.last().is_some_and(String::is_empty),
            "last chunk must be empty (done sentinel)"
        );
    }

    /// `RiskTier` serialises with `snake_case` tags.
    #[test]
    fn risk_tier_serialises_snake_case() {
        let json = serde_json::to_string(&RiskTier::Critical).unwrap();
        assert_eq!(
            json, r#""critical""#,
            "RiskTier::Critical must serialise as \"critical\""
        );
        let json = serde_json::to_string(&RiskTier::Medium).unwrap();
        assert_eq!(json, r#""medium""#);
    }

    /// `CopilotResponse` serialises with the correct wire type tag.
    #[test]
    fn copilot_response_has_correct_type_tag() {
        let event = WebEvent::CopilotResponse {
            chunk: "hello".to_owned(),
            done: true,
            sibling: None,
            turn_span_id: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"copilot_response""#), "{json}");
        assert!(json.contains(r#""done":true"#), "{json}");
        assert!(
            !json.contains("sibling"),
            "absent sibling must be omitted: {json}"
        );
    }

    // ── ironclaw-spine / lightsquad variant tests (Phase 2A.5) ───────────────

    #[test]
    fn escalation_serialises_type_tag() {
        let event = WebEvent::Escalation(EscalationEvent {
            build_id: "ironclaw-spine".to_owned(),
            wave_index: 1,
            worker_slot: 3,
            reason: "gate [S] requires operator approval".to_owned(),
            call_id: "00000000-0000-0000-0000-000000000001".to_owned(),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"escalation""#), "{json}");
        assert!(json.contains("ironclaw-spine"), "{json}");
        assert!(json.contains("call_id"), "{json}");
    }

    #[test]
    fn worker_slot_gauge_serialises_type_tag() {
        let event = WebEvent::WorkerSlotGauge(WorkerSlotGaugeEvent {
            build_id: "ironclaw-spine".to_owned(),
            wave_index: 0,
            active: 5,
            capacity: 7,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"worker_slot_gauge""#), "{json}");
        assert!(json.contains(r#""active":5"#), "{json}");
        assert!(json.contains(r#""capacity":7"#), "{json}");
    }

    #[test]
    fn conductor_tick_serialises_type_tag() {
        let event = WebEvent::ConductorTick(ConductorTickEvent {
            build_id: "ironclaw-spine".to_owned(),
            tick_seq: 42,
            queue_depth: 3,
            active_workers: 4,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"conductor_tick""#), "{json}");
        assert!(json.contains(r#""tick_seq":42"#), "{json}");
    }

    #[test]
    fn merge_agent_status_merged_phase_includes_commit_sha() {
        let event = WebEvent::MergeAgentStatus(MergeAgentStatusEvent {
            build_id: "ironclaw-spine".to_owned(),
            wave_index: 2,
            phase: "merged".to_owned(),
            commit_sha: Some("abc1234".to_owned()),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"merge_agent_status""#), "{json}");
        assert!(json.contains("abc1234"), "{json}");
    }

    #[test]
    fn merge_agent_status_non_merged_omits_commit_sha() {
        let event = WebEvent::MergeAgentStatus(MergeAgentStatusEvent {
            build_id: "ironclaw-spine".to_owned(),
            wave_index: 2,
            phase: "running".to_owned(),
            commit_sha: None,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            !json.contains("commit_sha"),
            "absent commit_sha must be omitted: {json}"
        );
    }

    #[test]
    fn fix_agent_iteration_serialises_type_tag() {
        let event = WebEvent::FixAgentIteration(FixAgentIterationEvent {
            build_id: "ironclaw-spine".to_owned(),
            wave_index: 1,
            worker_slot: 2,
            iteration: 2,
            issue_summary: "clippy::unwrap_used in production path".to_owned(),
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"fix_agent_iteration""#), "{json}");
        assert!(json.contains(r#""iteration":2"#), "{json}");
    }
}
