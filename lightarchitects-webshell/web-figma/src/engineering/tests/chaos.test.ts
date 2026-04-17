// ============================================================================
// File: web-figma/src/engineering/tests/chaos.test.ts
// Territory: ENGINEERING — not Figma Make synced
// Suite: CHAOS — stress, boundary, and stability tests for SiblingWave (SERAPH §27)
// ============================================================================

import { describe, it, expect } from 'vitest';
import { SiblingWave, BUF_LEN, AMP_SCALE } from '../scope/sibling-wave';

describe('Chaos: rapid spikes', () => {
  it('1000 consecutive spikes never push activity above 1.0', () => {
    const wave = new SiblingWave();
    for (let i = 0; i < 1000; i++) wave.spike();
    expect(wave.activity).toBe(1.0);
  });

  it('alternating spike/tick 200 cycles maintains BUF_LEN samples', () => {
    const wave = new SiblingWave();
    for (let i = 0; i < 200; i++) {
      wave.spike();
      wave.tick();
    }
    expect(wave.samples.length).toBe(BUF_LEN);
  });

  it('activity never exceeds 1.0 regardless of spike frequency', () => {
    const wave = new SiblingWave();
    for (let i = 0; i < 500; i++) {
      wave.spike();
      wave.tick();
      expect(wave.activity).toBeLessThanOrEqual(1.0);
    }
  });

  it('samples stay within AMP_SCALE bounds under maximum stress', () => {
    const wave = new SiblingWave();
    for (let i = 0; i < 500; i++) {
      wave.spike();
      wave.tick();
    }
    // AMP_SCALE = 0.55; ttsBoost = 1.0; sin ≤ 1.0; activity ≤ 1.0
    expect(wave.samples.every((s) => Math.abs(s) <= AMP_SCALE)).toBe(true);
  });
});

describe('Chaos: idle stability (no activation events)', () => {
  it('500 idle ticks produce no activity', () => {
    const wave = new SiblingWave();
    for (let i = 0; i < 500; i++) wave.tick();
    expect(wave.activity).toBe(0);
    expect(wave.isActive()).toBe(false);
  });

  it('500 idle ticks keep all samples exactly zero', () => {
    const wave = new SiblingWave();
    for (let i = 0; i < 500; i++) wave.tick();
    expect(wave.samples.every((s) => s === 0)).toBe(true);
  });

  it('phase does not drift during 500 idle ticks', () => {
    const wave = new SiblingWave();
    const initialPhase = wave.phase;
    for (let i = 0; i < 500; i++) wave.tick();
    expect(wave.phase).toBe(initialPhase);
  });
});

describe('Chaos: partial activation bursts', () => {
  it('spike → 10 ticks → spike: activity resets to 1.0', () => {
    const wave = new SiblingWave();
    wave.spike();
    for (let i = 0; i < 10; i++) wave.tick();
    const decayedActivity = wave.activity;
    expect(decayedActivity).toBeLessThan(1.0);

    wave.spike(); // re-spike during decay
    expect(wave.activity).toBe(1.0);
  });

  it('spike → full decay → spike: activity resets cleanly to 1.0', () => {
    const wave = new SiblingWave();
    wave.spike();
    for (let i = 0; i < 400; i++) wave.tick();
    expect(wave.activity).toBe(0);

    wave.spike();
    expect(wave.activity).toBe(1.0);
    expect(wave.isActive()).toBe(true);
  });

  it('10000 ticks total (mixed spike/idle) never throws', () => {
    const wave = new SiblingWave();
    expect(() => {
      for (let i = 0; i < 10000; i++) {
        if (i % 100 === 0) wave.spike();
        wave.tick();
      }
    }).not.toThrow();
  });
});
