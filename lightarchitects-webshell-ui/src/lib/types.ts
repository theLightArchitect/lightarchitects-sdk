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
}

export type BuildStatus = 'queued' | 'in_progress' | 'completed' | 'failed' | 'paused';

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

/** Per-skill action labels for each pillar */
export const PILLAR_ACTIONS: Record<MetaSkill, Record<Pillar, string>> = {
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

// --- Siblings ---

export type SiblingId = 'soul' | 'eva' | 'corso' | 'quantum' | 'seraph' | 'ayin' | 'larc';

export const SIBLINGS: SiblingId[] = ['soul', 'eva', 'corso', 'quantum', 'seraph', 'ayin', 'larc'];

export interface SiblingHealth {
  id: SiblingId;
  status: 'online' | 'degraded' | 'offline';
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
  | 'supervisor_decision';

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

export type AuthProfile = 'anthropic' | 'ollama';

export interface OllamaConfig {
  baseUrl: string;
  model: string;
  apiKey: string;
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