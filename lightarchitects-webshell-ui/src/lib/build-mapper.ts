// ============================================================================
// build-mapper.ts — Maps active.yaml portfolio entries to Build interface
// ============================================================================

import type { Build, BuildStatus, Pillar, PillarGate, PillarStatus, Priority, ProjectGroup } from './types';
import { PILLARS } from './types';

/**
 * Maps a raw portfolio entry status string to the BuildStatus union.
 */
function mapStatus(raw: string | undefined): BuildStatus {
  switch (raw) {
    case 'production':
    case 'complete':
      return 'completed';
    case 'planned':
      return 'queued';
    case 'in_progress':
      return 'in_progress';
    case 'active':
    case 'experimental':
      return 'in_progress';
    case 'prototype':
    case 'archived':
      return 'paused';
    default:
      return 'queued';
  }
}

/**
 * Synthesize 7 PillarGate objects from phase_detail (if available).
 * If entry has phase_detail array, distributes phases across pillars and
 * calculates confidence from completion ratio.
 */
function synthesizePillars(entry: Record<string, unknown>): PillarGate[] {
  const phaseDetail = entry.phase_detail as Array<{ status?: string }> | undefined;

  if (!phaseDetail || !Array.isArray(phaseDetail) || phaseDetail.length === 0) {
    return PILLARS.map(pillar => ({
      pillar,
      status: 'pending' as PillarStatus,
      confidence: 0,
      findings: [],
    }));
  }

  const total = phaseDetail.length;
  const completed = phaseDetail.filter(
    p => p.status === 'complete' || p.status === 'passed'
  ).length;
  const ratio = total > 0 ? completed / total : 0;

  return PILLARS.map((pillar, idx) => {
    // Assign phases to pillars round-robin
    const assignedPhases = phaseDetail.filter((_, i) => i % 7 === idx);
    const pillarComplete = assignedPhases.filter(
      p => p.status === 'complete' || p.status === 'passed'
    ).length;
    const pillarTotal = assignedPhases.length;
    const pillarConfidence = pillarTotal > 0 ? pillarComplete / pillarTotal : ratio;

    let status: PillarStatus = 'pending';
    if (pillarConfidence >= 1) {
      status = 'passed';
    } else if (pillarConfidence > 0) {
      status = 'in_progress';
    }

    return {
      pillar,
      status,
      confidence: pillarConfidence,
      findings: [],
    };
  });
}

/**
 * Resolve the current active pillar from the entry state.
 */
function resolveCurrentPillar(entry: Record<string, unknown>): Pillar {
  const phaseDetail = entry.phase_detail as Array<{ status?: string }> | undefined;

  if (!phaseDetail || !Array.isArray(phaseDetail) || phaseDetail.length === 0) {
    return 'ARCH';
  }

  // Find first non-complete phase, map its index to a pillar
  const firstIncomplete = phaseDetail.findIndex(
    p => p.status !== 'complete' && p.status !== 'passed'
  );
  if (firstIncomplete === -1) return 'OPS'; // all complete
  const pillarIdx = firstIncomplete % 7;
  return PILLARS[pillarIdx];
}

/**
 * Calculate overall build confidence (0-1) from phase progress.
 */
function calcConfidence(entry: Record<string, unknown>): number {
  const currentPhase = entry.current_phase as number | undefined;
  const phases = entry.phases as number | undefined;

  if (typeof currentPhase === 'number' && typeof phases === 'number' && phases > 0) {
    return currentPhase / phases;
  }
  return 0;
}

/**
 * Maps a single portfolio entry (from active.yaml / GET /api/builds)
 * to the frontend Build interface.
 */
export function mapPortfolioToBuild(entry: Record<string, unknown>): Build {
  const id = (entry.codename as string) || (entry.name as string) || 'unknown';
  const now = new Date().toISOString();

  // Extract extended portfolio fields
  const rawPriority = entry.priority as string | undefined;
  const priority: Priority | undefined =
    rawPriority === 'high' || rawPriority === 'medium' || rawPriority === 'low'
      ? rawPriority
      : undefined;

  const rawSiblings = entry.siblings as string[] | undefined;
  const siblings = Array.isArray(rawSiblings) ? rawSiblings : undefined;

  const rawBlocked = entry.blocked_by as string[] | string | undefined;
  const blockedBy = Array.isArray(rawBlocked)
    ? rawBlocked
    : typeof rawBlocked === 'string'
      ? [rawBlocked]
      : undefined;

  return {
    id,
    workspaceId: 'ws-portfolio',
    name: (entry.name as string) || id,
    metaSkill: ((entry.meta_skill as string) || '/BUILD') as Build['metaSkill'],
    status: mapStatus(entry.status as string | undefined),
    pillars: synthesizePillars(entry),
    currentPillar: resolveCurrentPillar(entry),
    confidence: calcConfidence(entry),
    createdAt: (entry.created_date as string) || now,
    updatedAt: now,
    modules: [],
    siblingDispatches: [],
    description: (entry.description as string) || undefined,
    priority,
    siblings,
    blockedBy,
    blocks: Array.isArray(entry.blocks) ? entry.blocks as string[] : undefined,
    path: (entry.path as string) || undefined,
    tier: typeof entry.tier === 'number' ? entry.tier : undefined,
    agent: entry.agent
      ? { kind: (entry.agent as Record<string, unknown>).kind as string, backend: ((entry.agent as Record<string, unknown>).backend as string) || undefined }
      : undefined,
  };
}

/**
 * Maps the full API response (either {builds: [...]} or bare [...])
 * to an array of Build objects.
 */
export function mapPortfolioBuilds(response: unknown): Build[] {
  let entries: unknown[];

  if (Array.isArray(response)) {
    entries = response;
  } else if (
    response !== null &&
    typeof response === 'object' &&
    'builds' in response &&
    Array.isArray((response as Record<string, unknown>).builds)
  ) {
    entries = (response as Record<string, unknown>).builds as unknown[];
  } else {
    return [];
  }

  return entries.map(e => mapPortfolioToBuild(e as Record<string, unknown>));
}

// ─── Project Grouping ─────────────────────────────────────────────────────────

/**
 * Normalize a project path to a grouping key.
 * Strips ~/ prefix, trailing slash, and takes first 3 segments as the project root.
 */
function normalizeProjectPath(path: string): string {
  const cleaned = path.replace(/^~\//, '').replace(/\/$/, '');
  const parts = cleaned.split('/');
  return parts.slice(0, Math.min(parts.length, 3)).join('/');
}

/**
 * Convert a path to a URL-safe ID (replace / with -).
 */
function pathToId(path: string): string {
  return path.replace(/\//g, '-');
}

/**
 * Get a display name from a project path (last meaningful segment).
 */
function pathToName(path: string): string {
  const parts = path.split('/').filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/**
 * Group builds by project path, creating a two-level hierarchy.
 * Projects (tier 1-2 or builds without phase_detail) are the top level.
 * Build plans (tier 3+ with phase_detail or blockedBy/blocks) are children.
 */
export function groupByProject(builds: Build[]): ProjectGroup[] {
  const groups = new Map<string, { path: string; builds: Build[] }>();

  for (const build of builds) {
    const rawPath = build.path ?? build.name;
    const key = normalizeProjectPath(rawPath);

    if (!groups.has(key)) {
      groups.set(key, { path: key, builds: [] });
    }
    groups.get(key)!.builds.push(build);
  }

  const result: ProjectGroup[] = [];

  for (const [key, group] of groups) {
    // Separate project-level entries from build plans
    const project = group.builds.find(b =>
      (b.tier !== undefined && b.tier <= 2) ||
      (b.status === 'completed' && !b.blockedBy?.length)
    );

    // Plans are everything that has phase_detail indicators or is explicitly a plan
    const plans = group.builds.filter(b => b !== project);

    const completed = plans.filter(b => b.status === 'completed').length;
    const total = plans.length || 1;

    result.push({
      id: pathToId(key),
      name: project?.name ?? pathToName(key),
      path: group.path,
      project,
      plans: plans.length > 0 ? plans : group.builds,
      planCount: plans.length || group.builds.length,
      activePlanCount: plans.filter(b => b.status === 'in_progress').length,
      progress: total > 0 ? completed / total : 0,
    });
  }

  // Sort: most active plans first, then by tier
  result.sort((a, b) => {
    if (a.activePlanCount !== b.activePlanCount) return b.activePlanCount - a.activePlanCount;
    if (a.planCount !== b.planCount) return b.planCount - a.planCount;
    return (a.project?.tier ?? 5) - (b.project?.tier ?? 5);
  });

  return result;
}
