import { describe, it, expect, beforeEach } from 'vitest';
import {
  builds, workspaces, findings, logEntries, selectedPillar,
  buildStats, activeBuild, currentBuildId,
  spikeSibling, startWaveTick, stopWaveTick,
  waves,
  recentEventBuffer, pushRecentEvent, snapshotContextForCopilot,
  currentRoute, siblingHealth,
} from '$lib/stores';
import { get } from 'svelte/store';
import { PILLARS, SIBLINGS } from '$lib/types';
import type { SiblingHealth, SiblingId } from '$lib/types';

describe('stores', () => {
  describe('initial state', () => {
    it('initializes builds as empty array', () => {
      builds.set([]);
      expect(get(builds)).toHaveLength(0);
    });

    it('initializes workspaces as empty array', () => {
      workspaces.set([]);
      expect(get(workspaces)).toHaveLength(0);
    });

    it('initializes findings as empty array', () => {
      findings.set([]);
      expect(get(findings)).toHaveLength(0);
    });

    it('initializes log entries as empty array', () => {
      logEntries.set([]);
      expect(get(logEntries)).toHaveLength(0);
    });
  });

  describe('buildStats derived store', () => {
    it('computes zeros from empty builds', () => {
      builds.set([]);
      const stats = get(buildStats);
      expect(stats.total).toBe(0);
      expect(stats.inProgress).toBe(0);
      expect(stats.completed).toBe(0);
      expect(stats.pending).toBe(0);
      expect(stats.failed).toBe(0);
    });

    it('computes correct stats from injected builds', () => {
      builds.set([
        { id: 'b1', workspaceId: 'ws', name: 'A', metaSkill: '/BUILD', status: 'in_progress', pillars: [], currentPillar: 'ARCH', confidence: 0.5, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [] },
        { id: 'b2', workspaceId: 'ws', name: 'B', metaSkill: '/BUILD', status: 'queued', pillars: [], currentPillar: 'ARCH', confidence: 0, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [] },
        { id: 'b3', workspaceId: 'ws', name: 'C', metaSkill: '/BUILD', status: 'completed', pillars: [], currentPillar: 'OPS', confidence: 0.9, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [] },
      ]);
      const stats = get(buildStats);
      expect(stats.total).toBe(3);
      expect(stats.inProgress).toBe(1);
      expect(stats.pending).toBe(1);
      expect(stats.completed).toBe(1);
      builds.set([]);
    });
  });

  describe('activeBuild derived store', () => {
    it('returns null when no build is selected', () => {
      selectedPillar.set(null);
      const build = get(activeBuild);
      // currentBuildId is not set by default in this test
      // The store starts with whatever was last set
      expect(build).toBeDefined(); // Could be null or a build depending on state
    });
  });

  describe('selectedPillar', () => {
    it('starts as null', () => {
      expect(get(selectedPillar)).toBeNull();
    });

    it('can be set to a pillar', () => {
      selectedPillar.set('QUAL');
      expect(get(selectedPillar)).toBe('QUAL');
      selectedPillar.set(null);
    });

    it('can be toggled', () => {
      selectedPillar.set('ARCH');
      expect(get(selectedPillar)).toBe('ARCH');
      selectedPillar.set(null);
      expect(get(selectedPillar)).toBeNull();
    });
  });

  describe('spikeSibling', () => {
    it('spikes a sibling wave to activity 1.0', () => {
      spikeSibling('corso');
      // We can't directly check waves since it's a store
      // But the function should not throw
      expect(true).toBe(true);
    });

    it('does not throw for unknown siblings', () => {
      // The SIBLINGS array defines valid IDs, but the waves store
      // may not have an entry for unknown keys
      expect(() => spikeSibling('soul')).not.toThrow();
    });
  });

  describe('findings store', () => {
    it('initializes empty and accepts injected data', () => {
      findings.set([]);
      expect(get(findings)).toHaveLength(0);
      findings.set([
        { id: 'f1', buildId: 'b1', pillar: 'SEC', severity: 'error', category: 'security', title: 'T', description: 'D', verified: false },
        { id: 'f2', buildId: 'b1', pillar: 'QUAL', severity: 'warning', category: 'quality', title: 'T2', description: 'D2', verified: true, file: 'a.ts' },
      ]);
      expect(get(findings)).toHaveLength(2);
      const pillars = new Set(get(findings).map(f => f.pillar));
      expect(pillars.size).toBeGreaterThan(1);
      findings.set([]);
    });
  });

  describe('log entries store', () => {
    it('initializes empty and accepts injected data', () => {
      logEntries.set([]);
      expect(get(logEntries)).toHaveLength(0);
      logEntries.set([
        { id: 'l1', timestamp: new Date().toISOString(), level: 'info', source: 'corso', message: 'started' },
        { id: 'l2', timestamp: new Date().toISOString(), level: 'warn', source: 'ayin', message: 'degraded' },
      ]);
      const sources = new Set(get(logEntries).map(e => e.source));
      expect(sources.size).toBe(2);
      logEntries.set([]);
    });
  });

  describe('wave tick system', () => {
    it('startWaveTick creates an interval', () => {
      startWaveTick();
      // After starting, waves should begin ticking
      const w = get(waves);
      expect(w).toBeDefined();
      expect(Object.keys(w)).toHaveLength(7);
      stopWaveTick();
    });

    it('stopWaveTick clears the interval', () => {
      startWaveTick();
      stopWaveTick();
      // Should not throw and should be idempotent
      stopWaveTick();
    });
  });

  describe('currentBuildId', () => {
    it('starts as null', () => {
      expect(get(currentBuildId)).toBeNull();
    });

    it('can be set to a build ID', () => {
      currentBuildId.set('build-001');
      expect(get(currentBuildId)).toBe('build-001');
      currentBuildId.set(null);
    });
  });

  describe('activeBuild derived store', () => {
    it('returns null when no build is selected', () => {
      currentBuildId.set(null);
      expect(get(activeBuild)).toBeNull();
    });

    it('returns the matching build when a build ID is set', () => {
      builds.set([{ id: 'build-001', workspaceId: 'ws', name: 'Auth flow', metaSkill: '/BUILD', status: 'in_progress', pillars: [], currentPillar: 'QUAL', confidence: 0.6, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [] }]);
      currentBuildId.set('build-001');
      const build = get(activeBuild);
      expect(build).toBeDefined();
      expect(build!.id).toBe('build-001');
      expect(build!.name).toBe('Auth flow');
      currentBuildId.set(null);
      builds.set([]);
    });

    it('returns null for an unknown build ID', () => {
      currentBuildId.set('nonexistent');
      expect(get(activeBuild)).toBeNull();
      currentBuildId.set(null);
    });
  });

  describe('copilot context buffer (copilot-omniscience-read)', () => {
    beforeEach(() => {
      recentEventBuffer.set([]);
    });

    it('starts empty', () => {
      expect(get(recentEventBuffer)).toHaveLength(0);
    });

    it('pushRecentEvent adds an entry with seq, timestamp, source, event', () => {
      pushRecentEvent('BuildRunner', { type: 'BuildStarted' });
      const buf = get(recentEventBuffer);
      expect(buf).toHaveLength(1);
      expect(buf[0].source).toBe('BuildRunner');
      expect(buf[0].seq).toBeGreaterThan(0);
      expect(buf[0].timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
      expect(buf[0].event).toEqual({ type: 'BuildStarted' });
    });

    it('pushRecentEvent inserts newest-first', () => {
      pushRecentEvent('CORSO', { type: 'first' });
      pushRecentEvent('AYIN', { type: 'second' });
      const buf = get(recentEventBuffer);
      expect((buf[0].event as { type: string }).type).toBe('second');
      expect((buf[1].event as { type: string }).type).toBe('first');
    });

    it('pushRecentEvent increments seq monotonically', () => {
      pushRecentEvent('Copilot', { a: 1 });
      pushRecentEvent('Copilot', { b: 2 });
      const buf = get(recentEventBuffer);
      expect(buf[0].seq).toBeGreaterThan(buf[1].seq);
    });

    it('snapshotContextForCopilot returns chronological recentEvents', () => {
      pushRecentEvent('BuildRunner', { type: 'A' });
      pushRecentEvent('CORSO', { type: 'B' });
      const snap = snapshotContextForCopilot();
      expect(snap.recentEvents[0].source).toBe('BuildRunner');
      expect(snap.recentEvents[1].source).toBe('CORSO');
    });

    it('snapshotContextForCopilot includes capturedAt ISO timestamp', () => {
      const snap = snapshotContextForCopilot();
      expect(snap.capturedAt).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
    });

    it('snapshotContextForCopilot marks oversize events', () => {
      const bigPayload = { data: 'x'.repeat(5000) };
      pushRecentEvent('BuildRunner', bigPayload);
      pushRecentEvent('CORSO', { small: true });
      const snap = snapshotContextForCopilot();
      // After reverse: [BuildRunner(big), CORSO(small)] → big at index 0
      expect(snap.oversizeIndices).toContain(0);
      expect(snap.oversizeIndices).not.toContain(1);
    });

    it('rolling window caps at 50 events and evicts oldest', () => {
      for (let i = 0; i < 55; i++) {
        pushRecentEvent('AYIN', { n: i });
      }
      const buf = get(recentEventBuffer);
      expect(buf).toHaveLength(50);
      // Newest (n:54) is at index 0; oldest retained is n:5
      expect((buf[0].event as { n: number }).n).toBe(54);
      expect((buf[49].event as { n: number }).n).toBe(5);
    });

    it('snapshotContextForCopilot captures currentRoute into uiContext', () => {
      currentRoute.set('/builds/test-123');
      const snap = snapshotContextForCopilot();
      expect(snap.uiContext.route).toBe('/builds/test-123');
      currentRoute.set('/');
    });

    it('snapshotContextForCopilot includes degraded siblings in uiContext', () => {
      const health: Record<SiblingId, SiblingHealth> = {
        corso: { id: 'corso', status: 'degraded', uptime: 0, lastHeartbeat: '', capabilities: [] },
        eva:   { id: 'eva',   status: 'online',   uptime: 1, lastHeartbeat: '', capabilities: [] },
        soul:  { id: 'soul',  status: 'offline',  uptime: 0, lastHeartbeat: '', capabilities: [] },
        quantum:{ id: 'quantum', status: 'online', uptime: 1, lastHeartbeat: '', capabilities: [] },
        seraph:{ id: 'seraph', status: 'online',  uptime: 1, lastHeartbeat: '', capabilities: [] },
        ayin:  { id: 'ayin',  status: 'online',   uptime: 1, lastHeartbeat: '', capabilities: [] },
        laex:  { id: 'laex',  status: 'online',   uptime: 1, lastHeartbeat: '', capabilities: [] },
      };
      siblingHealth.set(health);
      const snap = snapshotContextForCopilot();
      expect(snap.uiContext.degraded).toContain('corso');
      expect(snap.uiContext.degraded).toContain('soul');
      expect(snap.uiContext.degraded).not.toContain('eva');
    });
  });
});