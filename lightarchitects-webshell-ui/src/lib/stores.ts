// ============================================================================
// Svelte stores — reactive state management (replaces Zustand)
// ============================================================================

import { writable, derived } from 'svelte/store';
import type {
  Build, Workspace, Module, SiblingHealth, Finding,
  SiblingId, PillarGate, CopilotMessage, SiblingDispatch,
  Pillar, BuildStatus, LogEntry, LogLevel, Artifact, BuildNotes,
  ConductorTask, ArenaStatus, Alert, ConductorTaskStatus,
  IntakeSource, Priority, BuildRequest, MetaSkillCard, MetaSkill,
  AuthProfile, OllamaConfig,
  ContextMemo, HelixEntrySsePayload, SoulPromotionPayload, PillarUpdatePayload,
  SupervisorAlert,
  ActivePlan, PlanPhase, PlanPhaseStatus,
  ScrumReport,
  TrainingConfig, TrainingRun, ScoringDimension,
} from './types';
import { SiblingWave, SIBLINGS, PILLARS } from './types';
import { DEFAULT_SKIN, type HelixSkin } from './helix-skin';
import type { TextureMode } from './helix/procedural-textures';
export type { TextureMode };
export { TEXTURE_MODES, TEXTURE_LABELS } from './helix/procedural-textures';

// --- Connection status ---
export const ayinStatus = writable<'connected' | 'reconnecting' | 'offline'>('reconnecting');

// --- Auth status — set by SSE on 401/403; drives AuthBanner (#13) ---
export const authStatus = writable<'ok' | 'unauthorized' | 'forbidden'>('ok');

// --- Helix skin (3D visual customization, persisted via setup/save) ---
export const activeSkin = writable<HelixSkin>(DEFAULT_SKIN);

// --- Drawer layout offset (px) — updated by CopilotDrawer so layout can compensate ---
export const drawerHeightPx = writable<number>(32);

// --- Routing ---
export const currentRoute = writable<string>('/');
export const currentBuildId = writable<string | null>(null);
export const currentWorkspaceId = writable<string | null>(null);

// --- Global events overlay (Wave 1.5) ---
export const eventsOverlayOpen = writable<boolean>(false);

// --- Build data ---
export const workspaces = writable<Workspace[]>([]);
export const builds = writable<Build[]>([]);

// --- Findings ---
export const findings = writable<Finding[]>([]);

// --- Log entries ---
export const logEntries = writable<LogEntry[]>([]);

// --- Artifacts ---
export const artifacts = writable<Artifact[]>([]);

// --- Build notes (keyed by buildId) ---
export const buildNotes = writable<Record<string, BuildNotes>>({});

// --- Expanded findings (track which findings are expanded) ---
export const expandedFindings = writable<Set<string>>(new Set());

// --- Selected artifact (for detail view) ---
export const selectedArtifact = writable<Artifact | null>(null);

// --- Notes editing state ---
export const notesEditing = writable<boolean>(false);

// --- Conductor queue ---
export const conductorTasks = writable<ConductorTask[]>([]);

// --- Arena status ---
const DEFAULT_ARENA: ArenaStatus = { activeRoutines: 0, queuedRoutines: 0, agents: [], lastUpdate: '' };
export const arenaStatus = writable<ArenaStatus>(DEFAULT_ARENA);

// --- Arena training ---
const DEFAULT_WEIGHTS: Record<ScoringDimension, number> = {
  correctness: 50, completeness: 50, efficiency: 50, style: 50,
  security: 50, robustness: 50, clarity: 50, innovation: 50,
};
export const trainingConfig = writable<TrainingConfig>({
  exerciseType: '',
  weights: { ...DEFAULT_WEIGHTS },
  datasetSource: 'current_project',
});
export const trainingRun = writable<TrainingRun | null>(null);

// --- Alerts ---
export const alerts = writable<Alert[]>([]);

// --- Alert acknowledgment tracking ---
export const acknowledgedAlerts = writable<Set<string>>(new Set());

// --- SOUL vault hybrid memory (Phase 9) ---

/** Hot-tier memos — active-session turnlog projections. Newest-first. */
export const hotMemory = writable<ContextMemo[]>([]);

/** Cold-tier memos — promoted helix entries. Newest-first. */
export const coldMemory = writable<ContextMemo[]>([]);

/** Rolling window of recent helix_entry SSE events (for Helix3D orb spawn). */
export const helixEntries = writable<HelixEntrySsePayload[]>([]);

/** Active procedural texture mode for Helix3D polytope faces. */
export const helixTextureMode = writable<TextureMode>('noise');

/** Promotion feed — receives soul_promotion events as they arrive. */
export const promotionFeed = writable<SoulPromotionPayload[]>([]);

/** UI toggle for the MemoryDrawer overlay. */
export const memoryDrawerOpen = writable<boolean>(false);

// --- SOUL vault health (drives helix entity data) ---

/** Per-sibling entry counts from /api/soul/health. Null until fetched. */
export const vaultCounts = writable<Record<string, number> | null>(null);

// --- Helix interaction state ---

/** Currently hovered active node in the helix — drives tooltip. */
export const activeHelixNode = writable<{
  sibling: string;
  path: string;
  significance: number;
  excerpt: string;
  screenX: number;
  screenY: number;
} | null>(null);


// --- Activity feed (Phase 20) ---

/** Rolling window of live activity events (copilot stream + AYIN spans). Newest-first. */
const ACTIVITY_WINDOW = 500;
export const activityFeed = writable<import('./types').ActivityEntry[]>([]);

/** Whether the copilot is actively processing (streaming events). */
export const activityActive = writable<boolean>(false);

/** @internal Exposed for sse.ts — appends an activity entry, capped at window size. */
export function appendActivity(entry: import('./types').ActivityEntry): void {
  activityFeed.update(list => {
    const next = [entry, ...list];
    return next.length > ACTIVITY_WINDOW ? next.slice(0, ACTIVITY_WINDOW) : next;
  });
}

// --- Supervisor decision alerts (Phase 21) ---

/** Rolling window of supervisor decision alerts (CORSO guard/alpha/quality gate verdicts). Newest-first. */
const SUPERVISOR_ALERTS_WINDOW = 200;
export const supervisorAlerts = writable<SupervisorAlert[]>([]);

/** @internal Exposed for sse.ts — appends a supervisor alert and mirrors it into the activity feed. */
export function appendSupervisorAlert(alert: SupervisorAlert): void {
  supervisorAlerts.update(list => {
    const next = [alert, ...list];
    return next.length > SUPERVISOR_ALERTS_WINDOW ? next.slice(0, SUPERVISOR_ALERTS_WINDOW) : next;
  });
  // Also inject into the unified activity feed so it renders inline
  appendActivity({ source: 'supervisor', alert });
}

// --- CORSO scout plan (PlanView) ---

/** Active CORSO scout plan — drives the PlanView component on the Workspace screen. */
export const activePlan = writable<ActivePlan | null>(null);

/** Latest /SCRUM report — drives the ScrumReport overlay. Null = dismissed / no report. */
export const latestScrumReport = writable<ScrumReport | null>(null);

/**
 * Update a single phase within the active plan without replacing the entire object.
 * If the plan id doesn't match, the update is silently ignored.
 */
export function updatePlanPhase(planId: string, phaseId: number, status: PlanPhaseStatus): void {
  activePlan.update(plan => {
    if (!plan || plan.id !== planId) return plan;
    return {
      ...plan,
      phases: plan.phases.map(p =>
        p.id === phaseId ? { ...p, status } : p
      ),
    };
  });
}

/**
 * Rolling pillar-run event stream (Phase 15) — newest-first. Every pillar
 * subprocess emits `started` + N × `output` + `completed`. UI components
 * filter by `build_id` and `pillar`; completed events carry the artifact path.
 */
const PILLAR_STREAM_WINDOW = 500;
export const pillarStream = writable<PillarUpdatePayload[]>([]);

/** @internal Exposed for sse.ts — appends a pillar_update, capped at window size. */
export function appendPillarUpdate(ev: PillarUpdatePayload): void {
  pillarStream.update(list => {
    const next = [ev, ...list];
    return next.length > PILLAR_STREAM_WINDOW ? next.slice(0, PILLAR_STREAM_WINDOW) : next;
  });
}

// --- Derived: artifacts for active build ---
export const activeBuildArtifacts = derived(
  [artifacts, currentBuildId],
  ([$artifacts, $buildId]) =>
    $buildId ? $artifacts.filter(a => a.buildId === $buildId) : []
);

// --- Selected pillar in workspace view ---
export const selectedPillar = writable<Pillar | null>(null);

// --- Sibling health ---
export const siblingHealth = writable<Record<SiblingId, SiblingHealth>>(
  Object.fromEntries(SIBLINGS.map(s => [s, {
    id: s,
    status: 'unconfigured' as const,
    uptime: 0,
    lastHeartbeat: '',
    capabilities: [],
  }])) as unknown as Record<SiblingId, SiblingHealth>
);

// --- Wave data (oscilloscope) ---
export const waves = writable<Record<string, SiblingWave>>(
  Object.fromEntries(SIBLINGS.map(s => [s, new SiblingWave()]))
);

// --- Focused sibling ---
export const focusedSibling = writable<SiblingId | null>(null);

// --- Copilot ---
const COPILOT_HISTORY_KEY = 'la_copilot_history';
const HISTORY_CAP = 200;

function loadCopilotHistory(): CopilotMessage[] {
  if (typeof window === 'undefined') return [];
  try {
    const raw = localStorage.getItem(COPILOT_HISTORY_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    return Array.isArray(parsed) ? (parsed as CopilotMessage[]).slice(-HISTORY_CAP) : [];
  } catch {
    return [];
  }
}

export const copilotMessages = writable<CopilotMessage[]>(loadCopilotHistory());
export const copilotLoading = writable<boolean>(false);

/** Clears conversation history from the store and localStorage. */
export function clearCopilotHistory(): void {
  copilotMessages.set([]);
  try { localStorage.removeItem(COPILOT_HISTORY_KEY); } catch { /* quota */ }
}

// Persist copilotMessages to localStorage on every change (debounced, cap 200).
let _historyTimer: ReturnType<typeof setTimeout> | null = null;
copilotMessages.subscribe(msgs => {
  if (typeof window === 'undefined') return;
  if (_historyTimer !== null) clearTimeout(_historyTimer);
  _historyTimer = setTimeout(() => {
    _historyTimer = null;
    try {
      const toSave = msgs.slice(-HISTORY_CAP);
      if (toSave.length === 0) {
        localStorage.removeItem(COPILOT_HISTORY_KEY);
      } else {
        localStorage.setItem(COPILOT_HISTORY_KEY, JSON.stringify(toSave));
      }
    } catch { /* storage quota exceeded — silently ignore */ }
  }, 300);
});

/** Whether a build is actively running — drives Layer 2 helix dim effect. */
export const buildFocusActive = derived(
  [copilotLoading, currentBuildId],
  ([$loading, $buildId]) => $loading && Boolean($buildId),
);

/**
 * Vault paths accessed during the current build — Layer 2 highlight set.
 * Populated by SSE handler: helix_entry events that arrive while
 * copilotLoading + currentBuildId are both truthy get tagged.
 */
export const buildAccessedPaths = writable<Set<string>>(new Set());

/** @internal Called by SSE handler when a helix_entry arrives during a build. */
export function tagBuildAccess(path: string): void {
  buildAccessedPaths.update(s => {
    const next = new Set(s);
    next.add(path);
    return next;
  });
}

/** Reset build-accessed paths when a new build starts. */
export function resetBuildAccess(): void {
  buildAccessedPaths.set(new Set());
}

/** Build context string injected into copilot prompts */
export function buildBuildContext(
  build: Build | null,
  pillar: Pillar | null,
  buildFindings: Finding[],
): string {
  if (!build) return '[No active build]';
  const lines: string[] = [
    `Build: ${build.name} (${build.id})`,
    `MetaSkill: ${build.metaSkill}`,
    `Status: ${build.status} | Confidence: ${Math.round(build.confidence * 100)}%`,
    `Current Pillar: ${build.currentPillar}`,
  ];
  if (pillar) {
    const gate = build.pillars.find(p => p.pillar === pillar);
    if (gate) {
      lines.push(`Selected Pillar: ${pillar} (${gate.status}, confidence ${Math.round(gate.confidence * 100)}%)`);
    }
  }
  const findingsCount = buildFindings.length;
  if (findingsCount > 0) {
    const bySeverity = buildFindings.reduce<Record<string, number>>((acc, f) => {
      acc[f.severity] = (acc[f.severity] ?? 0) + 1;
      return acc;
    }, {});
    lines.push(`Findings: ${findingsCount} (${Object.entries(bySeverity).map(([k, v]) => `${v} ${k}`).join(', ')})`);
  }
  return lines.join('\n');
}

// --- Command palette ---
export const commandPaletteOpen = writable(false);

// --- Terminal ---
export const terminalConnected = writable(false);
export const authProfile = writable<AuthProfile>('anthropic');
export const ollamaConfig = writable<OllamaConfig | null>(null);

// --- Mailbox: global inter-agent messages from the platform SSE stream ---
export interface GlobalMailboxMessage {
  id: string;
  dispatchId?: string;
  agent: string;
  text: string;
  ts: number;
}
export const mailboxMessages = writable<GlobalMailboxMessage[]>([]);
export const mailboxUnread = writable(0);

// --- Context-window utilisation (from CLI subprocess NDJSON) ---
export const contextUsage = writable<import('./types').ContextStatusEvent | null>(null);

// --- Agent reactive state (native agent bridge) ---
export const agentConnected = writable(false);
export const agentEvents = writable<import('./types').AgentEvent[]>([]);
export const agentInput = writable('');

/** Cumulative token usage for the active agent session. */
export const agentTokenUsage = derived(agentEvents, ($evs) => {
  let input = 0, output = 0;
  for (const ev of $evs) {
    if (ev.type === 'token_usage') { input += ev.input; output += ev.output; }
  }
  return { input, output };
});

// --- Derived: active build ---
export const activeBuild = derived(
  [builds, currentBuildId],
  ([$builds, $id]) => $id ? $builds.find(b => b.id === $id) ?? null : null
);

// Whether the active build uses the native agent bridge (kind === 'lightarchitects_native')
export const isNativeAgent = derived(activeBuild, ($build) =>
  $build?.agent?.kind === 'lightarchitects_native'
);

// --- Derived: project groups (LASDLC — groups builds by project path) ---
export const projectGroups = derived(builds, ($builds) => groupByProject($builds));

// --- Derived: build stats ---
export const buildStats = derived(builds, ($builds) => ({
  total: $builds.length,
  inProgress: $builds.filter(b => b.status === 'in_progress').length,
  completed: $builds.filter(b => b.status === 'completed').length,
  failed: $builds.filter(b => b.status === 'failed').length,
  pending: $builds.filter(b => b.status === 'queued').length,
}));

// --- Derived: conductor stats ---
export const conductorStats = derived(conductorTasks, ($tasks) => ({
  total: $tasks.length,
  pending: $tasks.filter(t => t.status === 'pending').length,
  running: $tasks.filter(t => t.status === 'running').length,
  completed: $tasks.filter(t => t.status === 'completed').length,
  failed: $tasks.filter(t => t.status === 'failed').length,
  queueDepth: $tasks.filter(t => t.status === 'pending').length,
  activeTasks: $tasks.filter(t => t.status === 'running').length,
}));

// --- Derived: arena stats ---
export const arenaStats = derived(arenaStatus, ($arena) => ({
  activeRoutines: $arena.activeRoutines,
  queuedRoutines: $arena.queuedRoutines,
  activeAgents: $arena.agents.filter(a => a.status === 'active').length,
  idleAgents: $arena.agents.filter(a => a.status === 'idle').length,
  errorAgents: $arena.agents.filter(a => a.status === 'error').length,
}));

// --- Derived: alert stats ---
export const alertStats = derived(alerts, ($alerts) => ({
  total: $alerts.length,
  unacknowledged: $alerts.filter(a => !a.acknowledged).length,
  critical: $alerts.filter(a => a.severity === 'critical').length,
  error: $alerts.filter(a => a.severity === 'error').length,
  warning: $alerts.filter(a => a.severity === 'warning').length,
  info: $alerts.filter(a => a.severity === 'info').length,
}));

// --- Derived: sitrep ready (true when all data loaded) ---
export const sitrepReady = derived(
  [builds, siblingHealth, conductorTasks, arenaStatus, alerts],
  ([$builds, $health, $conductor, $arena, $alerts]) =>
    $builds.length > 0 && Object.keys($health).length > 0 && $conductor.length >= 0 && $arena.agents.length > 0 && $alerts.length >= 0
);

// --- Derived: sibling dispatch counts ---
export const siblingDispatchCounts = derived(
  [conductorTasks],
  ([$tasks]) => {
    const counts: Record<SiblingId, number> = {} as Record<SiblingId, number>;
    for (const sib of SIBLINGS) {
      counts[sib] = $tasks.filter(t => t.sibling === sib && (t.status === 'running' || t.status === 'pending')).length;
    }
    return counts;
  }
);

// --- Derived: platform health status ---
export const platformHealth = derived(
  [siblingHealth, arenaStats, alertStats],
  ([$health, $arena, $alerts]) => {
    const onlineSiblings = Object.values($health).filter(h => h.status === 'online').length;
    const hasErrors = $alerts.critical > 0 || $alerts.error > 0;

    if (onlineSiblings >= 5 && !hasErrors) return 'healthy' as const;
    if (onlineSiblings >= 3 || hasErrors) return 'degraded' as const;
    return 'offline' as const;
  }
);

// --- Meta-skill descriptions for Intake ---

const META_SKILL_DESCRIPTIONS: Record<string, string> = {
  '/BUILD': 'Full build cycle: ARCH→SEC→QUAL→PERF→TEST→DOC→OPS',
  '/RESEARCH': 'Investigation: SCAN→SWEEP→TRACE→PROBE→THEORIZE→VERIFY→CLOSE',
  '/SECURE': 'Security audit: RECON→SURVEY→EXAMINE→STRIKE→REPORT→REMEDIATE→CLOSE',
  '/SQUAD': 'Team coordination: TEAM→AUTH→CHECK→REVIEW→TEST→DOC→SCRUM',
  '/PLAN': 'Architecture planning: SCOUT→FETCH→SNIFF→GUARD→CHASE→HUNT→SCRUM',
  '/DEPLOY': 'Deployment pipeline: SCOUT→FETCH→SNIFF→GUARD→CHASE→HUNT→SCRUM',
  '/REVIEW': 'Code review: SCAN→SWEEP→TRACE→PROBE→THEORIZE→VERIFY→CLOSE',
  '/OBSERVE': 'Observability setup: SCAN→SWEEP→TRACE→PROBE→THEORIZE→VERIFY→CLOSE',
  '/ONBOARD': 'Onboarding automation: SCOUT→FETCH→SNIFF→GUARD→CHASE→HUNT→SCRUM',
  '/OPTIMIZE': 'Performance optimization: SCOUT→FETCH→SNIFF→GUARD→CHASE→HUNT→SCRUM',
  '/REFLECT': 'Retrospective: SCAN→SWEEP→TRACE→PROBE→THEORIZE→VERIFY→CLOSE',
  '/ENRICH': 'Knowledge enrichment: SCAN→SWEEP→TRACE→PROBE→THEORIZE→VERIFY→CLOSE',
};

import { META_SKILL_TO_SIBLING } from './design-tokens';
import { PILLAR_ACTIONS as PILLAR_ACTIONS_TYPE } from './types';
import { api } from './api';
import { loadPersistedSettings } from './settings-persistence';
import { mapPortfolioBuilds, groupByProject } from './build-mapper';

export const META_SKILL_CARDS: MetaSkillCard[] = Object.entries(PILLAR_ACTIONS_TYPE).map(([skill, actions]) => ({
  skill: skill as MetaSkill,
  label: skill.replace('/', ''),
  description: META_SKILL_DESCRIPTIONS[skill] ?? '',
  sibling: (META_SKILL_TO_SIBLING[skill] ?? 'soul') as SiblingId,
  pillarActions: actions,
}));

// --- Intake form state ---
const INTAKE_FORM_DEFAULT: BuildRequest = {
  metaSkill: '/BUILD',
  source: 'manual',
  priority: 'medium',
  repoPath: '',
  description: '',
};
const INTAKE_DRAFT_KEY = 'la.intake.draft';

function isIntakeFormEmpty(f: BuildRequest): boolean {
  return (f.repoPath ?? '').trim() === '' && (f.description ?? '').trim() === '';
}

function loadIntakeDraft(): BuildRequest {
  if (typeof localStorage === 'undefined') return INTAKE_FORM_DEFAULT;
  try {
    const stored = localStorage.getItem(INTAKE_DRAFT_KEY);
    if (!stored) return INTAKE_FORM_DEFAULT;
    const parsed = JSON.parse(stored) as Partial<BuildRequest>;
    return { ...INTAKE_FORM_DEFAULT, ...parsed };
  } catch {
    return INTAKE_FORM_DEFAULT;
  }
}

export const intakeForm = writable<BuildRequest>(loadIntakeDraft());

if (typeof localStorage !== 'undefined') {
  intakeForm.subscribe(form => {
    try {
      if (isIntakeFormEmpty(form)) {
        localStorage.removeItem(INTAKE_DRAFT_KEY);
      } else {
        localStorage.setItem(INTAKE_DRAFT_KEY, JSON.stringify(form));
      }
    } catch {
      /* localStorage write failed — draft is in-memory only */
    }
  });
}

/** True when intakeForm has user-typed content; drives beforeunload guard (#15). */
export const intakeFormDirty = derived(intakeForm, $f => !isIntakeFormEmpty($f));

// --- Plan Builder state (Phase 25) ---
/** Toggle between quick build and plan builder mode in Intake */
export const planBuilderMode = writable<boolean>(false);

/** Active plan being built/edited in Intake Plan Builder */
export const planBuilderDraft = writable<import('./types').BuildPlan | null>(null);

// ── Code editor (EEF Phase 3 Wave 1) ─────────────────────────────────────────

/** In-memory buffer for the code editor screen. */
export interface CodeBuffer {
  /** Absolute or relative path of the open file. */
  path: string;
  /** Current in-editor content (may differ from disk). */
  content: string;
  /** Content at last save — used for dirty-flag tracking. */
  savedContent: string;
  /** Monaco language identifier (e.g. `"rust"`, `"typescript"`). */
  language: string;
}

/** Active code editor buffer. `null` when no file is open. */
export const codeStore = writable<CodeBuffer | null>(null);

let waveInterval: ReturnType<typeof setInterval> | null = null;

export function startWaveTick(): void {
  if (waveInterval) return;
  waveInterval = setInterval(() => {
    waves.update(w => {
      const next = { ...w };
      for (const key of Object.keys(next)) {
        next[key] = Object.assign(Object.create(Object.getPrototypeOf(next[key])), next[key]);
        next[key].tick();
      }
      return next;
    });
  }, 25); // 40Hz
}

export function stopWaveTick(): void {
  if (waveInterval) {
    clearInterval(waveInterval);
    waveInterval = null;
  }
}

// --- Wave spike helper ---
export function spikeSibling(id: SiblingId): void {
  waves.update(w => {
    const wave = w[id];
    if (wave) wave.spike();
    return w;
  });
}

// --- Live data initialization (called on app mount) ---
export async function initializeStores(): Promise<void> {
  // Restore persisted UI settings (drawer height, panel visibility, etc.)
  // before fetching live data so the layout is correct immediately.
  await loadPersistedSettings();

  const [ws, conductor, arena, siblings, hot, cold, soulHealth, buildsResult] = await Promise.allSettled([
    api.listWorkspaces(),
    api.getConductor(),
    api.getArena(),
    api.getSiblingStatus(),
    api.getHotMemory(),
    api.getColdMemory(),
    api.getSoulHealth(),
    api.listBuilds(),
  ]);

  // Log API failures so operators can debug connectivity without reloading DevTools.
  const apiNames = ['workspaces', 'conductor', 'arena', 'siblings', 'hotMemory', 'coldMemory', 'soulHealth', 'builds'];
  [ws, conductor, arena, siblings, hot, cold, soulHealth, buildsResult].forEach((r, i) => {
    if (r.status === 'rejected') {
      console.warn(`[stores] ${apiNames[i]} fetch failed:`, r.reason);
    }
  });

  // Apply fulfilled results — each guarded individually so a data-processing
  // error in one store doesn't silently abort the rest.
  if (ws.status === 'fulfilled') {
    try { workspaces.set(ws.value); } catch (e) { console.error('[stores] workspaces.set failed:', e); }
  }
  if (conductor.status === 'fulfilled') {
    try { conductorTasks.set((conductor.value as { nodes: ConductorTask[] }).nodes ?? []); } catch (e) { console.error('[stores] conductorTasks.set failed:', e); }
  }
  if (arena.status === 'fulfilled') {
    try { arenaStatus.set({ ...arena.value, agents: arena.value.agents ?? [] }); } catch (e) { console.error('[stores] arenaStatus.set failed:', e); }
  }
  if (siblings.status === 'fulfilled') {
    try {
      const healthMap = Object.fromEntries(siblings.value.map(s => [s.id, s])) as Record<SiblingId, SiblingHealth>;
      siblingHealth.set(healthMap);
    } catch (e) { console.error('[stores] siblingHealth.set failed:', e); }
  }
  if (hot.status === 'fulfilled') {
    try { hotMemory.set(hot.value); } catch (e) { console.error('[stores] hotMemory.set failed:', e); }
  }
  if (cold.status === 'fulfilled') {
    try { coldMemory.set(cold.value); } catch (e) { console.error('[stores] coldMemory.set failed:', e); }
  }
  if (soulHealth.status === 'fulfilled') {
    try { vaultCounts.set(soulHealth.value.counts); } catch (e) { console.error('[stores] vaultCounts.set failed:', e); }
  }
  if (buildsResult.status === 'fulfilled') {
    try { builds.set(mapPortfolioBuilds(buildsResult.value)); } catch (e) { console.error('[stores] builds.set failed:', e); }
  }
}