import { describe, it, expect } from 'vitest';
import {
  computeFadeLevel,
  polytopeClusterFor,
  countActiveWorktrees,
  reconstructTopology,
} from '$lib/gitforest';
import type { BranchNode, GitForestTopology } from '$lib/gitforest';

// ── Helpers ────────────────────────────────────────────────────────────────

function makeNode(overrides: Partial<BranchNode> = {}): BranchNode {
  return {
    id: 'test-node',
    name: 'test',
    kind: 'build',
    parent_id: null,
    depth: 2,
    fork_commit_sha: null,
    fork_position: 0,
    children: [],
    overlay: {
      lifecycle: 'live_active',
      ci_status: 'unknown',
      hitl_state: 'none',
      phase: null,
      gate_score: null,
      merged_at: null,
      merged_to: null,
      age_days: 0,
      model_attribution: [],
      fade_level: 1.0,
    },
    build_progress: null,
    worktrees: [],
    ...overrides,
  };
}

// ── branch overlay merge (computeFadeLevel) ────────────────────────────────

describe('computeFadeLevel', () => {
  it('returns 1.0 for live branches (no merged_at)', () => {
    expect(computeFadeLevel(null)).toBe(1.0);
  });

  it('returns 1.0 for branches merged today', () => {
    const today = new Date().toISOString();
    expect(computeFadeLevel(today)).toBeCloseTo(1.0, 1);
  });

  it('returns 0.5 for branches merged 30 days ago', () => {
    const thirtyDaysAgo = new Date(Date.now() - 30 * 86_400_000).toISOString();
    expect(computeFadeLevel(thirtyDaysAgo)).toBeCloseTo(0.5, 1);
  });

  it('enforces floor of 0.20 for branches merged >60 days ago', () => {
    const ancient = new Date(Date.now() - 90 * 86_400_000).toISOString();
    expect(computeFadeLevel(ancient)).toBe(0.2);
  });

  it('does not exceed 1.0 for future merged_at (clock skew guard)', () => {
    const future = new Date(Date.now() + 86_400_000).toISOString();
    expect(computeFadeLevel(future)).toBeLessThanOrEqual(1.0);
  });
});

// ── stale-fade logic (lifecycle: merged → opacity decay) ──────────────────

describe('persistent-merged branch opacity', () => {
  it('30-day merged branch has 50% opacity (floor floor not hit)', () => {
    const mergedAt = new Date(Date.now() - 30 * 86_400_000).toISOString();
    const fade = computeFadeLevel(mergedAt);
    expect(fade).toBeGreaterThan(0.2);
    expect(fade).toBeLessThan(1.0);
  });

  it('60-day merged branch hits floor (0.20)', () => {
    const mergedAt = new Date(Date.now() - 60 * 86_400_000).toISOString();
    const fade = computeFadeLevel(mergedAt);
    expect(fade).toBeCloseTo(0.2, 2);
  });
});

// ── polytope cluster mapping (polytopeClusterFor) ─────────────────────────

describe('polytopeClusterFor', () => {
  it('returns [] for 0 active agents', () => {
    expect(polytopeClusterFor(0)).toEqual([]);
  });

  it('returns pentachoron only for 1 agent', () => {
    expect(polytopeClusterFor(1)).toEqual(['pentachoron']);
  });

  it('returns pentachoron + tesseract for 2 agents', () => {
    expect(polytopeClusterFor(2)).toEqual(['pentachoron', 'tesseract']);
  });

  it('caps at 4-agent cluster for counts ≥ 5', () => {
    const at4 = polytopeClusterFor(4);
    const at5 = polytopeClusterFor(5);
    const at10 = polytopeClusterFor(10);
    expect(at5).toEqual(at4);
    expect(at10).toEqual(at4);
  });
});

// ── countActiveWorktrees (branch overlay merge + recursion) ───────────────

describe('countActiveWorktrees', () => {
  it('returns 0 for a node with no worktrees', () => {
    const node = makeNode({ id: 'root', children: [] });
    expect(countActiveWorktrees('root', { root: node })).toBe(0);
  });

  it('counts writing + gate states as active', () => {
    const node = makeNode({
      id: 'build-1',
      worktrees: [
        { agent_key: 'w1', domain: 'engineer', state: 'writing', commits: 3, task_id: '', worktree_path: '', position_offset: 0 },
        { agent_key: 'w2', domain: 'quality',  state: 'gate',    commits: 1, task_id: '', worktree_path: '', position_offset: 0 },
        { agent_key: 'w3', domain: 'ops',      state: 'done',    commits: 2, task_id: '', worktree_path: '', position_offset: 0 },
      ],
    });
    expect(countActiveWorktrees('build-1', { 'build-1': node })).toBe(2);
  });

  it('accumulates active counts recursively from children', () => {
    const parent = makeNode({ id: 'p', children: ['c1', 'c2'], worktrees: [] });
    const c1 = makeNode({
      id: 'c1', parent_id: 'p', depth: 3,
      worktrees: [{ agent_key: 'w1', domain: 'engineer', state: 'writing', commits: 1, task_id: '', worktree_path: '', position_offset: 0 }],
    });
    const c2 = makeNode({
      id: 'c2', parent_id: 'p', depth: 3,
      worktrees: [{ agent_key: 'w2', domain: 'quality', state: 'done', commits: 1, task_id: '', worktree_path: '', position_offset: 0 }],
    });
    const nodes = { p: parent, c1, c2 };
    expect(countActiveWorktrees('p', nodes)).toBe(1);  // only w1 is active
  });
});

// ── ghost-branch positioning (reconstructTopology) ────────────────────────

describe('reconstructTopology', () => {
  it('builds a valid topology from a single root node', () => {
    const root = makeNode({ id: 'main', depth: 0, kind: 'main' });
    const topology: GitForestTopology = reconstructTopology('lightarchitects-sdk', root);
    expect(topology.repo).toBe('lightarchitects-sdk');
    expect(topology.root_id).toBe('main');
    expect(topology.nodes['main']).toBeDefined();
    expect(topology.fetched_at).toBeDefined();
  });

  it('Phase 2 scaffold: only root present in nodes (children referenced by id but not hydrated)', () => {
    const root = makeNode({ id: 'main', children: ['prog-1', 'prog-2'] });
    const topology = reconstructTopology('sdk', root);
    expect(Object.keys(topology.nodes)).toHaveLength(1);
    expect(topology.nodes['prog-1']).toBeUndefined();
  });
});
