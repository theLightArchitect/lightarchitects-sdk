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
import { matchRoute, navigate } from '$lib/routes';
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
// 1. Turn-zoom deep-link route parsing
// ---------------------------------------------------------------------------

describe('helix-3d-zoom: Turn-zoom deep-link route parsing', () => {
  it('extracts all 4 params from full Turn-zoom URL', () => {
    const r = matchRoute('/builds/b1/phase/p2/wave/w3/agent/engineer');
    expect(r.screen).toBe('BuildDetail');
    expect(r.params.buildId).toBe('b1');
    expect(r.params.phaseId).toBe('p2');
    expect(r.params.waveId).toBe('w3');
    expect(r.params.agentKey).toBe('engineer');
  });

  it('returns BuildDetail without drill-down params when only buildId present', () => {
    const r = matchRoute('/builds/abc-123');
    expect(r.screen).toBe('BuildDetail');
    expect(r.params.buildId).toBe('abc-123');
    expect(r.params.phaseId).toBeUndefined();
    expect(r.params.waveId).toBeUndefined();
    expect(r.params.agentKey).toBeUndefined();
  });

  it('returns BuildDetail with phaseId only (waveId + agentKey absent)', () => {
    const r = matchRoute('/builds/b1/phase/phase-3-build');
    expect(r.screen).toBe('BuildDetail');
    expect(r.params.phaseId).toBe('phase-3-build');
    expect(r.params.waveId).toBeUndefined();
    expect(r.params.agentKey).toBeUndefined();
  });

  it('accepts hyphenated agent keys', () => {
    const r = matchRoute('/builds/b1/phase/p2/wave/w3/agent/code-architect');
    expect(r.params.agentKey).toBe('code-architect');
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
// 3. navigate() hash construction
// ---------------------------------------------------------------------------

describe('helix-3d-zoom: navigate hash construction', () => {
  let locationStub: { hash: string };

  beforeEach(() => {
    locationStub = { hash: '' };
    vi.stubGlobal('location', locationStub);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('produces the correct Turn-zoom hash with all 4 segments', () => {
    navigate('/builds/:buildId/phase/:phaseId/wave/:waveId/agent/:agentKey', {
      buildId: 'build-42',
      phaseId: 'phase-3-build',
      waveId: 'wave-1',
      agentKey: 'engineer',
    });
    expect(locationStub.hash).toBe('/builds/build-42/phase/phase-3-build/wave/wave-1/agent/engineer');
  });

  it('back-nav to build zoom sets hash to buildId only', () => {
    navigate('/builds/:buildId', { buildId: 'build-42' });
    expect(locationStub.hash).toBe('/builds/build-42');
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
