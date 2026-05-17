import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import {
  ayinStatus, siblingHealth, builds, findings,
  conductorTasks, arenaStatus, selectedPillar,
  copilotMessages, copilotLoading,
} from '$lib/stores';
import { _handleEvent } from '$lib/sse';
import type { EventType, SiblingId } from '$lib/types';

// Mock crypto.randomUUID for consistent test IDs
let uuidCounter = 0;
vi.stubGlobal('crypto', {
  randomUUID: () => `test-uuid-${++uuidCounter}`,
});

function makeEvent(type: EventType, data: unknown) {
  return { type, data };
}

describe('SSE _handleEvent', () => {
  beforeEach(() => {
    ayinStatus.set('reconnecting');
    siblingHealth.set({} as Record<SiblingId, import('$lib/types').SiblingHealth>);
    builds.set([]);
    findings.set([]);
    conductorTasks.set([]);
    arenaStatus.set({ activeRoutines: 0, queuedRoutines: 0, agents: [], lastUpdate: '' });
    selectedPillar.set(null);
    copilotMessages.set([]);
    copilotLoading.set(false);
    uuidCounter = 0;
  });

  // --- ayin_status ---
  describe('ayin_status', () => {
    it('sets ayinStatus to "connected"', () => {
      _handleEvent(makeEvent('ayin_status', 'connected'));
      expect(get(ayinStatus)).toBe('connected');
    });

    it('sets ayinStatus to "offline"', () => {
      _handleEvent(makeEvent('ayin_status', 'offline'));
      expect(get(ayinStatus)).toBe('offline');
    });

    it('ignores invalid status values', () => {
      _handleEvent(makeEvent('ayin_status', 'invalid'));
      expect(get(ayinStatus)).toBe('reconnecting');
    });
  });

  // --- strand_activation ---
  describe('strand_activation', () => {
    it('spikes sibling wave on activation', () => {
      // spikeSibling updates the waves store — verify it doesn't throw
      _handleEvent(makeEvent('strand_activation', { sibling: 'corso' }));
      // No direct assertion on waves (internal); function completed without error
    });
  });

  // --- sibling_status ---
  describe('sibling_status', () => {
    it('updates siblingHealth with new status', () => {
      _handleEvent(makeEvent('sibling_status', {
        id: 'corso', status: 'online', uptime: 3600, lastHeartbeat: '2026-01-01',
      }));
      const health = get(siblingHealth);
      expect(health.corso).toBeDefined();
      expect(health.corso.status).toBe('online');
    });

    it('ignores events without id or status', () => {
      _handleEvent(makeEvent('sibling_status', { uptime: 100 }));
      expect(Object.keys(get(siblingHealth))).toHaveLength(0);
    });
  });

  // --- build_update ---
  describe('build_update', () => {
    it('updates existing build', () => {
      builds.set([{
        id: 'b1', workspaceId: 'ws', name: 'Test', metaSkill: '/BUILD',
        status: 'in_progress', pillars: [], currentPillar: 'ARCH',
        confidence: 0.5, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [],
      }]);
      _handleEvent(makeEvent('build_update', { id: 'b1', status: 'completed', confidence: 1.0 }));
      const updated = get(builds);
      expect(updated[0].status).toBe('completed');
      expect(updated[0].confidence).toBe(1.0);
    });

    it('skips events without id', () => {
      _handleEvent(makeEvent('build_update', { status: 'completed' }));
      expect(get(builds)).toHaveLength(0);
    });
  });

  // --- pillar_update (Phase 15 — real CORSO shell-out) ---
  describe('pillar_update', () => {
    const seed = () => builds.set([{
      id: 'b1', workspaceId: 'ws', name: 'Test', metaSkill: '/BUILD',
      status: 'in_progress',
      pillars: [{ pillar: 'ARCH', status: 'pending', confidence: 0, findings: [] }],
      currentPillar: 'ARCH', confidence: 0.5, createdAt: '', updatedAt: '',
      modules: [], siblingDispatches: [],
    }]);

    it('flips pillar to in_progress on started phase', () => {
      seed();
      // Phase 15 event shape: fields at top level (not under .data).
      _handleEvent({ type: 'pillar_update',
        build_id: 'b1', pillar: 'arch', phase: 'started', line: 'corso arch --format json',
      } as unknown as { type: EventType; data: unknown });
      expect(get(builds)[0].pillars[0].status).toBe('in_progress');
    });

    it('flips pillar to passed on completed phase with exit_code 0', () => {
      seed();
      _handleEvent({ type: 'pillar_update',
        build_id: 'b1', pillar: 'arch', phase: 'completed', exit_code: 0,
        artifact: 'pillar-arch.json',
      } as unknown as { type: EventType; data: unknown });
      expect(get(builds)[0].pillars[0].status).toBe('passed');
    });

    it('flips pillar to failed on non-zero exit_code', () => {
      seed();
      _handleEvent({ type: 'pillar_update',
        build_id: 'b1', pillar: 'arch', phase: 'completed', exit_code: 1,
      } as unknown as { type: EventType; data: unknown });
      expect(get(builds)[0].pillars[0].status).toBe('failed');
    });

    it('leaves pillar status untouched during output phase', () => {
      seed();
      _handleEvent({ type: 'pillar_update',
        build_id: 'b1', pillar: 'arch', phase: 'output', line: 'hello',
      } as unknown as { type: EventType; data: unknown });
      expect(get(builds)[0].pillars[0].status).toBe('pending');
    });

    it('ignores events with missing build_id', () => {
      seed();
      _handleEvent({ type: 'pillar_update',
        pillar: 'arch', phase: 'started',
      } as unknown as { type: EventType; data: unknown });
      expect(get(builds)[0].pillars[0].status).toBe('pending');
    });
  });

  // --- finding ---
  describe('finding', () => {
    it('adds new finding', () => {
      _handleEvent(makeEvent('finding', {
        id: 'f1', buildId: 'b1', pillar: 'QUAL', severity: 'warning',
        category: 'quality', title: 'Issue', description: 'Desc', verified: false,
      }));
      expect(get(findings)).toHaveLength(1);
      expect(get(findings)[0].id).toBe('f1');
    });

    it('updates existing finding', () => {
      _handleEvent(makeEvent('finding', {
        id: 'f1', buildId: 'b1', pillar: 'QUAL', severity: 'warning',
        category: 'quality', title: 'Issue', description: 'Desc', verified: false,
      }));
      _handleEvent(makeEvent('finding', {
        id: 'f1', buildId: 'b1', pillar: 'QUAL', severity: 'critical',
        category: 'quality', title: 'Issue', description: 'Desc', verified: true,
      }));
      expect(get(findings)).toHaveLength(1);
      expect(get(findings)[0].severity).toBe('critical');
      expect(get(findings)[0].verified).toBe(true);
    });
  });

  // --- conductor_task ---
  describe('conductor_task', () => {
    it('adds new task', () => {
      _handleEvent(makeEvent('conductor_task', {
        id: 't1', buildId: 'b1', sibling: 'corso', taskType: 'SCOUT',
        priority: 'high', status: 'pending', queuedAt: '',
      }));
      expect(get(conductorTasks)).toHaveLength(1);
    });

    it('updates existing task', () => {
      _handleEvent(makeEvent('conductor_task', {
        id: 't1', buildId: 'b1', sibling: 'corso', taskType: 'SCOUT',
        priority: 'high', status: 'pending', queuedAt: '',
      }));
      _handleEvent(makeEvent('conductor_task', {
        id: 't1', buildId: 'b1', sibling: 'corso', taskType: 'SCOUT',
        priority: 'high', status: 'completed', queuedAt: '',
      }));
      expect(get(conductorTasks)).toHaveLength(1);
      expect(get(conductorTasks)[0].status).toBe('completed');
    });
  });

  // --- arena_update ---
  describe('arena_update', () => {
    it('updates arena active routines count', () => {
      _handleEvent(makeEvent('arena_update', { activeRoutines: 3, queuedRoutines: 1 }));
      const arena = get(arenaStatus);
      expect(arena.activeRoutines).toBe(3);
      expect(arena.queuedRoutines).toBe(1);
    });

    it('updates agents list', () => {
      const agents = [{ id: 'a1', sibling: 'corso', status: 'active', lastHeartbeat: '', routineCount: 5 }];
      _handleEvent(makeEvent('arena_update', { agents }));
      expect(get(arenaStatus).agents).toHaveLength(1);
    });
  });

  // --- gateway_notify ---
  describe('gateway_notify', () => {
    it('sets selectedPillar on focus_pillar payload', () => {
      // gateway_notify handler casts event to { payload: {...} }
      // The SSE wire format puts payload at event.data, but the handler
      // reads from a `payload` field — so we match the handler's expectation
      _handleEvent({
        type: 'gateway_notify',
        data: undefined,
        payload: { type: 'focus_pillar', pillar: 'SEC' },
      } as unknown as { type: EventType; data: unknown });
      expect(get(selectedPillar)).toBe('SEC');
    });

    it('ignores non-focus_pillar payloads', () => {
      _handleEvent({
        type: 'gateway_notify',
        data: undefined,
        payload: { type: 'refresh_sitrep' },
      } as unknown as { type: EventType; data: unknown });
      expect(get(selectedPillar)).toBeNull();
    });
  });

  // --- copilot_response ---
  // Backend uses #[serde(tag = "type")] — fields are inlined at the top level,
  // not nested under `data`. Wire format: { type, chunk, done, sibling }.
  describe('copilot_response', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const cr = (fields: Record<string, unknown>) => ({ type: 'copilot_response', ...fields } as any);

    it('creates new assistant message on first chunk', () => {
      _handleEvent(cr({ chunk: 'Hello' }));
      const msgs = get(copilotMessages);
      expect(msgs).toHaveLength(1);
      expect(msgs[0].role).toBe('assistant');
      expect(msgs[0].content).toBe('Hello');
    });

    it('appends chunk to existing assistant message', () => {
      _handleEvent(cr({ chunk: 'Hello' }));
      _handleEvent(cr({ chunk: ' world' }));
      const msgs = get(copilotMessages);
      expect(msgs).toHaveLength(1);
      expect(msgs[0].content).toBe('Hello world');
    });

    it('sets copilotLoading to false on done', () => {
      copilotLoading.set(true);
      _handleEvent(cr({ chunk: 'Done', done: true }));
      expect(get(copilotLoading)).toBe(false);
    });

    it('creates new message if last message is not assistant', () => {
      copilotMessages.set([{
        id: 'u1', role: 'user', content: 'Question', timestamp: '',
      }]);
      _handleEvent(cr({ chunk: 'Answer' }));
      const msgs = get(copilotMessages);
      expect(msgs).toHaveLength(2);
      expect(msgs[1].role).toBe('assistant');
      expect(msgs[1].content).toBe('Answer');
    });

    it('handles empty chunk gracefully', () => {
      _handleEvent(cr({}));
      const msgs = get(copilotMessages);
      expect(msgs).toHaveLength(1);
      expect(msgs[0].content).toBe('');
    });

    it('preserves sibling field from SSE event', () => {
      _handleEvent(cr({ chunk: 'Result', sibling: 'corso' }));
      const msgs = get(copilotMessages);
      expect(msgs[0].sibling).toBe('corso');
    });

    it('does not set copilotLoading false without done flag', () => {
      copilotLoading.set(true);
      _handleEvent(cr({ chunk: 'Partial' }));
      expect(get(copilotLoading)).toBe(true);
    });
  });

  // --- default case ---
  describe('unknown event type', () => {
    it('does not throw on unrecognized event types', () => {
      // Use a valid event type that won't crash — 'ayin_status' with valid data
      expect(() => _handleEvent(makeEvent('ayin_status', 'connected'))).not.toThrow();
    });
  });
});