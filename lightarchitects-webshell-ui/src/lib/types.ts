// ============================================================================
// TypeScript interfaces for the LA Platform GUI
// ============================================================================

/** 7 CORSO pillars — same for every meta-skill */
export type Pillar = 'ARCH' | 'SEC' | 'QUAL' | 'PERF' | 'TEST' | 'DOC' | 'OPS';

export const PILLARS: Pillar[] = ['ARCH', 'SEC', 'QUAL', 'PERF', 'TEST', 'DOC', 'OPS'];

export type PillarStatus = 'pending' | 'in_progress' | 'passed' | 'failed' | 'blocked';

export interface PillarGate {
  pillar: Pillar;
  status: PillarStatus;
  confidence: number; // 0–1
  entryGate?: GateResult;
  exitGate?: GateResult;
  findings: string[];
}

export interface GateResult {
  passed: boolean;
  timestamp: string;
  metrics: Record<string, number>;
  hitl?: boolean; // requires human-in-the-loop approval
}

// --- Hierarchy ---

export interface Workspace {
  id: string;
  name: string;
  path: string;
  builds: Build[];
}

export interface Build {
  id: string;
  workspaceId: string;
  name: string;
  metaSkill: MetaSkill;
  status: BuildStatus;
  pillars: PillarGate[];
  currentPillar: Pillar;
  confidence: number;
  createdAt: string;
  updatedAt: string;
  modules: Module[];
  siblingDispatches: SiblingDispatch[];
  // Extended fields from active.yaml portfolio entries
  description?: string;
  priority?: Priority;
  siblings?: string[];
  blockedBy?: string[];
  blocks?: string[];
  path?: string;
  tier?: number;
  codename?: string;  // adjective-gerund-noun build identifier (from portfolio YAML)
  branch?: string;    // feature branch (e.g. feat/luminous-tracing-polytope)
  /** Agent descriptor from the build session — `kind` indicates which agent runs this build. */
  agent?: { kind: string; backend?: string };
}

export type BuildStatus = 'queued' | 'in_progress' | 'completed' | 'failed' | 'paused' | 'rejected' | 'rolled_back';

/** Project group — aggregates builds by project path */
export interface ProjectGroup {
  id: string;
  name: string;
  path: string;
  project?: Build;
  plans: Build[];
  planCount: number;
  activePlanCount: number;
  progress: number;
}

export interface Module {
  id: string;
  buildId: string;
  name: string;
  path: string;
  language?: string;
}

// --- Meta-skills ---

export type MetaSkill =
  | '/BUILD'
  | '/RESEARCH'
  | '/SECURE'
  | '/SQUAD'
  | '/PLAN'
  | '/DEPLOY'
  | '/REVIEW'
  | '/OBSERVE'
  | '/ONBOARD'
  | '/OPTIMIZE'
  | '/REFLECT'
  | '/ENRICH';

export const META_SKILLS: MetaSkill[] = [
  '/BUILD', '/RESEARCH', '/SECURE', '/SQUAD',
  '/PLAN', '/DEPLOY', '/REVIEW', '/OBSERVE',
  '/ONBOARD', '/OPTIMIZE', '/REFLECT', '/ENRICH',
];

// --- LASDLC Framework (v1.0) ────────────────────────────────────────────────
// Three orthogonal axes: Execution Phases × Quality Gates × Agent Topology
// Spec: helix/user/standards/canon/lasdlc-spec.md
// Template: helix/corso/builds/LASDLC-TEMPLATE-v1.yaml

/** LASDLC execution phases — sequential work order */
export type ExecutionPhase = 'Plan' | 'Research' | 'Implement' | 'Harden' | 'Verify' | 'Ship' | 'Learn';

/** All 7 LASDLC execution phases in order */
export const EXECUTION_PHASES: ExecutionPhase[] = ['Plan', 'Research', 'Implement', 'Harden', 'Verify', 'Ship', 'Learn'];

/** Build complexity tier — determines which phases are active */
export type BuildTier = 'SMALL' | 'MEDIUM' | 'LARGE';

/** Phase sets per tier (tier telescoping) */
export const TIER_PHASES: Record<BuildTier, ExecutionPhase[]> = {
  SMALL:  ['Plan', 'Implement', 'Verify', 'Ship'],
  MEDIUM: ['Plan', 'Research', 'Implement', 'Verify', 'Ship', 'Learn'],
  LARGE:  ['Plan', 'Research', 'Implement', 'Harden', 'Verify', 'Ship', 'Learn'],
};

/** 7 quality dimensions — checked IN PARALLEL at every phase boundary */
export type QualityDimension = 'Architecture' | 'Security' | 'Quality' | 'Performance' | 'Testing' | 'Documentation' | 'Operations';

/** Quality dimension abbreviations for compact display */
export const QUALITY_DIMENSION_ABBREV: Record<QualityDimension, string> = {
  Architecture: 'A', Security: 'S', Quality: 'Q', Performance: 'P',
  Testing: 'T', Documentation: 'D', Operations: 'O',
};

/** All 7 quality dimensions */
export const QUALITY_DIMENSIONS: QualityDimension[] = [
  'Architecture', 'Security', 'Quality', 'Performance', 'Testing', 'Documentation', 'Operations',
];

/** Quality dimension result at a gate boundary */
export interface QualityDimensionResult {
  dimension: QualityDimension;
  status: 'passed' | 'pending' | 'failed' | 'waived';
  criteria_passed: number;
  criteria_total: number;
  evaluated_by?: string;
}

/** Agent assignment within a phase (file-ownership partitioning) */
export interface AgentAssignment {
  id: string;
  sibling: SiblingId;
  owns: string[];                    // files with exclusive write access
  functions: string[];               // file::function targets
  tools: string[];                   // allowed tool names
  budget: number;                    // max token consumption
  depends_on: string[];              // agent IDs that must complete first
  status: 'queued' | 'running' | 'complete' | 'failed';
}

/** File-function map for planning granularity */
export interface FileFunctionMap {
  [filePath: string]: {
    create?: string[];               // new functions to add
    modify?: string[];               // existing functions to change
    delete?: string[];               // functions to remove
  };
}

// --- Quality Gate Labels (per-skill action names for CORSO gate evaluation) ---
// NOTE: These are quality dimension labels, NOT execution phases.
// CORSO uses these action names when evaluating gates at each phase boundary.

/** Per-skill quality gate action labels (what CORSO calls each dimension check) */
export const QUALITY_GATE_LABELS: Record<MetaSkill, Record<Pillar, string>> = {
  '/BUILD':        { ARCH: 'SCOUT',  SEC: 'FETCH',  QUAL: 'SNIFF',  PERF: 'GUARD',  TEST: 'CHASE',  DOC: 'HUNT',   OPS: 'SCRUM' },
  '/RESEARCH':     { ARCH: 'SCAN',   SEC: 'SWEEP',  QUAL: 'TRACE',  PERF: 'PROBE',  TEST: 'THEORIZE', DOC: 'VERIFY', OPS: 'CLOSE' },
  '/SECURE':       { ARCH: 'RECON',  SEC: 'SURVEY', QUAL: 'EXAMINE',PERF: 'STRIKE', TEST: 'REPORT', DOC: 'REMEDIATE', OPS: 'CLOSE' },
  '/SQUAD':        { ARCH: 'TEAM',   SEC: 'AUTH',   QUAL: 'CHECK',  PERF: 'REVIEW',  TEST: 'TEST',   DOC: 'DOC',    OPS: 'SCRUM' },
  '/PLAN':         { ARCH: 'SCOUT',  SEC: 'FETCH',  QUAL: 'SNIFF',  PERF: 'GUARD',  TEST: 'CHASE',  DOC: 'HUNT',   OPS: 'SCRUM' },
  '/DEPLOY':       { ARCH: 'SCOUT',  SEC: 'FETCH',  QUAL: 'SNIFF',  PERF: 'GUARD',  TEST: 'CHASE',  DOC: 'HUNT',   OPS: 'SCRUM' },
  '/REVIEW':       { ARCH: 'SCAN',   SEC: 'SWEEP',  QUAL: 'TRACE',  PERF: 'PROBE',  TEST: 'THEORIZE', DOC: 'VERIFY', OPS: 'CLOSE' },
  '/OBSERVE':      { ARCH: 'SCAN',   SEC: 'SWEEP',  QUAL: 'TRACE',  PERF: 'PROBE',  TEST: 'THEORIZE', DOC: 'VERIFY', OPS: 'CLOSE' },
  '/ONBOARD':      { ARCH: 'SCOUT',  SEC: 'FETCH',  QUAL: 'SNIFF',  PERF: 'GUARD',  TEST: 'CHASE',  DOC: 'HUNT',   OPS: 'SCRUM' },
  '/OPTIMIZE':     { ARCH: 'SCOUT',  SEC: 'FETCH',  QUAL: 'SNIFF',  PERF: 'GUARD',  TEST: 'CHASE',  DOC: 'HUNT',   OPS: 'SCRUM' },
  '/REFLECT':      { ARCH: 'SCAN',   SEC: 'SWEEP',  QUAL: 'TRACE',  PERF: 'PROBE',  TEST: 'THEORIZE', DOC: 'VERIFY', OPS: 'CLOSE' },
  '/ENRICH':       { ARCH: 'SCAN',   SEC: 'SWEEP',  QUAL: 'TRACE',  PERF: 'PROBE',  TEST: 'THEORIZE', DOC: 'VERIFY', OPS: 'CLOSE' },
};

// Backward-compat alias — consumers are being migrated to QUALITY_GATE_LABELS.
export const PILLAR_ACTIONS = QUALITY_GATE_LABELS;

// --- Siblings ---

export type SiblingId = 'soul' | 'eva' | 'corso' | 'quantum' | 'seraph' | 'ayin' | 'larc';

export const SIBLINGS: SiblingId[] = ['soul', 'eva', 'corso', 'quantum', 'seraph', 'ayin', 'larc'];

export interface SiblingHealth {
  id: SiblingId;
  status: 'online' | 'degraded' | 'offline' | 'unconfigured';
  uptime: number;
  lastHeartbeat: string;
  capabilities: string[];
}

// --- Sibling Dispatch ---

export interface SiblingDispatch {
  id: string;
  buildId: string;
  sibling: SiblingId;
  agent: string;
  prompt: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  startedAt?: string;
  completedAt?: string;
  result?: string;
}

// --- Findings & Artifacts ---

export interface Finding {
  id: string;
  buildId: string;
  pillar: Pillar;
  severity: 'info' | 'warning' | 'error' | 'critical';
  category: 'quality' | 'security' | 'semver' | 'performance' | 'documentation';
  title: string;
  description: string;
  verified: boolean;
  file?: string;
  line?: number;
}

export interface Artifact {
  id: string;
  buildId: string;
  name: string;
  type: 'log' | 'report' | 'coverage' | 'audit' | 'binary';
  size: number;
  url: string;
  createdAt: string;
  pillar?: Pillar; // link to phase gate evidence
}

// --- Build Notes ---

export interface BuildNotes {
  buildId: string;
  content: string; // markdown
  updatedAt: string;
}

// --- Log entries (build output streaming) ---

export type LogLevel = 'debug' | 'info' | 'warn' | 'error' | 'success';

export interface LogEntry {
  id: string;
  timestamp: string;
  level: LogLevel;
  source: string; // e.g., 'corso', 'arena', 'claude-code'
  message: string;
}

// --- Copilot ---

export interface CopilotMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  sibling?: SiblingId;
  timestamp: string;
}

// --- Event types (SSE) ---

export type EventType =
  | 'strand_activation'
  | 'ayin_status'
  | 'ayin_span'
  | 'helix_entry'
  | 'build_update'
  | 'pillar_update'
  | 'finding'
  | 'conductor_task'
  | 'arena_update'
  | 'sibling_status'
  | 'gateway_notify'
  | 'copilot_response'
  | 'copilot_activity'
  | 'control'
  | 'soul_promotion'
  | 'supervisor_update'
  | 'supervisor_decision'
  | 'plan_update'
  | 'scrum_report'
  | 'training_progress'
  | 'fs_mutation_pending'
  | 'permission_request'
  | 'strand_convergence'
  | 'mailbox_message'
  | 'context_status';

// --- Agent protocol (native agent bridge) ---

/** Execution mode from the PICK classifier. */
export type AgentExecutionMode = 'solo' | 'solo_with_verify' | 'squad';

/** A single action queued in plan mode, awaiting operator review. */
export interface QueuedAction {
  tool: string;
  target: string;
  preview: string;
  args: unknown;
  tool_call_id: string;
}

/** Agent event streamed from the native agent bridge. */
export type AgentEvent =
  // ── Core streaming ──────────────────────────────────────────────────────
  | { type: 'text'; chunk: string }
  | { type: 'thinking'; content: string }
  | { type: 'tool_start'; name: string; id: string; input: unknown }
  | { type: 'tool_complete'; id: string; success: boolean; duration_ms: number; result?: string }
  | { type: 'complete'; reason: { kind: 'complete' | 'max_iterations' | 'token_budget_exhausted' | 'user_cancelled' | 'timeout' | 'error' | 'verify_exhausted'; message?: string } }
  | { type: 'error'; message: string; recoverable?: boolean }
  | { type: 'token_usage'; input: number; output: number }
  | { type: 'status_update'; text: string }
  | { type: 'heartbeat' }
  // ── HITL permission ──────────────────────────────────────────────────────
  | { type: 'permission_request'; call_id: string; tool: string; summary: string; agent_id: string; timeout_secs: number }
  // ── Phase 5 TRUST hooks ──────────────────────────────────────────────────
  | { type: 'pick_classified'; mode: AgentExecutionMode }
  | { type: 'discover_injected'; entry_count: number; chars_injected: number }
  | { type: 'verify_complete'; passed: boolean; retries_used: number }
  | { type: 'verify_failed'; reason: string }
  | { type: 'reflect_complete'; significance: number; enrich_triggered: boolean }
  | { type: 'cost_gate_check'; projected_usd: number; gate_usd: number }
  // ── Phase 10+ advanced ───────────────────────────────────────────────────
  | { type: 'squad_suggestion'; preset: string; reason: string }
  | { type: 'strand_bump'; strand: number; delta: number }
  | { type: 'security_violation'; event_type: string; tool: string; path?: string; detail: string }
  | { type: 'sandbox_blocked'; tool: string; attempted_path: string; reason: string }
  | { type: 'resource_limit_hit'; tool: string; limit_type: string; value: number; max: number }
  | { type: 'exec_server_status'; connected: boolean; pid?: number }
  | { type: 'provider_fallback'; from: string; to: string }
  // ── Phase 11 lens system ─────────────────────────────────────────────────
  | { type: 'lenses_selected'; lenses: string[]; tier: number }
  | { type: 'lens_assessment'; sibling: string; tier: number; finding?: string; confidence: number }
  // ── Phase 14 child agents ────────────────────────────────────────────────
  | { type: 'child_agent_forked'; child_name: string; task_id: string; cwd: string }
  | { type: 'child_agent_completed'; child_name: string; task_id: string; success: boolean; summary: string }
  // ── Plan mode ────────────────────────────────────────────────────────────
  | { type: 'plan_queue_ready'; actions: QueuedAction[] };

// --- CORSO scout plan (PlanView) ---

export type PlanPhaseStatus = 'pending' | 'active' | 'complete' | 'failed' | 'skipped';

export interface PlanPhase {
  id: number;
  title: string;
  status: PlanPhaseStatus;
  files: string[];
  description: string;
}

export interface ActivePlan {
  id: string;
  title: string;
  phases: PlanPhase[];
  createdAt: number;
}

// --- Build Plan System (Phase 25 — schema v1.1) ---
// Full lifecycle: pre-flight → phases with mandatory exit gates → close-out
// Covers: 7 CORSO pillars, 9 domain gates, agentic SDLC, research enrichment

/** Gate types — maps to LASDLC-TEMPLATE-v1.yaml Section 3 (inter-phase gates) */
export type GateType =
  | 'quality'       // fmt, clippy, tests, test_ratchet
  | 'structural'    // ownership_drift, contract_drift, boundary_drift
  | 'testing'       // unit, contract, integration, roundtrip, real_world
  | 'security'      // injection, permission_bypass, scope_escape, redaction
  | 'complexity'    // no O(n²), cyclomatic ≤ 10, function ≤ 60 lines
  | 'clean_room'    // no code copying, attribution
  | 'custom';       // user-defined criteria

/** Single criterion within a gate */
export interface GateCriterion {
  id: string;                        // e.g., 'fmt_clean', 'tests_pass'
  label: string;                     // human-readable description
  type: 'automated' | 'manual';     // automated = skill/command, manual = HITL checkbox
  passed: boolean;
  evidence?: string;                 // artifact path, log excerpt, or URL
}

/** Gate status lifecycle */
export type GateStatus = 'pending' | 'passed' | 'failed' | 'waived' | 'blocked';

/** Exit gate — mandatory between every work phase */
export interface ExitGate {
  type: GateType;
  criteria: GateCriterion[];
  status: GateStatus;
  evaluated_by?: string;             // 'corso' | 'seraph' | 'quantum' | 'human'
  evaluated_at?: string;             // ISO-8601
  hitl_required?: boolean;           // if true, cannot auto-pass
  skill?: string;                    // '/GATE' | '/SECURE' | '/TESTING' | '/WIRING'
  fallback_command?: string;         // e.g., "cargo fmt --check && cargo clippy -D warnings"
}

/** Research enrichment attached to a phase */
export interface PhaseResearch {
  context7_refs?: string[];          // library doc references
  security_findings?: string[];      // SERAPH scan results
  prior_art?: string[];              // QUANTUM research
  enriched_at?: string;              // ISO-8601
  enriched_by?: string;              // 'quantum' | 'seraph' | 'context7'
}

/** Work phase with mandatory exit gate */
export interface PhaseWithGates {
  id: number;
  title: string;
  status: PlanPhaseStatus;
  description: string;
  items?: string[];                  // task checklist
  files_expected?: string[];         // files this phase touches
  deliverables?: string[];           // what this phase produces
  meta_skill?: MetaSkill;            // override per-phase
  assigned_sibling?: string;         // SQUAD member for this phase
  research?: PhaseResearch;          // populated by enrichment
  exit_gate: ExitGate;              // MANDATORY — enforced by validator
}

/** 9 domain gate categories (opt-in per build based on what it touches) */
export type DomainGateCategory =
  | 'security'        // auth, network, file I/O, user input
  | 'ui_ux'           // frontend changes
  | 'dx'              // SDK/API ergonomics
  | 'optimization'    // hot paths, performance
  | 'proofing'        // determinism, idempotency, serialization
  | 'research'        // new libs/patterns
  | 'observability'   // tracing, logging
  | 'memory'          // event/vault operations
  | 'retrieval';      // search/indexing

/** Pre-flight check (Section 0 of template v2) */
export interface PreFlightCheck {
  id: string;                        // '0a' through '0k'
  label: string;
  blocking: boolean;
  status: 'pending' | 'passed' | 'failed' | 'skipped';
  skill?: string;                    // '/PLAN', '/RESEARCH', '/RISK-ANALYSIS'
  fallback_command?: string;
  output?: string;                   // result or artifact path
}

/** Close-out step (Section 5 of template v2) */
export interface CloseOutStep {
  id: string;                        // '5a' through '5f'
  label: string;
  status: 'pending' | 'complete' | 'skipped';
  skill?: string;                    // '/REFLECT', '/ENRICH', '/SCRUM', '/DEPLOY'
  output?: string;
}

/** Agentic SDLC configuration (Section 6 of template v2) */
export interface AgenticConfig {
  /** File-ownership partitioning (Canon XXIII) */
  agent_composition?: string;
  /** Token estimate per phase */
  context_budget?: string;
  /** ExecutionPolicy per phase */
  tool_permissions?: string;
  /** LLM retry, MCP fallback, context overflow strategy */
  fallback_chains?: string;
  /** What requires HITL vs autonomous */
  hitl_protocol?: string;
}

/** Full build plan — complete lifecycle with pre-flight, gated phases, close-out */
export interface BuildPlan {
  // Identity
  name: string;
  codename: string;                  // adjective-gerund-noun
  version: string;                   // semver target
  description: string;

  // Classification
  meta_skill: MetaSkill;
  priority: Priority;
  source: IntakeSource;
  tier: number;                      // 1–5 (project maturity)
  build_tier?: BuildTier;            // LASDLC complexity: SMALL | MEDIUM | LARGE
  status: 'planned' | 'in_progress' | 'complete' | 'failed' | 'archived';

  // Project
  path: string;                      // repository path
  language?: string;
  binary?: string;
  deploy?: string;

  // Lifecycle — full gated pipeline
  pre_flight: PreFlightCheck[];      // Section 0 (11 checks)
  phase_detail: PhaseWithGates[];    // Sections 1-4 (phases with mandatory exit gates)
  domain_gates: DomainGateCategory[]; // Which domain gates are active for this build
  close_out: CloseOutStep[];          // Section 5 (6 steps)
  agentic?: AgenticConfig;            // Section 6 (agentic SDLC)

  // Progress tracking
  phases: number;                    // total phase count
  current_phase: number;
  phase_status: string;              // human-readable current status

  // Dependencies
  siblings: string[];
  blocked_by?: string[];             // codenames of blocking builds
  blocks?: string[];                 // codenames this blocks

  // Dates
  created_date?: string;
  completed_date?: string;
  plan?: string;                     // path to .claude/plans/*.md
}

// --- Activity tab (Phase 20) ---

/** Live copilot subprocess event streamed during a turn. */
export interface CopilotActivityEvent {
  build_id: string;
  /** Event category: assistant, content_block_start, content_block_delta, tool_use, result, etc. */
  kind: string;
  /** Human-readable summary (first ~200 chars of content). */
  summary?: string;
  /** Full raw JSON for verbose/auditable mode. */
  raw: unknown;
  /** ISO-8601 timestamp. */
  timestamp: string;
  /** Number of agentic loop iterations at the time this event was emitted. */
  loop_count?: number;
}

/** Context-window utilisation snapshot from the LightArchitects CLI subprocess. */
export interface ContextStatusEvent {
  /** Usage as a fraction of the context window (0.0–1.0). */
  usage_pct: number;
  /** Active compaction level: null, "l1", "l2", or "l3". */
  level?: string;
  /** Total token budget for this session. */
  budget: number;
  /** Tokens consumed so far in this session. */
  used: number;
}

/** Context-window utilisation snapshot from the LightArchitects CLI subprocess. */
export interface ContextStatusEvent {
  /** Usage as a fraction of the context window (0.0–1.0). */
  usage_pct: number;
  /** Active compaction level: null, "l1", "l2", or "l3". */
  level?: string;
  /** Total token budget for this session. */
  budget: number;
  /** Tokens consumed so far in this session. */
  used: number;
}

/** AYIN trace span forwarded from the backend. */
export interface AyinSpanEvent {
  id: string;
  parent_id?: string;
  actor: string;
  action: string;
  timestamp: string;
  duration_ms: number;
  outcome: unknown;
  metadata?: unknown;
  strand_activations?: unknown[];
}

/** Supervisor decision verdict — gate pass/fail/warn from CORSO alpha, guard, quality. */
export type SupervisorVerdict = 'PASS' | 'FAIL' | 'WARN';

/** Supervisor gate type — which CORSO gate produced this decision. */
export type SupervisorGate = 'guard' | 'alpha' | 'quality' | 'canon';

/** A supervisor decision alert surfaced from CORSO gate evaluations. */
export interface SupervisorAlert {
  id: string;
  timestamp: number;
  sibling: string;
  gate: SupervisorGate;
  verdict: SupervisorVerdict;
  message: string;
  details?: string;
}

/** Unified Activity feed entry — copilot event, AYIN span, or supervisor alert. */
export type ActivityEntry =
  | { source: 'copilot'; event: CopilotActivityEvent }
  | { source: 'ayin'; span: AyinSpanEvent }
  | { source: 'supervisor'; alert: SupervisorAlert };

// --- Northstar supervisor (copilot-supervised-orchestration) ---

/** Result of evaluating a completed wave against the operator's northstar. */
export interface NorthstarEvaluationEvent {
  /** Build UUID this evaluation belongs to. */
  build_id: string;
  /** Wave index (0-based) this evaluation covers. */
  wave_num: number;
  /** Alignment verdict: `"advancing"` | `"neutral"` | `"drifting"`. */
  status: 'advancing' | 'neutral' | 'drifting';
  /** Model confidence in the verdict, clamped to `[0, 1]`. */
  confidence: number;
  /** Suggested next action for the operator when drifting. */
  recommended_next: string;
  /** Whether a proposal card is currently awaiting acknowledgement. */
  proposal_pending: boolean;
}

/** Point-in-time snapshot returned by `GET /api/builds/:id/supervisor/state`. */
export interface SupervisorState {
  /** Operator's declared northstar text, or `null` if not set. */
  northstar_text: string | null;
  /** Number of consecutive drifting wave evaluations. */
  consecutive_drifts: number;
  /** Drift count threshold at which a proposal card is triggered. */
  drift_threshold: number;
  /** Whether a proposal is currently awaiting operator acknowledgement. */
  proposal_pending: boolean;
  /** Last completed wave evaluation, or `null` if no waves evaluated yet. */
  last_evaluation: NorthstarEvaluationEvent | null;
}

// --- SOUL vault hybrid memory (Phase 9) ---

/** Which memory tier an entry belongs to. */
export type MemoryTier = 'hot' | 'cold';

/**
 * Projection of a turnlog or helix entry for UI display.
 *
 * Hot memos come from active-session turnlogs (`/api/soul/memory/hot`);
 * cold memos come from promoted helix entries (`/api/soul/memory/cold`).
 */
export interface ContextMemo {
  id: string;
  tier: MemoryTier;
  content: string;
  significance: number;
  sibling: string;
  strands: string[];
  created_at: string;
  source_path?: string;
  // Phase 13.1 — zettelkasten primitives. All optional so old payloads still
  // deserialize; undefined/empty defaults render as "not present" in the UI.
  resonance?: string[];
  themes?: string[];
  self_defining?: boolean;
  entry_type?: string;
}

/**
 * Enriched helix entry — carries front-matter + excerpt for list/search display
 * without requiring a raw-markdown round trip.
 */
export interface EnrichedHelixEntry {
  path: string;
  sibling: string;
  significance?: number;
  strands: string[];
  content_excerpt?: string;
  created_at?: string;
  frontmatter_raw?: Record<string, unknown>;
  entry_type?: string;
}

/**
 * Enriched `helix_entry` SSE payload (Phase 9.3).
 *
 * Superset of the historical `{path, event_kind}` shape — all enrichment
 * fields are optional so old events still deserialize.
 */
export interface HelixEntrySsePayload {
  path: string;
  event_kind: 'created' | 'modified';
  sibling?: string;
  significance?: number;
  strands?: string[];
  content_excerpt?: string;
  created_at?: string;
  entry_type?: string;
}

/**
 * `soul_promotion` SSE payload (Phase 9.4).
 *
 * Emitted by the Rust webshell when a hot memo crosses the promotion gate
 * and is durably written to the cold helix tier.
 */
export interface SoulPromotionPayload {
  memo_id: string;
  from: 'hot';
  to: 'cold';
  path: string;
  sibling: string;
  significance: number;
  promoted_at: string;
}

/**
 * Phase 16 — retention policy shape (tagged enum matches the Rust
 * `RetentionPolicy::{KeepNewest, AgeLimit, SignificanceTier}`).
 *
 * `kind` is the serde discriminator (`snake_case`); remaining fields are
 * policy-specific. Typed as a discriminated union so the UI can pattern-
 * match on `kind` without runtime checks.
 */
export type RetentionPolicy =
  | { kind: 'keep_newest'; n: number }
  | { kind: 'age_limit'; max_days: number }
  | { kind: 'significance_tier'; min_significance: number };

/** Phase 16 — one entry flagged for compaction. */
export interface CompactionCandidate {
  path: string;
  sibling: string;
  significance: number;
  created_at: string;
  reason: string;
}

/** Phase 16 — preview/apply response envelope. */
export interface CompactionSummary {
  total_scanned: number;
  candidates: CompactionCandidate[];
  /** Count of entries the permanent guard protected (self_defining OR ≥0.9). */
  permanent_skipped: number;
  /** Echo of the policy evaluated, so the UI doesn't round-trip. */
  policy: RetentionPolicy;
}

/**
 * `pillar_update` SSE payload (Phase 15).
 *
 * Emitted by the Rust webshell's `trigger_pillar` runner as a `corso <cmd>`
 * subprocess produces output. Lifecycle phases:
 *   * `started`   — one event before spawn, `line` holds the command string
 *   * `output`    — one event per stdout line, `line` is the line contents
 *   * `completed` — one final event with `exit_code` and (on success) `artifact`
 */
export interface PillarUpdatePayload {
  build_id: string;
  pillar: string;
  phase: 'started' | 'output' | 'completed';
  line?: string;
  exit_code?: number;
  /** Relative artifact path under `~/lightarchitects/corso/builds/{build_id}/`. */
  artifact?: string;
}

// --- Auth profiles ---

export type AuthProfile = 'anthropic' | 'ollama' | 'lightarchitects';

export interface OllamaConfig {
  baseUrl: string;
  model: string;
  apiKey: string;
}

// --- Scrum Report (squad review output) ---

export type ScrumFindingCategory = 'good' | 'gap' | 'fix';
export type ScrumSeverity = 'critical' | 'high' | 'medium' | 'low' | 'info';

export interface ScrumFinding {
  sibling: string;
  category: ScrumFindingCategory;
  severity?: ScrumSeverity;
  text: string;
  file?: string;
  line?: number;
}

export interface ScrumReport {
  id: string;
  title: string;
  timestamp: number;
  findings: ScrumFinding[];
  consensus?: string;
  conflicts?: string[];
}

export interface SSEEvent {
  type: EventType;
  data: unknown;
  timestamp: string;
}

// --- Conductor Queue ---

export type ConductorTaskStatus = 'pending' | 'running' | 'completed' | 'failed';

export interface ConductorTask {
  id: string;
  buildId: string;
  sibling: SiblingId;
  taskType: string; // e.g., 'SCOUT', 'FETCH', 'SNIFF'
  priority: 'high' | 'normal' | 'low';
  status: ConductorTaskStatus;
  queuedAt: string;
  startedAt?: string;
  completedAt?: string;
  result?: string;
  error?: string;
}

// --- Arena Status ---

export interface ArenaAgent {
  id: string;
  sibling: SiblingId;
  status: 'active' | 'idle' | 'error';
  lastHeartbeat: string;
  currentBuildId?: string;
  routineCount: number;
}

export interface ArenaStatus {
  activeRoutines: number;
  queuedRoutines: number;
  agents: ArenaAgent[];
  lastUpdate: string;
}

// --- Arena Training ---

export type ExerciseType =
  | 'code_review'
  | 'bug_fix'
  | 'refactor'
  | 'test_gen'
  | 'architecture'
  | 'security_audit'
  | 'optimization';

export type DatasetSource = 'current_project' | 'helix_history' | 'custom_path';

export type ScoringDimension =
  | 'correctness'
  | 'completeness'
  | 'efficiency'
  | 'style'
  | 'security'
  | 'robustness'
  | 'clarity'
  | 'innovation';

export interface TrainingConfig {
  exerciseType: ExerciseType | '';
  weights: Record<ScoringDimension, number>;
  datasetSource: DatasetSource;
  customPath?: string;
}

export type TrainingRunStatus = 'configuring' | 'running' | 'complete' | 'failed';

export interface TrainingRun {
  id: string;
  status: TrainingRunStatus;
  progress: number;
  startedAt?: number;
  completedAt?: number;
  results?: { score: number; exercises: number; passed: number };
}

// --- Platform Alerts ---

export type AlertSeverity = 'info' | 'warning' | 'error' | 'critical';

export interface Alert {
  id: string;
  severity: AlertSeverity;
  source: 'webhook' | 'system' | 'sibling' | 'arena';
  title: string;
  message: string;
  timestamp: string;
  acknowledged: boolean;
  buildId?: string;
  siblingId?: SiblingId;
}

// --- Sitrep Aggregate ---

export interface SitrepData {
  platform: {
    status: 'healthy' | 'degraded' | 'offline';
    version: string;
    uptime: number;
  };
  siblings: Record<SiblingId, SiblingHealth>;
  builds: {
    total: number;
    inProgress: number;
    queued: number;
    completed: number;
    failed: number;
  };
  conductor: {
    queueDepth: number;
    activeTasks: number;
  };
  arena: ArenaStatus;
  alerts: Alert[];
  lastUpdated: string;
}

// --- Webshell PTY session (POST /api/builds response) ---

export interface BuildResponse {
  build_id: string;
  cwd?: string;
  agent?: { kind: string; backend?: string };
  claude_agent_template?: string | null;
  model?: string | null;
}

// --- Intake / Build Creation ---

export type IntakeSource = 'manual' | 'github' | 'audit' | 'discovery';

export type Priority = 'high' | 'medium' | 'low';

export interface BuildRequest {
  metaSkill: MetaSkill;
  source: IntakeSource;
  priority: Priority;
  repoPath: string;
  description?: string;
  siblingOverride?: SiblingId;
}

export interface MetaSkillCard {
  skill: MetaSkill;
  label: string;
  description: string;
  sibling: SiblingId;
  pillarActions: Record<Pillar, string>;
}

// --- Wave data (from sibling oscilloscope) ---

export const BUF_LEN = 56;
export const DECAY = 0.88;
export const PHASE_STEP = 0.38;
export const AMP_SCALE = 0.55;
export const PEAK_THRESHOLD = 0.7;
const ACTIVITY_EPS = 0.001;
const IS_ACTIVE_EPS = 0.01;

export class SiblingWave {
  activity = 0;
  phase = 0;
  samples: number[];
  ttsBoost = 1.0;

  constructor() {
    this.samples = new Array(BUF_LEN).fill(0);
  }

  spike(): void {
    this.activity = 1.0;
  }

  tick(): void {
    this.activity = Math.max(this.activity * DECAY, 0);
    if (this.activity < ACTIVITY_EPS) this.activity = 0;
    if (this.activity > 0) this.phase += PHASE_STEP;
    const effective = this.activity * this.ttsBoost;
    const sample = effective * Math.sin(this.phase) * AMP_SCALE;
    this.samples.shift();
    this.samples.push(sample);
  }

  isActive(): boolean {
    return this.activity > IS_ACTIVE_EPS;
  }
}

// ============================================================================
// Mosaic Panel Layout — recursive flex-tree (Zed flex-ratio model)
// ============================================================================

/** Named panel IDs — each maps to a concrete Svelte component in PanelHost. */
export type PanelId =
  | 'copilot'
  | 'terminal'
  | 'git-forest'
  | 'agent-console'
  | 'file-diff'
  | 'file-explorer'
  | 'build-status'
  | 'findings'
  | 'helix'
  | 'ayin-traces';

/** The six built-in layout presets. */
export type LayoutPreset = 'ops' | 'ide' | 'debug' | 'pr-review' | 'focus' | 'observe';

/**
 * Recursive panel tree using Zed's flex-ratio model.
 * flexes[i] / sum(flexes) gives each child's proportional size.
 * Invariant: flexes.length === children.length.
 * Validation on load: if |sum(flexes) - children.length| > 0.001, reset to [1, 1, ...].
 */
export type PanelTree =
  | { type: 'axis'; direction: 'row' | 'column'; children: PanelTree[]; flexes: number[] }
  | { type: 'tabgroup'; activeIndex: number; tabs: PanelId[] }
  | { type: 'leaf'; panelId: PanelId };

/** Context a panel writes on focus — consumed by CopilotDrawer. */
export type PanelContext =
  | { type: 'file-diff'; path: string; diff?: string }
  | { type: 'branch'; name: string; commits?: string[]; gates?: PillarGate[] }
  | { type: 'agent-log'; buildId: string; recentEvents?: AgentEvent[] }
  | { type: 'finding'; finding: Finding }
  | { type: 'terminal'; recentOutput: string }
  | { type: 'git-forest'; repoName: string }
  | { type: 'helix'; query?: string }
  | { type: 'ayin-traces'; spanCount?: number };

/** Cross-panel navigation request (e.g. finding → file-diff at line N). */
export interface PanelNavRequest {
  targetPanel: PanelId;
  path?: string;
  line?: number;
  findingId?: string;
}

// ── Plan Draft (plan-builder-copilot-bridge Phase 3) ─────────────────────────

/** Parsed review verdict from the `/PLAN` Step 5 self-review loop. */
export interface ReviewVerdict {
  /** `VALIDATED` | `INSUFFICIENT_EVIDENCE` | `REVISION_NEEDED` */
  validation_status: string;
  /** Which LASDLC review iteration this verdict closes (1-based). */
  iteration: number;
  /** Human-readable summary of what passed / needs revision. */
  summary: string | null;
}

/**
 * Streaming event emitted over `GET /api/builds/plan/draft-stream/:session_id`.
 *
 * Discriminated on `type` (snake_case, matching Rust `#[serde(tag = "type", rename_all = "snake_case")]`).
 */
export type PlanDraftEvent =
  | { type: 'text_chunk';      text: string }
  | { type: 'iteration_start'; iteration: number }
  | { type: 'verdict_block';   verdict: ReviewVerdict }
  | { type: 'done';            codename: string }
  | { type: 'error';           message: string };

/** Request body for `POST /api/builds/plan/draft`. */
export interface PlanDraftRequest {
  description: string;
  northstar?:  string;
  repository?: string;
  research:    boolean;
  tier?:       string;
}

/** Immediate response from `POST /api/builds/plan/draft`. */
export interface PlanDraftResponseEnvelope {
  session_id: string;
  codename:   string;
  sse_url:    string;
}

/** Request body for `POST /api/builds/plan/commit`. */
export interface PlanCommitRequest {
  session_id:      string;
  codename:        string;
  body:            string;
  idempotency_key?: string;
}