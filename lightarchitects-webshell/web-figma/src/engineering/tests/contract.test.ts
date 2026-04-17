// ============================================================================
// File: web-figma/src/engineering/tests/contract.test.ts
// Territory: ENGINEERING — not Figma Make synced
// Suite: CONTRACT — SSE event schema + sceneState type contracts
// ============================================================================

import { describe, it, expect } from 'vitest';
import { BUF_LEN, DECAY, PHASE_STEP, AMP_SCALE, PEAK_THRESHOLD } from '../scope/sibling-wave';
import { SIBLINGS, INITIAL_STATE } from '../store/sceneState';
import type { StrandActivationEvent, AyinSpanEvent, AyinConnStatus } from '../store/sceneState';

// ── SiblingWave constant contract (must match oscilloscope.rs) ──────────────

describe('SiblingWave constants — Rust parity contract', () => {
  it('BUF_LEN = 56', () => expect(BUF_LEN).toBe(56));
  it('DECAY = 0.88', () => expect(DECAY).toBeCloseTo(0.88));
  it('PHASE_STEP = 0.38', () => expect(PHASE_STEP).toBeCloseTo(0.38));
  it('AMP_SCALE = 0.55', () => expect(AMP_SCALE).toBeCloseTo(0.55));
  it('PEAK_THRESHOLD = 0.7', () => expect(PEAK_THRESHOLD).toBeCloseTo(0.7));
});

// ── SIBLINGS roster contract ─────────────────────────────────────────────────

describe('SIBLINGS roster', () => {
  it('contains exactly 7 members', () => expect(SIBLINGS).toHaveLength(7));
  it('includes all canonical squad members', () => {
    const set = new Set(SIBLINGS);
    for (const s of ['soul', 'eva', 'corso', 'quantum', 'seraph', 'ayin', 'larc']) {
      expect(set.has(s as never), `missing: ${s}`).toBe(true);
    }
  });
  it('has no duplicates', () => expect(new Set(SIBLINGS).size).toBe(SIBLINGS.length));
});

// ── INITIAL_STATE contract ───────────────────────────────────────────────────

describe('INITIAL_STATE', () => {
  it('ayinStatus starts as reconnecting', () => {
    expect(INITIAL_STATE.ayinStatus).toBe('reconnecting');
  });
  it('focusedSibling starts null', () => {
    expect(INITIAL_STATE.focusedSibling).toBeNull();
  });
});

// ── StrandActivationEvent shape contract ─────────────────────────────────────

describe('StrandActivationEvent schema', () => {
  it('accepts a valid event', () => {
    const evt: StrandActivationEvent = {
      sibling: 'soul',
      strand: 'helix',
      weight: 0.85,
      timestamp: '2026-04-17T00:00:00Z',
    };
    expect(evt.sibling).toBe('soul');
    expect(evt.weight).toBeGreaterThanOrEqual(0);
    expect(evt.weight).toBeLessThanOrEqual(1);
  });
});

// ── AyinSpanEvent shape contract ─────────────────────────────────────────────

describe('AyinSpanEvent schema', () => {
  it('accepts a valid span', () => {
    const span: AyinSpanEvent = {
      id: 'abc-123',
      actor: 'soul',
      action: 'helix.query',
      timestamp: '2026-04-17T00:00:00Z',
      durationMs: 42,
    };
    expect(span.durationMs).toBeGreaterThanOrEqual(0);
  });
});

// ── AyinConnStatus exhaustiveness ────────────────────────────────────────────

describe('AyinConnStatus values', () => {
  it('covers all 3 states', () => {
    const states: AyinConnStatus[] = ['connected', 'reconnecting', 'offline'];
    expect(states).toHaveLength(3);
  });
});
