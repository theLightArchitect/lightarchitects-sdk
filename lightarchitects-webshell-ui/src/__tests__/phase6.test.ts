import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import {
  conductorTasks, arenaStatus, alerts, acknowledgedAlerts,
  conductorStats, arenaStats, alertStats, sitrepReady,
  siblingDispatchCounts, platformHealth,
  builds, siblingHealth, buildStats,
} from '$lib/stores';
import { SIBLINGS } from '$lib/types';

const NOW = new Date().toISOString();

const TEST_BUILD = {
  id: 'build-001', workspaceId: 'ws', name: 'Auth flow', metaSkill: '/BUILD' as const,
  status: 'in_progress' as const, pillars: [], currentPillar: 'ARCH' as const,
  confidence: 0.5, createdAt: NOW, updatedAt: NOW, modules: [], siblingDispatches: [],
};

const TEST_TASKS = [
  { id: 'ct-001', buildId: 'build-001', sibling: 'corso' as const, taskType: 'SCOUT', priority: 'high' as const, status: 'completed' as const, queuedAt: NOW, completedAt: NOW },
  { id: 'ct-002', buildId: 'build-001', sibling: 'quantum' as const, taskType: 'SCAN', priority: 'normal' as const, status: 'pending' as const, queuedAt: NOW },
  { id: 'ct-003', buildId: 'build-001', sibling: 'eva' as const, taskType: 'FETCH', priority: 'low' as const, status: 'running' as const, queuedAt: NOW, startedAt: NOW },
];

const TEST_ARENA = {
  activeRoutines: 3,
  queuedRoutines: 2,
  agents: [
    { id: 'a-01', sibling: 'corso' as const, status: 'active' as const, lastHeartbeat: NOW, routineCount: 2 },
    { id: 'a-02', sibling: 'quantum' as const, status: 'idle' as const, lastHeartbeat: NOW, routineCount: 0 },
    { id: 'a-03', sibling: 'eva' as const, status: 'error' as const, lastHeartbeat: NOW, routineCount: 1 },
  ],
  lastUpdate: NOW,
};

const TEST_ALERTS = [
  { id: 'al-001', severity: 'critical' as const, source: 'system' as const, title: 'Memory spike', message: 'RSS > 4GB', timestamp: NOW, acknowledged: false },
  { id: 'al-002', severity: 'warning' as const, source: 'sibling' as const, title: 'Degraded', message: 'corso latency high', timestamp: NOW, acknowledged: false },
  { id: 'al-003', severity: 'info' as const, source: 'webhook' as const, title: 'Deploy', message: 'Deployment complete', timestamp: NOW, acknowledged: false },
];

describe('Phase 6: SITREP + Platform Status', () => {
  beforeEach(() => {
    acknowledgedAlerts.set(new Set());
    builds.set([TEST_BUILD]);
    conductorTasks.set(TEST_TASKS);
    arenaStatus.set(TEST_ARENA);
    alerts.set(TEST_ALERTS);
  });

  afterEach(() => {
    builds.set([]);
    conductorTasks.set([]);
    arenaStatus.set({ activeRoutines: 0, queuedRoutines: 0, agents: [], lastUpdate: '' });
    alerts.set([]);
  });

  describe('Conductor stores', () => {
    it('has seeded conductor tasks', () => {
      const tasks = get(conductorTasks);
      expect(tasks.length).toBeGreaterThan(0);
    });

    it('has tasks in different states', () => {
      const tasks = get(conductorTasks);
      const statuses = new Set(tasks.map(t => t.status));
      expect(statuses.size).toBeGreaterThan(1);
    });

    it('has tasks assigned to siblings', () => {
      const tasks = get(conductorTasks);
      for (const t of tasks) {
        expect(SIBLINGS).toContain(t.sibling);
      }
    });

    it('has valid priority levels', () => {
      const tasks = get(conductorTasks);
      const validPriorities = ['high', 'normal', 'low'];
      for (const t of tasks) {
        expect(validPriorities).toContain(t.priority);
      }
    });

    it('conductorStats computes correctly', () => {
      const stats = get(conductorStats);
      const tasks = get(conductorTasks);
      expect(stats.total).toBe(tasks.length);
      expect(stats.pending + stats.running + stats.completed + stats.failed).toBe(tasks.length);
    });

    it('conductorStats queueDepth matches pending count', () => {
      const stats = get(conductorStats);
      const tasks = get(conductorTasks);
      expect(stats.queueDepth).toBe(tasks.filter(t => t.status === 'pending').length);
    });

    it('conductorTasks have valid buildId references', () => {
      const tasks = get(conductorTasks);
      const allBuilds = get(builds);
      const buildIds = new Set(allBuilds.map(b => b.id));
      for (const t of tasks) {
        expect(buildIds.has(t.buildId)).toBe(true);
      }
    });
  });

  describe('Arena stores', () => {
    it('has seeded arena agents', () => {
      const arena = get(arenaStatus);
      expect(arena.agents.length).toBeGreaterThan(0);
      expect(arena.activeRoutines).toBeGreaterThanOrEqual(0);
    });

    it('has agents for siblings', () => {
      const arena = get(arenaStatus);
      for (const agent of arena.agents) {
        expect(SIBLINGS).toContain(agent.sibling);
      }
    });

    it('agents have valid status', () => {
      const arena = get(arenaStatus);
      const validStatuses = ['active', 'idle', 'error'];
      for (const agent of arena.agents) {
        expect(validStatuses).toContain(agent.status);
      }
    });

    it('arenaStats computes correctly', () => {
      const stats = get(arenaStats);
      const arena = get(arenaStatus);
      expect(stats.activeAgents).toBe(arena.agents.filter(a => a.status === 'active').length);
      expect(stats.idleAgents).toBe(arena.agents.filter(a => a.status === 'idle').length);
    });

    it('arenaStats activeRoutines matches arena data', () => {
      const stats = get(arenaStats);
      const arena = get(arenaStatus);
      expect(stats.activeRoutines).toBe(arena.activeRoutines);
      expect(stats.queuedRoutines).toBe(arena.queuedRoutines);
    });
  });

  describe('Alert stores', () => {
    it('has seeded alerts', () => {
      const a = get(alerts);
      expect(a.length).toBeGreaterThan(0);
    });

    it('has alerts with different severities', () => {
      const a = get(alerts);
      const severities = new Set(a.map(alert => alert.severity));
      expect(severities.size).toBeGreaterThan(1);
    });

    it('has alerts with different sources', () => {
      const a = get(alerts);
      const sources = new Set(a.map(alert => alert.source));
      expect(sources.size).toBeGreaterThan(1);
    });

    it('alertStats computes correctly', () => {
      const stats = get(alertStats);
      const a = get(alerts);
      expect(stats.total).toBe(a.length);
      expect(stats.unacknowledged).toBe(a.filter(alert => !alert.acknowledged).length);
    });

    it('alertStats severity counts match data', () => {
      const stats = get(alertStats);
      const a = get(alerts);
      expect(stats.critical).toBe(a.filter(x => x.severity === 'critical').length);
      expect(stats.error).toBe(a.filter(x => x.severity === 'error').length);
      expect(stats.warning).toBe(a.filter(x => x.severity === 'warning').length);
      expect(stats.info).toBe(a.filter(x => x.severity === 'info').length);
    });

    it('acknowledgedAlerts starts empty', () => {
      expect(get(acknowledgedAlerts).size).toBe(0);
    });

    it('can add acknowledged alerts', () => {
      acknowledgedAlerts.update(s => { s.add('alert-001'); return new Set(s); });
      expect(get(acknowledgedAlerts).has('alert-001')).toBe(true);
      acknowledgedAlerts.set(new Set());
    });
  });

  describe('sitrepReady derived store', () => {
    it('returns true when data is loaded', () => {
      const ready = get(sitrepReady);
      // builds + siblingHealth (always 7 entries) + arena.agents > 0
      expect(ready).toBe(true);
    });
  });

  describe('siblingDispatchCounts derived store', () => {
    it('returns counts for all siblings', () => {
      const counts = get(siblingDispatchCounts);
      expect(Object.keys(counts).length).toBe(7);
    });

    it('counts only pending and running tasks', () => {
      const tasks = get(conductorTasks);
      const counts = get(siblingDispatchCounts);

      for (const sib of SIBLINGS) {
        const expected = tasks.filter(t =>
          t.sibling === sib && (t.status === 'running' || t.status === 'pending')
        ).length;
        expect(counts[sib as keyof typeof counts]).toBe(expected);
      }
    });
  });

  describe('platformHealth derived store', () => {
    it('returns a valid health status', () => {
      const health = get(platformHealth);
      expect(['healthy', 'degraded', 'offline']).toContain(health);
    });
  });

  describe('Component imports', () => {
    it('BuildPortfolio imports successfully', async () => {
      const mod = await import('$lib/../components/BuildPortfolio.svelte');
      expect(mod.default).toBeDefined();
    });

    it('ConductorPanel imports successfully', async () => {
      const mod = await import('$lib/../components/ConductorPanel.svelte');
      expect(mod.default).toBeDefined();
    });

    it('ArenaPanel imports successfully', async () => {
      const mod = await import('$lib/../components/ArenaPanel.svelte');
      expect(mod.default).toBeDefined();
    });

    it('AlertPanel imports successfully', async () => {
      const mod = await import('$lib/../components/AlertPanel.svelte');
      expect(mod.default).toBeDefined();
    });
  });

  describe('Dashboard.svelte integration', () => {
    it('Dashboard screen imports successfully', { timeout: 20_000 }, async () => {
      const mod = await import('$lib/../screens/Dashboard.svelte');
      expect(mod.default).toBeDefined();
    });
  });

  describe('Mock data integrity', () => {
    it('conductor tasks have valid timestamps', () => {
      const tasks = get(conductorTasks);
      for (const t of tasks) {
        expect(t.queuedAt).toBeTruthy();
        expect(() => new Date(t.queuedAt)).not.toThrow();
        if (t.startedAt) expect(() => new Date(t.startedAt!)).not.toThrow();
        if (t.completedAt) expect(() => new Date(t.completedAt!)).not.toThrow();
      }
    });

    it('arena agents have valid heartbeats', () => {
      const arena = get(arenaStatus);
      for (const agent of arena.agents) {
        expect(agent.lastHeartbeat).toBeTruthy();
        expect(() => new Date(agent.lastHeartbeat)).not.toThrow();
      }
    });

    it('alerts have valid timestamps', () => {
      const a = get(alerts);
      for (const alert of a) {
        expect(alert.timestamp).toBeTruthy();
        expect(() => new Date(alert.timestamp)).not.toThrow();
      }
    });

    it('build stats are consistent with build data', () => {
      const stats = get(buildStats);
      const b = get(builds);
      expect(stats.total).toBe(b.length);
      expect(stats.inProgress).toBe(b.filter(x => x.status === 'in_progress').length);
    });

    it('conductor tasks reference valid task types', () => {
      const tasks = get(conductorTasks);
      for (const t of tasks) {
        expect(t.taskType).toBeTruthy();
        expect(typeof t.taskType).toBe('string');
        expect(t.taskType.length).toBeGreaterThan(0);
      }
    });

    it('arena agents have non-negative routine counts', () => {
      const arena = get(arenaStatus);
      for (const agent of arena.agents) {
        expect(agent.routineCount).toBeGreaterThanOrEqual(0);
      }
    });
  });
});
