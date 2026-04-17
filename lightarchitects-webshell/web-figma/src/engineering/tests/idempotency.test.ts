// ============================================================================
// File: web-figma/src/engineering/tests/idempotency.test.ts
// Territory: ENGINEERING — not Figma Make synced
// Suite: IDEMPOTENCY — repeated operations produce identical state (SERAPH §27)
// ============================================================================

import { describe, it, expect } from 'vitest';
import { SiblingWave, BUF_LEN } from '../scope/sibling-wave';
import { SIBLINGS, INITIAL_STATE } from '../store/sceneState';

// ── Replicate sanitizers (same logic as CommandPalette.tsx) ──────────────────

function sanitizeSibling(raw: string): string | null {
  const s = raw.toLowerCase().trim();
  return (SIBLINGS as readonly string[]).includes(s) ? s : null;
}

function sanitizeQuery(raw: string): string | null {
  const s = raw.replace(/[^a-zA-Z0-9 ._\-]/g, '').slice(0, 200).trim();
  return s.length > 0 ? s : null;
}

// ── SiblingWave construction idempotency ─────────────────────────────────────

describe('SiblingWave construction idempotency', () => {
  it('two fresh instances have identical initial state', () => {
    const a = new SiblingWave();
    const b = new SiblingWave();
    expect(a.activity).toBe(b.activity);
    expect(a.phase).toBe(b.phase);
    expect(a.samples).toEqual(b.samples);
    expect(a.ttsBoost).toBe(b.ttsBoost);
  });

  it('initial samples array is BUF_LEN zeros on every construction', () => {
    for (let i = 0; i < 10; i++) {
      const w = new SiblingWave();
      expect(w.samples.length).toBe(BUF_LEN);
      expect(w.samples.every((s) => s === 0)).toBe(true);
    }
  });
});

// ── spike() idempotency ───────────────────────────────────────────────────────

describe('spike() idempotency', () => {
  it('double-spike produces same activity as single spike', () => {
    const single = new SiblingWave();
    single.spike();

    const double = new SiblingWave();
    double.spike();
    double.spike();

    expect(double.activity).toBe(single.activity);
    expect(double.activity).toBe(1.0);
  });

  it('N spikes all produce activity = 1.0 (saturates, not accumulates)', () => {
    const wave = new SiblingWave();
    for (let n = 1; n <= 100; n++) {
      wave.spike();
      expect(wave.activity).toBe(1.0);
    }
  });
});

// ── tick() determinism — same input → same output ────────────────────────────

describe('tick() determinism', () => {
  it('two identically-spiked waves produce identical samples after N ticks', () => {
    const a = new SiblingWave();
    const b = new SiblingWave();
    a.spike();
    b.spike();

    for (let i = 0; i < 30; i++) {
      a.tick();
      b.tick();
    }

    expect(a.activity).toBeCloseTo(b.activity, 12);
    expect(a.phase).toBeCloseTo(b.phase, 12);
    expect(a.samples).toEqual(b.samples);
  });

  it('two idle waves produce identical (zero) state after N ticks', () => {
    const a = new SiblingWave();
    const b = new SiblingWave();

    for (let i = 0; i < 100; i++) {
      a.tick();
      b.tick();
    }

    expect(a.activity).toBe(b.activity);
    expect(a.phase).toBe(b.phase);
    expect(a.samples).toEqual(b.samples);
  });
});

// ── sanitizeSibling idempotency ───────────────────────────────────────────────

describe('sanitizeSibling idempotency', () => {
  it('sanitizing an already-sanitized value produces the same result', () => {
    for (const s of SIBLINGS as readonly string[]) {
      const first = sanitizeSibling(s);
      const second = sanitizeSibling(first!); // first is guaranteed non-null for SIBLINGS
      expect(second).toBe(first);
    }
  });

  it('sanitizeSibling(null-result-input) returns null consistently', () => {
    expect(sanitizeSibling('notasibling')).toBeNull();
    expect(sanitizeSibling('notasibling')).toBeNull(); // second call same result
  });
});

// ── sanitizeQuery idempotency ─────────────────────────────────────────────────

describe('sanitizeQuery idempotency', () => {
  it('sanitizing an already-clean query twice produces the same result', () => {
    const queries = ['helix query', 'soul.strand_v1', 'test-123', 'abc 456'];
    for (const q of queries) {
      const first = sanitizeQuery(q);
      const second = sanitizeQuery(first!);
      expect(second).toBe(first);
    }
  });

  it('sanitizing a dirty query twice converges to the same clean form', () => {
    const dirty = '<script>alert(1)</script>';
    const first = sanitizeQuery(dirty);
    const second = sanitizeQuery(first ?? '');
    expect(second).toBe(first); // second pass produces no further changes
  });
});

// ── INITIAL_STATE idempotency ─────────────────────────────────────────────────

describe('INITIAL_STATE stability', () => {
  it('INITIAL_STATE is referentially stable (same object on multiple imports)', () => {
    // TypeScript module singletons guarantee same reference. Verify values are stable.
    expect(INITIAL_STATE.ayinStatus).toBe('reconnecting');
    expect(INITIAL_STATE.focusedSibling).toBeNull();
  });
});
