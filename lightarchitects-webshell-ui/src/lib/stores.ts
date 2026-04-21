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
} from './types';
import { SiblingWave, SIBLINGS, PILLARS } from './types';

// --- Connection status ---
export const ayinStatus = writable<'connected' | 'reconnecting' | 'offline'>('reconnecting');

// --- Drawer layout offset (px) — updated by CopilotDrawer so layout can compensate ---
export const drawerHeightPx = writable<number>(32);

// --- Routing ---
export const currentRoute = writable<string>('/');
export const currentBuildId = writable<string | null>(null);
export const currentWorkspaceId = writable<string | null>(null);

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

/** Promotion feed — receives soul_promotion events as they arrive. */
export const promotionFeed = writable<SoulPromotionPayload[]>([]);

/** UI toggle for the MemoryDrawer overlay. */
export const memoryDrawerOpen = writable<boolean>(false);

// --- SOUL vault health (drives helix entity data) ---

/** Per-sibling entry counts from /api/soul/health. Null until fetched. */
export const vaultCounts = writable<Record<string, number> | null>(null);

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
    status: 'offline' as const,
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
export const copilotMessages = writable<CopilotMessage[]>([]);
export const copilotLoading = writable<boolean>(false);

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

// --- Derived: active build ---
export const activeBuild = derived(
  [builds, currentBuildId],
  ([$builds, $id]) => $id ? $builds.find(b => b.id === $id) ?? null : null
);

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

export const META_SKILL_CARDS: MetaSkillCard[] = Object.entries(PILLAR_ACTIONS_TYPE).map(([skill, actions]) => ({
  skill: skill as MetaSkill,
  label: skill.replace('/', ''),
  description: META_SKILL_DESCRIPTIONS[skill] ?? '',
  sibling: (META_SKILL_TO_SIBLING[skill] ?? 'soul') as SiblingId,
  pillarActions: actions,
}));

// --- Intake form state ---
export const intakeForm = writable<BuildRequest>({
  metaSkill: '/BUILD',
  source: 'manual',
  priority: 'medium',
  repoPath: '',
  description: '',
});
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
  try {
    const [ws, conductor, arena, siblings, hot, cold, soulHealth] = await Promise.allSettled([
      api.listWorkspaces(),
      api.getConductor(),
      api.getArena(),
      api.getSiblingStatus(),
      api.getHotMemory(),
      api.getColdMemory(),
      api.getSoulHealth(),
    ]);
    if (ws.status === 'fulfilled') workspaces.set(ws.value);
    if (conductor.status === 'fulfilled') {
      conductorTasks.set((conductor.value as { nodes: ConductorTask[] }).nodes ?? []);
    }
    if (arena.status === 'fulfilled') arenaStatus.set({ ...arena.value, agents: arena.value.agents ?? [] });
    if (siblings.status === 'fulfilled') {
      const healthMap = Object.fromEntries(siblings.value.map(s => [s.id, s])) as Record<SiblingId, SiblingHealth>;
      siblingHealth.set(healthMap);
    }
    if (hot.status === 'fulfilled') hotMemory.set(hot.value);
    if (cold.status === 'fulfilled') coldMemory.set(cold.value);
    if (soulHealth.status === 'fulfilled') {
      vaultCounts.set(soulHealth.value.counts);
    }
  } catch {
    // Server offline — stores remain at empty defaults
  }
}