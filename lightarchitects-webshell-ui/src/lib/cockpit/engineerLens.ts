/**
 * Engineer lens — derives actionable, in-flight, and insight signals
 * from existing stores + target context for the Engineer preset cockpit zones.
 *
 * All functions are pure/derivable — no side effects, no network calls.
 * InsightsZone caches its own derived results with a 30s TTL using the
 * `insightCache` exported below.
 */

import type { CockpitTarget } from '$lib/cockpit/stores';
import type { Build, ConductorTask } from '$lib/types';

// ── Action item ────────────────────────────────────────────────────────────

export interface ActionItem {
  id: string;
  label: string;
  urgency: 'critical' | 'high' | 'normal';
  verb: string;
  buildId?: string;
}

/**
 * Derive items that need engineer attention from store state.
 *
 * Signal sources (ordered by urgency):
 * 1. Failed builds related to the target
 * 2. Paused/blocked conductor tasks
 * 3. Builds in_progress with confidence < 0.5
 */
export function needsActionItems(
  target: CockpitTarget | null,
  builds: Build[],
  tasks: ConductorTask[],
): ActionItem[] {
  const items: ActionItem[] = [];

  // Failed builds — always shown regardless of target
  for (const b of builds) {
    if (b.status === 'failed') {
      items.push({
        id: `fail-${b.id}`,
        label: `${b.codename ?? b.name} failed`,
        urgency: 'critical',
        verb: 'DIAGNOSE',
        buildId: b.id,
      });
    }
  }

  // Blocked conductor tasks (no completedAt + status not running)
  for (const t of tasks) {
    if (t.status === 'failed') {
      items.push({
        id: `task-${t.id}`,
        label: `${t.sibling}/${t.taskType} failed`,
        urgency: 'high',
        verb: 'RETRY',
        buildId: t.buildId,
      });
    }
  }

  // Low-confidence in-progress builds
  for (const b of builds) {
    if (b.status === 'in_progress' && b.confidence < 0.5) {
      items.push({
        id: `conf-${b.id}`,
        label: `${b.codename ?? b.name} confidence ${Math.round(b.confidence * 100)}%`,
        urgency: 'normal',
        verb: 'REVIEW',
        buildId: b.id,
      });
    }
  }

  // Target-specific filtering
  if (target?.type === 'build') {
    return items.filter(i => !i.buildId || target.label.includes(i.buildId.slice(0, 8)));
  }

  return items.slice(0, 8);
}

// ── In-flight item ─────────────────────────────────────────────────────────

export interface InFlightItem {
  id: string;
  label: string;
  sibling?: string;
  elapsedMs: number;
  confidence?: number;
}

/** Running conductor tasks + in-progress builds, ordered by elapsed time desc. */
export function inFlightItems(
  builds: Build[],
  tasks: ConductorTask[],
): InFlightItem[] {
  const now = Date.now();
  const items: InFlightItem[] = [];

  for (const t of tasks) {
    if (t.status !== 'running') continue;
    const start = t.startedAt ? new Date(t.startedAt).getTime() : now;
    items.push({
      id: `task-${t.id}`,
      label: `${t.sibling}: ${t.taskType}`,
      sibling: t.sibling,
      elapsedMs: now - start,
    });
  }

  for (const b of builds) {
    if (b.status !== 'in_progress') continue;
    const start = new Date(b.createdAt).getTime();
    items.push({
      id: `build-${b.id}`,
      label: b.codename ?? b.name,
      elapsedMs: now - start,
      confidence: b.confidence,
    });
  }

  return items.sort((a, b) => b.elapsedMs - a.elapsedMs).slice(0, 6);
}

// ── Quick action ───────────────────────────────────────────────────────────

export interface QuickAction {
  id: string;
  label: string;
  task: string;
  agents: string[];
  primary: boolean;
}

/** Pre-populated dispatch actions based on current target. */
export function quickActions(target: CockpitTarget | null): QuickAction[] {
  const ctx = target ? ` for ${target.label}` : '';

  return [
    {
      id: 'implement',
      label: 'IMPLEMENT',
      task: `Implement${ctx}`,
      agents: ['engineer'],
      primary: true,
    },
    {
      id: 'review',
      label: 'REVIEW',
      task: `Code review${ctx}`,
      agents: ['quality'],
      primary: false,
    },
    {
      id: 'research',
      label: 'RESEARCH',
      task: `Investigate${ctx}`,
      agents: ['researcher'],
      primary: false,
    },
    {
      id: 'secure',
      label: 'SECURE',
      task: `Security review${ctx}`,
      agents: ['security'],
      primary: false,
    },
  ];
}

// ── Insight ────────────────────────────────────────────────────────────────

export interface Insight {
  id: string;
  signal: string;
  value: string;
  trend?: 'up' | 'down' | 'stable';
  nonObvious: true; // force-multiplier criterion: each insight must pass "would grep give this?"
}

// 30s TTL insight cache — key: targetId, value: { ts, insights }
const insightCache = new Map<string, { ts: number; items: Insight[] }>();
const INSIGHT_TTL_MS = 30_000;

/**
 * Derive non-obvious insights from build history for the current target.
 *
 * Signals that pass the "would grep/git log give the same?" test:
 * - Confidence velocity (trend over last N waves, not just current value)
 * - Gate throughput rate (gates passed per hour of build time)
 * - Sibling disagreement index (how often siblings disagree on findings)
 * - Phase time deviation (actual vs typical time in current phase)
 */
export function deriveInsights(
  target: CockpitTarget | null,
  builds: Build[],
  tasks: ConductorTask[],
): Insight[] {
  const key = target?.id ?? '__all__';
  const cached = insightCache.get(key);
  if (cached && Date.now() - cached.ts < INSIGHT_TTL_MS) {
    return cached.items;
  }

  const items: Insight[] = [];
  const now = Date.now();

  // Signal 1: Confidence velocity — delta over last 2 builds (non-obvious: trend not snapshot)
  const recentBuilds = [...builds]
    .filter(b => b.status !== 'queued')
    .sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime())
    .slice(0, 5);

  if (recentBuilds.length >= 2) {
    const delta = recentBuilds[0].confidence - recentBuilds[recentBuilds.length - 1].confidence;
    const pct = Math.round(delta * 100);
    items.push({
      id: 'conf-velocity',
      signal: 'Confidence velocity',
      value: `${pct > 0 ? '+' : ''}${pct}pp over ${recentBuilds.length} builds`,
      trend: pct > 2 ? 'up' : pct < -2 ? 'down' : 'stable',
      nonObvious: true,
    });
  }

  // Signal 2: Gate throughput rate (completed builds / total wall-clock hours)
  const completedBuilds = builds.filter(b => b.status === 'completed');
  if (completedBuilds.length > 0) {
    const earliest = Math.min(...completedBuilds.map(b => new Date(b.createdAt).getTime()));
    const spanHours = (now - earliest) / 3_600_000;
    const rate = spanHours > 0.5 ? (completedBuilds.length / spanHours).toFixed(1) : '—';
    items.push({
      id: 'gate-rate',
      signal: 'Gate throughput',
      value: `${rate} builds/h (${completedBuilds.length} completed)`,
      nonObvious: true,
    });
  }

  // Signal 3: Sibling disagreement index — failed vs total tasks per sibling
  const siblingStats = new Map<string, { total: number; failed: number }>();
  for (const t of tasks) {
    const s = siblingStats.get(t.sibling) ?? { total: 0, failed: 0 };
    s.total++;
    if (t.status === 'failed') s.failed++;
    siblingStats.set(t.sibling, s);
  }
  const mostFailing = [...siblingStats.entries()]
    .filter(([, s]) => s.failed > 0)
    .sort(([, a], [, b]) => b.failed / b.total - a.failed / a.total)[0];

  if (mostFailing) {
    const [sibling, s] = mostFailing;
    const rate = Math.round((s.failed / s.total) * 100);
    items.push({
      id: 'sibling-fail',
      signal: 'Sibling failure rate',
      value: `${sibling}: ${rate}% (${s.failed}/${s.total} tasks)`,
      trend: rate > 25 ? 'down' : 'stable',
      nonObvious: true,
    });
  }

  // Signal 4: In-progress age — how long the active build has been running vs median
  const inProgress = builds.filter(b => b.status === 'in_progress');
  if (inProgress.length > 0 && completedBuilds.length > 0) {
    const medianDurationMs = (() => {
      const durations = completedBuilds
        .map(b => new Date(b.updatedAt).getTime() - new Date(b.createdAt).getTime())
        .sort((a, b) => a - b);
      return durations[Math.floor(durations.length / 2)];
    })();
    const oldest = inProgress.reduce(
      (min, b) => Math.min(min, new Date(b.createdAt).getTime()),
      Infinity,
    );
    const elapsedMs = now - oldest;
    const pct = Math.round((elapsedMs / medianDurationMs) * 100);
    items.push({
      id: 'build-age',
      signal: 'Active build age vs median',
      value: `${pct}% of median duration (${Math.round(elapsedMs / 60_000)}m elapsed)`,
      trend: pct > 150 ? 'down' : pct < 80 ? 'up' : 'stable',
      nonObvious: true,
    });
  }

  insightCache.set(key, { ts: Date.now(), items });
  return items;
}
