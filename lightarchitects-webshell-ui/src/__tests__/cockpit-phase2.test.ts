import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { score, rank } from '$lib/cockpit/fuzzyMatch';
import { selectedTarget, quickPickOpen, type CockpitTarget } from '$lib/cockpit/stores';
import { builds } from '$lib/stores';
import { getBuildList } from '$lib/cockpit/localTargets';

// ── fuzzyMatch ─────────────────────────────────────────────────────────────

describe('fuzzyMatch', () => {
  describe('score()', () => {
    it('exact match returns 1.0', () => {
      expect(score('foo', 'foo')).toBe(1.0);
    });

    it('prefix match returns > 0.85', () => {
      expect(score('foo', 'foobar')).toBeGreaterThan(0.85);
    });

    it('substring match returns > 0.7', () => {
      expect(score('bar', 'foobar')).toBeGreaterThan(0.7);
    });

    it('scattered match returns > 0 and < 0.7', () => {
      const s = score('fb', 'foobar');
      expect(s).toBeGreaterThan(0);
      expect(s).toBeLessThan(0.7);
    });

    it('no match returns 0', () => {
      expect(score('xyz', 'foobar')).toBe(0);
    });

    it('empty query returns 1.0', () => {
      expect(score('', 'anything')).toBe(1.0);
    });

    it('case-insensitive', () => {
      expect(score('FOO', 'foobar')).toBeGreaterThan(0.8);
    });
  });

  describe('rank()', () => {
    const items = [
      { label: 'src/lib/auth.ts' },
      { label: 'src/lib/api.ts' },
      { label: 'src/components/Cockpit/PresetChips.svelte' },
      { label: 'package.json' },
    ];

    it('returns all items for empty query', () => {
      expect(rank('', items, i => i.label)).toHaveLength(items.length);
    });

    it('filters out non-matching items', () => {
      const result = rank('preset', items, i => i.label);
      expect(result).toHaveLength(1);
      expect(result[0].label).toContain('PresetChips');
    });

    it('exact prefix ranked first', () => {
      const result = rank('auth', items, i => i.label);
      expect(result[0].label).toContain('auth');
    });

    it('returns empty array when nothing matches', () => {
      expect(rank('zzzzzz', items, i => i.label)).toHaveLength(0);
    });
  });
});

// ── localTargets — getBuildList ────────────────────────────────────────────

describe('getBuildList()', () => {
  beforeEach(() => {
    builds.set([
      {
        id: 'b1', workspaceId: 'ws', name: 'Auth flow', metaSkill: '/BUILD',
        status: 'in_progress', pillars: [], currentPillar: 'ARCH', confidence: 0.7,
        createdAt: '', updatedAt: '', modules: [], siblingDispatches: [],
        codename: 'auth-flow',
      },
      {
        id: 'b2', workspaceId: 'ws', name: 'Helix core', metaSkill: '/BUILD',
        status: 'completed', pillars: [], currentPillar: 'QUAL', confidence: 0.95,
        createdAt: '', updatedAt: '', modules: [], siblingDispatches: [],
      },
    ]);
  });

  it('returns a CockpitTarget for each build', () => {
    const list = getBuildList();
    expect(list).toHaveLength(2);
  });

  it('uses codename as id when present', () => {
    const list = getBuildList();
    const first = list.find(t => t.label === 'Auth flow');
    expect(first?.id).toBe('auth-flow');
  });

  it('falls back to build id when codename absent', () => {
    const list = getBuildList();
    const second = list.find(t => t.label === 'Helix core');
    expect(second?.id).toBe('b2');
  });

  it('all results have type "build"', () => {
    getBuildList().forEach(t => expect(t.type).toBe('build'));
  });
});

// ── cockpit stores integration ─────────────────────────────────────────────

describe('cockpit stores', () => {
  it('selectedTarget starts null', () => {
    selectedTarget.set(null);
    expect(get(selectedTarget)).toBeNull();
  });

  it('can set and read a target', () => {
    const t: CockpitTarget = { type: 'build', id: 'b1', label: 'Auth flow' };
    selectedTarget.set(t);
    expect(get(selectedTarget)).toEqual(t);
    selectedTarget.set(null);
  });

  it('quickPickOpen toggles', () => {
    quickPickOpen.set(false);
    expect(get(quickPickOpen)).toBe(false);
    quickPickOpen.set(true);
    expect(get(quickPickOpen)).toBe(true);
    quickPickOpen.set(false);
  });
});

// ── API endpoint shape contracts ───────────────────────────────────────────

describe('localTargets fetch contract', () => {
  it('getFileList maps paths to file CockpitTargets', async () => {
    const { getFileList } = await import('$lib/cockpit/localTargets');
    // Mock fetch to return a path array
    const originalFetch = globalThis.fetch;
    globalThis.fetch = vi.fn().mockResolvedValueOnce({
      ok: true,
      json: async () => ['src/lib/auth.ts', 'src/lib/api.ts'],
    } as Response);

    const result = await getFileList('auth');
    expect(result).toHaveLength(2);
    expect(result[0].type).toBe('file');
    expect(result[0].id).toBe('src/lib/auth.ts');
    globalThis.fetch = originalFetch;
  });

  it('getBranchList maps branch strings to branch CockpitTargets', async () => {
    const { getBranchList } = await import('$lib/cockpit/localTargets');
    const originalFetch = globalThis.fetch;
    globalThis.fetch = vi.fn().mockResolvedValueOnce({
      ok: true,
      json: async () => ({ branches: ['main', 'feat/cockpit-phase2'] }),
    } as Response);

    const result = await getBranchList();
    expect(result).toHaveLength(2);
    expect(result[0].type).toBe('branch');
    expect(result.map(r => r.label)).toContain('main');
    globalThis.fetch = originalFetch;
  });

  it('getFileList returns [] on non-ok response', async () => {
    const { getFileList } = await import('$lib/cockpit/localTargets');
    const originalFetch = globalThis.fetch;
    globalThis.fetch = vi.fn().mockResolvedValueOnce({ ok: false } as Response);
    const result = await getFileList('anything');
    expect(result).toHaveLength(0);
    globalThis.fetch = originalFetch;
  });

  it('getBranchList returns [] on fetch error', async () => {
    const { getBranchList } = await import('$lib/cockpit/localTargets');
    const originalFetch = globalThis.fetch;
    globalThis.fetch = vi.fn().mockRejectedValueOnce(new Error('network'));
    const result = await getBranchList();
    expect(result).toHaveLength(0);
    globalThis.fetch = originalFetch;
  });

  it('getCommitList maps worktree head_sha to commit targets', async () => {
    const { getCommitList } = await import('$lib/cockpit/localTargets');
    const originalFetch = globalThis.fetch;
    globalThis.fetch = vi.fn().mockResolvedValueOnce({
      ok: true,
      json: async () => ([
        { path: '/wt/cockpit', branch: 'feat/cockpit', head_sha: 'abc1234def5', status: 'active', locked: false, created_at: null },
      ]),
    } as Response);

    const result = await getCommitList();
    expect(result).toHaveLength(1);
    expect(result[0].type).toBe('commit');
    expect(result[0].id).toBe('abc1234def5');
    expect(result[0].label).toContain('abc1234d');
    globalThis.fetch = originalFetch;
  });
});
