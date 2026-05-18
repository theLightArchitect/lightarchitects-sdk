/**
 * Phase 7 unit tests — gitforest-live-ops deliverables.
 *
 * Covers:
 *   1. WavePipelineView.contract.ts — CONTEXT_TIER_DEFAULTS shape + GateLabel invariants
 *   2. BranchTooltip.svelte — component import + position math
 *   3. StatsTopbar — derived counter logic (replicated from component)
 *   4. SharedSlotBar — store-driven assignment derivation
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import {
  CONTEXT_TIER_DEFAULTS,
  type GateLabel,
  type GateVerdictSummary,
  type Phase,
  type Wave,
  type WaveTask,
} from '$lib/WavePipelineView.contract';
import { builds, activityFeed, gitforestTree, slotAssignments } from '$lib/stores';
import type { Build, ActivityEntry } from '$lib/types';

// ── Helpers ────────────────────────────────────────────────────────────────

const NOW = new Date().toISOString();

function makeBuild(overrides: Partial<Build> = {}): Build {
  return {
    id: 'build-001',
    workspaceId: 'ws-001',
    name: 'Test Build',
    metaSkill: '/BUILD',
    status: 'in_progress',
    pillars: [],
    currentPillar: 'ARCH',
    confidence: 0,
    createdAt: NOW,
    updatedAt: NOW,
    modules: [],
    siblingDispatches: [],
    ...overrides,
  };
}

function makeCopilotEntry(buildId: string, kind: string, age_ms = 0): ActivityEntry {
  return {
    source: 'copilot',
    event: {
      build_id: buildId,
      kind,
      raw: null,
      timestamp: new Date(Date.now() - age_ms).toISOString(),
    },
  };
}

// ── §1 WavePipelineView.contract.ts ───────────────────────────────────────

describe('CONTEXT_TIER_DEFAULTS', () => {
  it('has entries for T1, T2, T3', () => {
    expect(CONTEXT_TIER_DEFAULTS).toHaveProperty('T1');
    expect(CONTEXT_TIER_DEFAULTS).toHaveProperty('T2');
    expect(CONTEXT_TIER_DEFAULTS).toHaveProperty('T3');
  });

  it('T1 has the highest token_count', () => {
    expect(CONTEXT_TIER_DEFAULTS.T1.token_count).toBeGreaterThan(CONTEXT_TIER_DEFAULTS.T2.token_count);
    expect(CONTEXT_TIER_DEFAULTS.T2.token_count).toBeGreaterThan(CONTEXT_TIER_DEFAULTS.T3.token_count);
  });

  it('each tier has the correct icon glyph', () => {
    expect(CONTEXT_TIER_DEFAULTS.T1.icon).toBe('◈');
    expect(CONTEXT_TIER_DEFAULTS.T2.icon).toBe('◇');
    expect(CONTEXT_TIER_DEFAULTS.T3.icon).toBe('○');
  });

  it('tier fields match tier key', () => {
    expect(CONTEXT_TIER_DEFAULTS.T1.tier).toBe('T1');
    expect(CONTEXT_TIER_DEFAULTS.T2.tier).toBe('T2');
    expect(CONTEXT_TIER_DEFAULTS.T3.tier).toBe('T3');
  });

  it('labels are non-empty strings', () => {
    for (const [, v] of Object.entries(CONTEXT_TIER_DEFAULTS)) {
      expect(typeof v.label).toBe('string');
      expect(v.label.length).toBeGreaterThan(0);
    }
  });

  it('T1 token_count is 200_000 (full context budget)', () => {
    expect(CONTEXT_TIER_DEFAULTS.T1.token_count).toBe(200_000);
  });
});

describe('GateVerdictSummary shape', () => {
  it('overall values are the three gate states', () => {
    const valids: GateVerdictSummary['overall'][] = ['pass', 'hitl', 'fail'];
    // Just validate the type structure by constructing valid values
    for (const v of valids) {
      const verdict: GateVerdictSummary = { overall: v, results: [], evaluated_at: NOW };
      expect(verdict.overall).toBe(v);
    }
  });

  it('GateLabel covers all 10 LASDLC dimensions', () => {
    const expected: GateLabel[] = ['A', 'S', 'Q', 'C', 'O', 'P', 'K', 'D', 'T', 'R'];
    for (const label of expected) {
      const result = { label, passed: true, score: 1.0, blocker: null };
      expect(result.label).toBe(label);
    }
  });
});

describe('Phase / Wave / WaveTask schema consistency', () => {
  const STATUSES = ['pending', 'in_progress', 'completed', 'failed'] as const;

  it('all status values are the same union for Phase, Wave, WaveTask', () => {
    for (const s of STATUSES) {
      const phase: Phase = { id: 'p1', label: 'Phase 1', status: s, waves: [], gate_verdict: null };
      const wave: Wave  = { id: 'w1', label: 'Wave 1',  status: s, tasks: [], gate_verdict: null };
      const task: WaveTask = { id: 't1', title: 'Task', status: s, agent_key: null, started_at: null, completed_at: null };
      expect(phase.status).toBe(s);
      expect(wave.status).toBe(s);
      expect(task.status).toBe(s);
    }
  });

  it('Phase can nest Waves which nest WaveTasks', () => {
    const task: WaveTask = { id: 't1', title: 'Write module', status: 'completed', agent_key: 'engineer', started_at: NOW, completed_at: NOW };
    const wave: Wave = { id: 'w1', label: 'Wave 1', status: 'completed', tasks: [task], gate_verdict: null };
    const phase: Phase = { id: 'p1', label: 'Phase 1', status: 'in_progress', waves: [wave], gate_verdict: null };
    expect(phase.waves[0].tasks[0].id).toBe('t1');
  });
});

// ── §2 BranchTooltip.svelte ────────────────────────────────────────────────

describe('BranchTooltip', () => {
  it('imports successfully', async () => {
    const mod = await import('$lib/../components/topology/BranchTooltip.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('BranchTooltip position math', () => {
  const VIEWPORT_PAD = 8;
  const CARD_HALF_WIDTH = 120;
  const GAP = 8;
  const MIN_ABOVE_HEIGHT = 180;

  function computeCardLeft(anchor: { left: number; width: number }): number {
    return Math.max(VIEWPORT_PAD, anchor.left + anchor.width / 2 - CARD_HALF_WIDTH);
  }

  function computeFlipBelow(anchor: { top: number }): boolean {
    return anchor.top - GAP <= MIN_ABOVE_HEIGHT;
  }

  it('card left clamps to VIEWPORT_PAD when anchor is near left edge', () => {
    expect(computeCardLeft({ left: 0, width: 10 })).toBe(VIEWPORT_PAD);
  });

  it('card left centers on anchor midpoint for mid-screen anchors', () => {
    const anchor = { left: 400, width: 20 };
    const expected = anchor.left + anchor.width / 2 - CARD_HALF_WIDTH;
    expect(computeCardLeft(anchor)).toBe(expected);
  });

  it('flips below when anchor is within 180px of top (not enough room above)', () => {
    expect(computeFlipBelow({ top: 100 })).toBe(true);
    expect(computeFlipBelow({ top: 50 })).toBe(true);
  });

  it('does not flip when anchor has room above (> 180px from top)', () => {
    expect(computeFlipBelow({ top: 300 })).toBe(false);
    expect(computeFlipBelow({ top: 200 })).toBe(false);
  });

  it('boundary: top === 188 is exactly at threshold (flip = true, 188-8=180 ≤ 180)', () => {
    expect(computeFlipBelow({ top: 188 })).toBe(true);
  });

  it('boundary: top === 189 is just above threshold (flip = false, 189-8=181 > 180)', () => {
    expect(computeFlipBelow({ top: 189 })).toBe(false);
  });
});

// ── §3 WavePipelineView component ─────────────────────────────────────────

describe('WavePipelineView', () => {
  it('imports successfully', async () => {
    const mod = await import('$lib/../components/views/WavePipelineView.svelte');
    expect(mod.default).toBeDefined();
  });
});

// ── §4 StatsTopbar counter logic ──────────────────────────────────────────

describe('StatsTopbar counter logic', () => {
  beforeEach(() => {
    builds.set([]);
    activityFeed.set([]);
    gitforestTree.set(null);
  });

  afterEach(() => {
    builds.set([]);
    activityFeed.set([]);
    gitforestTree.set(null);
  });

  it('StatsTopbar imports successfully', async () => {
    const mod = await import('$lib/../components/StatsTopbar.svelte');
    expect(mod.default).toBeDefined();
  });

  // Replicate the derived counter logic to unit test it independently
  function countActive(bs: Build[]): number {
    return bs.filter(b => b.status === 'in_progress' || b.status === 'queued').length;
  }
  function countHitl(bs: Build[]): number {
    return bs.filter(b => b.status === 'paused').length;
  }
  function countRecentGates(feed: ActivityEntry[]): number {
    return feed.filter(e => {
      if (e.source !== 'copilot') return false;
      const ev = (e as { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent }).event;
      return ev.kind === 'gate' && Date.now() - new Date(ev.timestamp).getTime() < 60_000;
    }).length;
  }
  function countStale(bs: Build[], feed: ActivityEntry[]): number {
    return bs.filter(b => {
      if (b.status !== 'in_progress') return false;
      const lastActivity = feed.findLast(e => {
        if (e.source !== 'copilot') return false;
        const ev = (e as { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent }).event;
        return 'build_id' in ev && (ev as unknown as Record<string, unknown>).build_id === b.id;
      });
      if (!lastActivity) return true;
      const ts = (lastActivity as { source: 'copilot'; event: import('$lib/types').CopilotActivityEvent }).event.timestamp;
      return Date.now() - new Date(ts).getTime() > 10 * 60_000;
    }).length;
  }

  it('activeBuilds counts in_progress + queued', () => {
    const bs = [
      makeBuild({ id: 'a', status: 'in_progress' }),
      makeBuild({ id: 'b', status: 'queued' }),
      makeBuild({ id: 'c', status: 'completed' }),
      makeBuild({ id: 'd', status: 'failed' }),
    ];
    expect(countActive(bs)).toBe(2);
  });

  it('hitlPending counts only paused builds', () => {
    const bs = [
      makeBuild({ id: 'a', status: 'paused' }),
      makeBuild({ id: 'b', status: 'in_progress' }),
      makeBuild({ id: 'c', status: 'paused' }),
    ];
    expect(countHitl(bs)).toBe(2);
  });

  it('recentGates counts only copilot gate events within last 60s', () => {
    const feed: ActivityEntry[] = [
      makeCopilotEntry('b1', 'gate', 30_000),          // 30s ago — counts
      makeCopilotEntry('b1', 'gate', 90_000),          // 90s ago — does NOT count
      makeCopilotEntry('b2', 'assistant', 5_000),      // not a gate event
      makeCopilotEntry('b3', 'gate', 1_000),           // 1s ago — counts
      { source: 'ayin', span: { id: 'x', actor: 'y', action: 'z', timestamp: NOW, duration_ms: 1, outcome: null } },
    ];
    expect(countRecentGates(feed)).toBe(2);
  });

  it('staleBuilds: in_progress with no activity feed entry is stale', () => {
    const bs = [makeBuild({ id: 'b1', status: 'in_progress' })];
    expect(countStale(bs, [])).toBe(1);
  });

  it('staleBuilds: in_progress with recent activity is NOT stale', () => {
    const bs = [makeBuild({ id: 'b1', status: 'in_progress' })];
    const feed: ActivityEntry[] = [makeCopilotEntry('b1', 'assistant', 30_000)]; // 30s ago
    expect(countStale(bs, feed)).toBe(0);
  });

  it('staleBuilds: in_progress with activity > 10 minutes ago IS stale', () => {
    const bs = [makeBuild({ id: 'b1', status: 'in_progress' })];
    const feed: ActivityEntry[] = [makeCopilotEntry('b1', 'assistant', 11 * 60_000)]; // 11 min ago
    expect(countStale(bs, feed)).toBe(1);
  });

  it('staleBuilds uses findLast (latest entry determines staleness)', () => {
    const bs = [makeBuild({ id: 'b1', status: 'in_progress' })];
    const feed: ActivityEntry[] = [
      makeCopilotEntry('b1', 'assistant', 15 * 60_000), // old entry (15min) — ignored
      makeCopilotEntry('b1', 'assistant', 30_000),       // recent entry (30s) — takes precedence
    ];
    expect(countStale(bs, feed)).toBe(0);
  });

  it('non-in_progress builds are never stale', () => {
    const bs = [
      makeBuild({ id: 'a', status: 'completed' }),
      makeBuild({ id: 'b', status: 'paused' }),
      makeBuild({ id: 'c', status: 'failed' }),
      makeBuild({ id: 'd', status: 'queued' }),
    ];
    expect(countStale(bs, [])).toBe(0);
  });
});

// ── §5 SharedSlotBar ──────────────────────────────────────────────────────

describe('SharedSlotBar', () => {
  it('imports successfully', async () => {
    const mod = await import('$lib/../components/SharedSlotBar.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('slotAssignments derived store', () => {
  beforeEach(() => { gitforestTree.set(null); });
  afterEach(() => { gitforestTree.set(null); });

  it('returns empty map when gitforestTree is null', () => {
    const map = get(slotAssignments);
    expect(map.size).toBe(0);
  });

  it('populates from worktrees in topology nodes', () => {
    gitforestTree.set({
      repo: 'test-repo',
      root_id: 'main',
      nodes: {
        main: {
          id: 'main',
          name: 'main',
          kind: 'main',
          parent_id: null,
          depth: 0,
          fork_commit_sha: null,
          fork_position: 0,
          children: ['feat-1'],
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
        },
        'feat-1': {
          id: 'feat-1',
          name: 'feat/test',
          kind: 'build',
          parent_id: 'main',
          depth: 1,
          fork_commit_sha: null,
          fork_position: 0,
          children: [],
          overlay: {
            lifecycle: 'live_active',
            ci_status: 'success',
            hitl_state: 'none',
            phase: 'phase-2',
            gate_score: 0.97,
            merged_at: null,
            merged_to: null,
            age_days: 3,
            model_attribution: ['claude-opus-4-7'],
            fade_level: 1.0,
          },
          build_progress: null,
          worktrees: [
            { agent_key: 'w1', domain: 'engineer', state: 'writing', commits: 5, task_id: 'task-001', worktree_path: '/tmp/wt1', position_offset: 0 },
            { agent_key: 'w2', domain: 'quality',  state: 'gate',    commits: 2, task_id: 'task-002', worktree_path: '/tmp/wt2', position_offset: 1 },
          ],
        },
      },
      fetched_at: new Date().toISOString(),
    });

    const map = get(slotAssignments);
    expect(map.size).toBeGreaterThan(0);
    const featAssignments = [...map.values()].flat();
    expect(featAssignments.some(w => w.state === 'writing')).toBe(true);
    expect(featAssignments.some(w => w.state === 'gate')).toBe(true);
  });
});
