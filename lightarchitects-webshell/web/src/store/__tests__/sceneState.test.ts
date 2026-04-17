/**
 * sceneState — unit tests for the Zustand scene store.
 *
 * Tests verify every mutation: addStep, spawnOrb, tickOrbs,
 * setAyinStatus, clearSteps — and their effect on store state.
 *
 * Because vitest `isolate: true` is set in vite.config.ts, each test FILE
 * gets a fresh module registry.  Within this file, we reset store state
 * before each test via `useSceneStore.setState()` so tests are independent.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import {
  useSceneStore,
  STEP_PITCH,
  MAX_VISIBLE_STEPS,
  MAX_ORBS,
  DEFAULT_COLOUR,
} from '../sceneState';
import { tMin } from '../../helix-math';

/** Reset store to initial state before each test. */
function resetStore() {
  useSceneStore.setState({
    steps: [],
    orbQueue: [],
    ayinStatus: { connected: false, reconnecting: false, attempt: 0 },
  });
}

// ── addStep ───────────────────────────────────────────────────────────────────

describe('addStep', () => {
  beforeEach(resetStore);

  it('adds a step to the empty store', () => {
    useSceneStore.getState().addStep('s1', 'soul', 'rag.query');
    expect(useSceneStore.getState().steps).toHaveLength(1);
  });

  it('first step gets Y = tMin (index 0)', () => {
    useSceneStore.getState().addStep('s1', 'soul', 'rag.query');
    const step = useSceneStore.getState().steps[0];
    expect(step.y).toBeCloseTo(tMin + 0 * STEP_PITCH, 6);
  });

  it('second step gets Y = tMin + 1 * STEP_PITCH', () => {
    useSceneStore.getState().addStep('s1', 'soul', 'a');
    useSceneStore.getState().addStep('s2', 'soul', 'b');
    const step = useSceneStore.getState().steps[1];
    expect(step.y).toBeCloseTo(tMin + 1 * STEP_PITCH, 6);
  });

  it('step carries the correct id, actor, and action', () => {
    useSceneStore.getState().addStep('abc', 'corso', 'guard.scan');
    const step = useSceneStore.getState().steps[0];
    expect(step.id).toBe('abc');
    expect(step.actor).toBe('corso');
    expect(step.action).toBe('guard.scan');
  });

  it('eva actor gets colour 0xFF1493', () => {
    useSceneStore.getState().addStep('e1', 'eva', 'write');
    expect(useSceneStore.getState().steps[0].color).toBe(0xFF1493);
  });

  it('corso actor gets colour 0x00BFFF', () => {
    useSceneStore.getState().addStep('c1', 'corso', 'scan');
    expect(useSceneStore.getState().steps[0].color).toBe(0x00BFFF);
  });

  it('unknown actor gets DEFAULT_COLOUR', () => {
    useSceneStore.getState().addStep('u1', 'unknown_sibling', 'action');
    expect(useSceneStore.getState().steps[0].color).toBe(DEFAULT_COLOUR);
  });

  it('actor matching is case-insensitive', () => {
    useSceneStore.getState().addStep('e1', 'EVA', 'write');
    expect(useSceneStore.getState().steps[0].color).toBe(0xFF1493);
  });

  it('evicts the oldest step when MAX_VISIBLE_STEPS is exceeded', () => {
    // Fill to capacity.
    for (let i = 0; i < MAX_VISIBLE_STEPS; i++) {
      useSceneStore.getState().addStep(`s${i}`, 'soul', 'fill');
    }
    expect(useSceneStore.getState().steps).toHaveLength(MAX_VISIBLE_STEPS);
    const firstId = useSceneStore.getState().steps[0].id;
    expect(firstId).toBe('s0');

    // Adding one more must evict s0.
    useSceneStore.getState().addStep('overflow', 'soul', 'overflow');
    expect(useSceneStore.getState().steps).toHaveLength(MAX_VISIBLE_STEPS);
    expect(useSceneStore.getState().steps[0].id).toBe('s1');
    expect(useSceneStore.getState().steps[MAX_VISIBLE_STEPS - 1].id).toBe('overflow');
  });

  it('step count never exceeds MAX_VISIBLE_STEPS', () => {
    for (let i = 0; i < MAX_VISIBLE_STEPS + 100; i++) {
      useSceneStore.getState().addStep(`s${i}`, 'soul', 'fill');
    }
    expect(useSceneStore.getState().steps.length).toBeLessThanOrEqual(MAX_VISIBLE_STEPS);
  });
});

// ── spawnOrb ──────────────────────────────────────────────────────────────────

describe('spawnOrb', () => {
  beforeEach(resetStore);

  it('adds an orb to the queue', () => {
    useSceneStore.getState().spawnOrb('q1', []);
    expect(useSceneStore.getState().orbQueue).toHaveLength(1);
  });

  it('orb has correct id', () => {
    useSceneStore.getState().spawnOrb('query-42', []);
    expect(useSceneStore.getState().orbQueue[0].id).toBe('query-42');
  });

  it('orb starts with elapsed=0', () => {
    useSceneStore.getState().spawnOrb('q1', []);
    expect(useSceneStore.getState().orbQueue[0].elapsed).toBe(0);
  });

  it('orb colour is retrieval cyan (0x00f5ff)', () => {
    useSceneStore.getState().spawnOrb('q1', []);
    expect(useSceneStore.getState().orbQueue[0].color).toBe(0x00f5ff);
  });

  it('orb totalDuration > 0 even with no hits (origin + return path)', () => {
    useSceneStore.getState().spawnOrb('q1', []);
    expect(useSceneStore.getState().orbQueue[0].totalDuration).toBeGreaterThan(0);
  });

  it('orb resolves hit waypoints from step IDs', () => {
    // Add 3 steps, then spawn orb targeting the first two.
    useSceneStore.getState().addStep('step-a', 'soul', 'a');
    useSceneStore.getState().addStep('step-b', 'soul', 'b');
    useSceneStore.getState().addStep('step-c', 'soul', 'c');
    useSceneStore.getState().spawnOrb('q2', ['step-a', 'step-b']);
    const orb = useSceneStore.getState().orbQueue[0];
    // Path: origin + 2 hits + return = 4 waypoints.
    expect(orb.waypoints).toHaveLength(4);
  });

  it('ignores step IDs that do not exist in the store', () => {
    useSceneStore.getState().addStep('real', 'soul', 'x');
    useSceneStore.getState().spawnOrb('q3', ['real', 'ghost-id']);
    const orb = useSceneStore.getState().orbQueue[0];
    // ghost-id not in store → only 1 hit → path has 3 waypoints.
    expect(orb.waypoints).toHaveLength(3);
  });

  it('evicts the oldest orb when MAX_ORBS is exceeded', () => {
    for (let i = 0; i < MAX_ORBS; i++) {
      useSceneStore.getState().spawnOrb(`q${i}`, []);
    }
    expect(useSceneStore.getState().orbQueue).toHaveLength(MAX_ORBS);
    const firstId = useSceneStore.getState().orbQueue[0].id;
    expect(firstId).toBe('q0');

    useSceneStore.getState().spawnOrb('overflow', []);
    expect(useSceneStore.getState().orbQueue).toHaveLength(MAX_ORBS);
    expect(useSceneStore.getState().orbQueue[0].id).toBe('q1');
  });
});

// ── tickOrbs ─────────────────────────────────────────────────────────────────

describe('tickOrbs', () => {
  beforeEach(resetStore);

  it('advances elapsed on all orbs by delta', () => {
    useSceneStore.getState().spawnOrb('q1', []);
    useSceneStore.getState().spawnOrb('q2', []);
    useSceneStore.getState().tickOrbs(0.5);
    useSceneStore.getState().orbQueue.forEach((orb) => {
      expect(orb.elapsed).toBeCloseTo(0.5, 5);
    });
  });

  it('removes orbs whose elapsed >= totalDuration', () => {
    useSceneStore.getState().spawnOrb('q1', []);
    const orb = useSceneStore.getState().orbQueue[0];
    // Tick past the end of the animation.
    useSceneStore.getState().tickOrbs(orb.totalDuration + 1);
    expect(useSceneStore.getState().orbQueue).toHaveLength(0);
  });

  it('keeps orbs that have not yet finished', () => {
    useSceneStore.getState().spawnOrb('q1', []);
    const duration = useSceneStore.getState().orbQueue[0].totalDuration;
    // Tick to halfway.
    useSceneStore.getState().tickOrbs(duration * 0.5);
    expect(useSceneStore.getState().orbQueue).toHaveLength(1);
  });

  it('handles empty orbQueue without throwing', () => {
    expect(() => useSceneStore.getState().tickOrbs(0.016)).not.toThrow();
  });

  it('short-lived and long-lived orbs are evicted independently', () => {
    useSceneStore.getState().spawnOrb('short', []);   // empty path → minimal duration
    useSceneStore.getState().addStep('far-step', 'soul', 'x');
    // Spawn orb with a distant hit to create a longer path.
    for (let i = 0; i < 10; i++) {
      useSceneStore.getState().addStep(`s${i}`, 'soul', 'fill');
    }
    useSceneStore.getState().spawnOrb('long', ['s9']);

    const shortDur = useSceneStore.getState().orbQueue[0].totalDuration;
    // Tick just past the short orb's duration.
    useSceneStore.getState().tickOrbs(shortDur + 0.01);
    // The 'short' orb must be gone; the 'long' orb may still be alive.
    const remaining = useSceneStore.getState().orbQueue.map((o) => o.id);
    expect(remaining).not.toContain('short');
  });
});

// ── setAyinStatus ─────────────────────────────────────────────────────────────

describe('setAyinStatus', () => {
  beforeEach(resetStore);

  it('sets connected=true', () => {
    useSceneStore.getState().setAyinStatus({ connected: true, reconnecting: false, attempt: 0 });
    expect(useSceneStore.getState().ayinStatus.connected).toBe(true);
  });

  it('sets reconnecting state with attempt counter', () => {
    useSceneStore.getState().setAyinStatus({ connected: false, reconnecting: true, attempt: 3 });
    const s = useSceneStore.getState().ayinStatus;
    expect(s.reconnecting).toBe(true);
    expect(s.attempt).toBe(3);
  });

  it('reflects disconnected state', () => {
    useSceneStore.getState().setAyinStatus({ connected: true, reconnecting: false, attempt: 0 });
    useSceneStore.getState().setAyinStatus({ connected: false, reconnecting: false, attempt: 0 });
    expect(useSceneStore.getState().ayinStatus.connected).toBe(false);
  });
});

// ── clearSteps ────────────────────────────────────────────────────────────────

describe('clearSteps', () => {
  beforeEach(resetStore);

  it('removes all steps from the store', () => {
    useSceneStore.getState().addStep('a', 'soul', 'x');
    useSceneStore.getState().addStep('b', 'soul', 'y');
    useSceneStore.getState().clearSteps();
    expect(useSceneStore.getState().steps).toHaveLength(0);
  });

  it('does not affect orbQueue', () => {
    useSceneStore.getState().spawnOrb('q1', []);
    useSceneStore.getState().clearSteps();
    expect(useSceneStore.getState().orbQueue).toHaveLength(1);
  });
});
