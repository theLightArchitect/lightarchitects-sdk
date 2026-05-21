// Local target source adapters for QuickPickPalette.
// All calls use existing /api endpoints — no new backend routes in Phase 2.

import { get } from 'svelte/store';
import { builds } from '$lib/stores';
import { authHeaders } from '$lib/auth';
import type { CockpitTarget } from '$lib/cockpit/stores';
import type { WorktreeMeta } from '$lib/types';

// ── Builds ───────────────────────────────────────────────────────────────────

/**
 * Returns build targets from the already-populated `builds` store.
 * O(n) read — no fetch needed.
 */
export function getBuildList(): CockpitTarget[] {
  return get(builds).map(b => ({
    type: 'build' as const,
    id: b.codename ?? b.id,
    label: b.name || b.codename || b.id,
  }));
}

// ── Phases ───────────────────────────────────────────────────────────────────

interface BuildDetail {
  current_phase?: number;
  total_phases?: number;
  phase_status_history?: Record<string, string> | null;
  codename?: string;
}

/**
 * Fetches phase targets for a specific build codename.
 * Falls back to numeric labels if `phase_status_history` is unavailable.
 */
export async function getPhaseList(codename: string): Promise<CockpitTarget[]> {
  try {
    const res = await fetch(`/api/builds?codename=${encodeURIComponent(codename)}`, {
      headers: authHeaders(),
    });
    if (!res.ok) return [];
    const items: BuildDetail[] = await res.json();
    const detail = items[0];
    if (!detail) return [];

    const total = detail.total_phases ?? 0;
    const history = detail.phase_status_history;

    if (history && typeof history === 'object') {
      return Object.keys(history).map(key => ({
        type: 'phase' as const,
        id: `${codename}/${key}`,
        label: key,
      }));
    }
    // Fallback: generate phase-N labels
    return Array.from({ length: total }, (_, i) => ({
      type: 'phase' as const,
      id: `${codename}/phase-${i}`,
      label: `phase-${i}`,
    }));
  } catch {
    return [];
  }
}

// ── Files ────────────────────────────────────────────────────────────────────

/**
 * Searches files via `GET /api/files?q=<query>`.
 * Server-side BFS walk — depth 5, max 50 results, skips hidden/build dirs.
 */
export async function getFileList(query: string): Promise<CockpitTarget[]> {
  try {
    const q = query ? `?q=${encodeURIComponent(query)}` : '';
    const res = await fetch(`/api/files${q}`, { headers: authHeaders() });
    if (!res.ok) return [];
    const paths: string[] = await res.json();
    return paths.map(p => ({
      type: 'file' as const,
      id: p,
      label: p,
    }));
  } catch {
    return [];
  }
}

// ── Branches ─────────────────────────────────────────────────────────────────

/**
 * Lists git branches via `POST /api/git/branch {op: "list", cwd: "."}`.
 */
export async function getBranchList(): Promise<CockpitTarget[]> {
  try {
    const res = await fetch('/api/git/branch', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify({ op: 'list', cwd: '.' }),
    });
    if (!res.ok) return [];
    const data: { branches?: string[] } = await res.json();
    return (data.branches ?? []).map(b => ({
      type: 'branch' as const,
      id: b,
      label: b,
    }));
  } catch {
    return [];
  }
}

// ── Commits (worktree HEADs) ──────────────────────────────────────────────────

/**
 * Returns commit targets from active worktree HEAD SHAs.
 * Uses `POST /api/git/worktrees` — covers active development context.
 */
export async function getCommitList(): Promise<CockpitTarget[]> {
  try {
    const res = await fetch('/api/git/worktrees', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify({ cwd: '.' }),
    });
    if (!res.ok) return [];
    const wts: WorktreeMeta[] = await res.json();
    return wts
      .filter(w => w.head_sha && w.head_sha !== '0000000000000000000000000000000000000000')
      .map(w => ({
        type: 'commit' as const,
        id: w.head_sha,
        label: `${w.head_sha.slice(0, 8)} (${w.branch || 'detached'})`,
      }));
  } catch {
    return [];
  }
}

// ── GitHub PRs (HITL inbox) ───────────────────────────────────────────────────

interface HitlSearchItem {
  number: number;
  title: string;
  html_url: string;
  owner: string;
  repo: string;
  author: string;
  updated_at: string;
  draft: boolean;
}

/**
 * Returns open PRs awaiting review from the HITL inbox (`GET /api/gitforest/hitl-search`).
 * Falls back gracefully to an empty list when the GitHub PAT is not configured.
 */
export async function getPRTargets(): Promise<CockpitTarget[]> {
  try {
    const res = await fetch('/api/gitforest/hitl-search', { headers: authHeaders() });
    if (!res.ok) return [];
    const items: HitlSearchItem[] = await res.json();
    return items.map(pr => ({
      type: 'pr' as const,
      id: pr.html_url,
      label: `#${pr.number} ${pr.title} (${pr.repo})`,
    }));
  } catch {
    return [];
  }
}

// ── Composite query ───────────────────────────────────────────────────────────

/** All local target types, ordered by relevance for general search. */
export type LocalSourceKey = 'build' | 'phase' | 'file' | 'branch' | 'commit' | 'pr';

/**
 * Fetch all local sources in parallel and merge into a single list.
 * `fileQuery` is passed to the server-side file search.
 * `buildCodename` scopes phase results to a specific build (optional).
 */
export async function getAllLocalTargets(
  fileQuery: string,
  buildCodename?: string,
): Promise<CockpitTarget[]> {
  const [files, branches, commits] = await Promise.all([
    getFileList(fileQuery),
    getBranchList(),
    getCommitList(),
  ]);
  const phases = buildCodename ? await getPhaseList(buildCodename) : [];
  const buildTargets = getBuildList();
  return [...buildTargets, ...phases, ...files, ...branches, ...commits];
}
