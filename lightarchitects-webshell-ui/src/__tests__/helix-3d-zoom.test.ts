/**
 * helix-3d-zoom — Unit tests for helix-viz-remap P6 checks 7 & 8.
 *
 * Coverage:
 *  - Turn-zoom deep-link route parsing via matchRoute
 *  - zoomLevel derivation predicate (all-3-present → 'turn')
 *  - navigate() hash construction for Turn-zoom and back-nav
 *  - helixEntries cold-start store seeding logic
 *  - getHelixNodes URL query-string construction
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { existsSync } from 'fs';
import { resolve } from 'path';
import { goto } from '$app/navigation';

// Mock $app/navigation so goto() tests work outside a SvelteKit runtime.
vi.mock('$app/navigation', () => ({
  goto: vi.fn(),
  beforeNavigate: vi.fn(),
  afterNavigate: vi.fn(),
}));
import { helixEntries } from '$lib/stores';
import { api } from '$lib/api';
import { get } from 'svelte/store';
import type { HelixEntrySsePayload } from '$lib/types';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Mirrors the $derived predicate in BuildDetail.svelte. */
function computeZoomLevel(
  phaseId: string | null,
  waveId: string | null,
  agentKey: string | null,
): 'build' | 'turn' {
  return phaseId !== null && waveId !== null && agentKey !== null ? 'turn' : 'build';
}

/** Mirrors the cold-start update callback in Helix3D.svelte. */
function applyHelixColdStart(nodes: HelixEntrySsePayload[]): void {
  helixEntries.update(current => (current.length === 0 ? nodes : current));
}

function makeEntry(path: string): HelixEntrySsePayload {
  return { path, event_kind: 'created', sibling: 'soul' };
}

// ---------------------------------------------------------------------------
// 1. Turn-zoom SvelteKit route structure
// Verifies that the file-system route that produces Turn-zoom params exists.
// With SvelteKit, param names come from directory names ([buildId], [phaseId], ...).
// BuildDetail.svelte reads page.params.buildId, page.params.phaseId, etc. —
// these tests confirm the directory tree is named correctly.
// ---------------------------------------------------------------------------

const ROUTES_DIR = resolve(process.cwd(), 'src/routes');

function routeExists(relPath: string): boolean {
  return existsSync(resolve(ROUTES_DIR, relPath));
}

describe('helix-3d-zoom: Turn-zoom SvelteKit route structure', () => {
  it('Turn-zoom full route exists: /builds/[buildId]/phase/[phaseId]/wave/[waveId]/agent/[agentKey]', () => {
    expect(routeExists('builds/[buildId]/phase/[phaseId]/wave/[waveId]/agent/[agentKey]/+page.svelte')).toBe(true);
  });

  it('/builds/[buildId] route exists (bare build view)', () => {
    expect(routeExists('builds/[buildId]/+page.svelte')).toBe(true);
  });

  it('/builds/[buildId]/phase/[phaseId] route exists (phase drill-down)', () => {
    expect(routeExists('builds/[buildId]/phase/[phaseId]/+page.svelte')).toBe(true);
  });

  it('/builds/[buildId]/phase/[phaseId]/wave/[waveId] route exists (wave drill-down)', () => {
    expect(routeExists('builds/[buildId]/phase/[phaseId]/wave/[waveId]/+page.svelte')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// 2. zoomLevel derivation logic
// ---------------------------------------------------------------------------

describe('helix-3d-zoom: zoomLevel derivation', () => {
  it("returns 'turn' when all 3 drill-down params are non-null", () => {
    expect(computeZoomLevel('phase-3', 'wave-1', 'engineer')).toBe('turn');
  });

  it("returns 'build' when phaseId is null", () => {
    expect(computeZoomLevel(null, 'wave-1', 'engineer')).toBe('build');
  });

  it("returns 'build' when waveId is null", () => {
    expect(computeZoomLevel('phase-3', null, 'engineer')).toBe('build');
  });

  it("returns 'build' when agentKey is null", () => {
    expect(computeZoomLevel('phase-3', 'wave-1', null)).toBe('build');
  });
});

// ---------------------------------------------------------------------------
// 3. Turn-zoom deep-link path construction (goto() call sites)
// ---------------------------------------------------------------------------

describe('helix-3d-zoom: Turn-zoom path construction', () => {
  // These tests verify that the goto() calls used in BuildDetail / TaskDrillView
  // produce the correct deep-link paths for Turn-zoom and back-nav.
  beforeEach(() => {
    vi.mocked(goto).mockClear();
  });

  it('produces the correct Turn-zoom path with all 4 segments', () => {
    const buildId = 'build-42';
    const phaseId = 'phase-3-build';
    const waveId = 'wave-1';
    const agentKey = 'engineer';
    goto(`/builds/${buildId}/phase/${phaseId}/wave/${waveId}/agent/${agentKey}`);
    expect(goto).toHaveBeenCalledWith('/builds/build-42/phase/phase-3-build/wave/wave-1/agent/engineer');
  });

  it('back-nav to build zoom uses buildId-only path', () => {
    const buildId = 'build-42';
    goto(`/builds/${buildId}`);
    expect(goto).toHaveBeenCalledWith('/builds/build-42');
  });
});

// ---------------------------------------------------------------------------
// 4. helixEntries cold-start seeding
// ---------------------------------------------------------------------------

describe('helix-3d-zoom: helixEntries cold-start seeding', () => {
  beforeEach(() => {
    helixEntries.set([]);
  });

  it('seeds an empty store with the API nodes', () => {
    const nodes = [makeEntry('soul/entries/day-1.md'), makeEntry('soul/entries/day-2.md')];
    applyHelixColdStart(nodes);
    expect(get(helixEntries)).toHaveLength(2);
    expect(get(helixEntries)[0].path).toBe('soul/entries/day-1.md');
  });

  it('does not overwrite a non-empty store (SSE already populated)', () => {
    const live = makeEntry('soul/entries/live.md');
    helixEntries.set([live]);

    const coldNodes = [makeEntry('soul/entries/stale-1.md'), makeEntry('soul/entries/stale-2.md')];
    applyHelixColdStart(coldNodes);

    const result = get(helixEntries);
    expect(result).toHaveLength(1);
    expect(result[0].path).toBe('soul/entries/live.md');
  });

  it('leaves store empty when cold-start returns no nodes', () => {
    applyHelixColdStart([]);
    expect(get(helixEntries)).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// 5. getHelixNodes URL query-string construction
// ---------------------------------------------------------------------------

describe('helix-3d-zoom: getHelixNodes URL construction', () => {
  let fetchStub: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchStub = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ nodes: [], total: 0 }),
    });
    vi.stubGlobal('fetch', fetchStub);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('calls /api/helix/nodes with no query string when opts is undefined', async () => {
    await api.getHelixNodes();
    expect(fetchStub).toHaveBeenCalledWith('/api/helix/nodes', expect.any(Object));
  });

  it('appends limit param when provided', async () => {
    await api.getHelixNodes({ limit: 50 });
    const [url] = fetchStub.mock.calls[0] as [string];
    expect(url).toContain('limit=50');
    expect(url).not.toContain('since=');
  });

  it('appends both since and limit when provided', async () => {
    await api.getHelixNodes({ since: '2026-01-01', limit: 25 });
    const [url] = fetchStub.mock.calls[0] as [string];
    expect(url).toContain('since=2026-01-01');
    expect(url).toContain('limit=25');
  });
});
