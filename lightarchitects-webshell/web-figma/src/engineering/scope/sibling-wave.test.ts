// ============================================================================
// File: web-figma/src/engineering/scope/sibling-wave.test.ts
// Territory: ENGINEERING — not Figma Make synced
// Purpose: Parity tests for SiblingWave — mirrors lÆx0-cli/tests/oscilloscope.rs
// Status: HIBERNATING — vitest framework adopted in Phase 7 (see plan)
// ============================================================================
//
// This file is written in vitest-compatible syntax (describe/it/expect).
// Once vitest is added as a devDep (Phase 7), these tests run automatically.
// Until then, the file compiles as-is (the imports resolve; vitest is the
// only missing piece, and TypeScript doesn't require it at build time since
// this file isn't imported by the app bundle).

import { describe, it, expect } from 'vitest';
import { SiblingWave, BUF_LEN, PEAK_THRESHOLD } from './sibling-wave';

describe('SiblingWave init', () => {
  it('starts with activity = 0', () => {
    const w = new SiblingWave();
    expect(w.activity).toBe(0);
  });

  it('starts with exactly BUF_LEN samples', () => {
    const w = new SiblingWave();
    expect(w.samples.length).toBe(BUF_LEN);
  });

  it('starts with all samples zero', () => {
    const w = new SiblingWave();
    expect(w.samples.every((s) => s === 0)).toBe(true);
  });

  it('BUF_LEN is 56 (Rust parity)', () => {
    expect(BUF_LEN).toBe(56);
  });
});

describe('SiblingWave.spike', () => {
  it('sets activity to 1.0', () => {
    const w = new SiblingWave();
    w.spike();
    expect(w.activity).toBe(1.0);
  });
});

describe('SiblingWave.tick — decay', () => {
  it('decays activity below 1.0 after one tick', () => {
    const w = new SiblingWave();
    w.spike();
    w.tick();
    expect(w.activity).toBeLessThan(1.0);
    expect(w.activity).toBeGreaterThan(0);
  });

  it('decays activity to near-zero after 200 ticks', () => {
    const w = new SiblingWave();
    w.spike();
    for (let i = 0; i < 200; i++) w.tick();
    expect(w.activity).toBeLessThan(0.001);
  });

  it('first tick activity ≈ 0.88 (DECAY)', () => {
    const w = new SiblingWave();
    w.spike();
    w.tick();
    // After spike (activity=1.0), tick multiplies by DECAY=0.88.
    expect(Math.abs(w.activity - 0.88)).toBeLessThan(1e-10);
  });
});

describe('SiblingWave.tick — ring buffer', () => {
  it('keeps samples.length at BUF_LEN after 100 ticks', () => {
    const w = new SiblingWave();
    w.spike();
    for (let i = 0; i < 100; i++) w.tick();
    expect(w.samples.length).toBe(BUF_LEN);
  });

  it('produces non-zero samples while active', () => {
    const w = new SiblingWave();
    w.spike();
    for (let i = 0; i < 10; i++) w.tick();
    // Some of the recent samples must be non-zero (sine is rarely exactly zero).
    const recent = w.samples.slice(-10);
    expect(recent.some((s) => s !== 0)).toBe(true);
  });

  it('produces only zero samples when idle', () => {
    const w = new SiblingWave();
    // No spike — just tick forever.
    for (let i = 0; i < 100; i++) w.tick();
    expect(w.samples.every((s) => s === 0)).toBe(true);
  });
});

describe('SiblingWave.isActive', () => {
  it('is false on init', () => {
    expect(new SiblingWave().isActive()).toBe(false);
  });

  it('is true right after spike', () => {
    const w = new SiblingWave();
    w.spike();
    expect(w.isActive()).toBe(true);
  });

  it('becomes false after full decay', () => {
    const w = new SiblingWave();
    w.spike();
    for (let i = 0; i < 200; i++) w.tick();
    expect(w.isActive()).toBe(false);
  });
});

describe('Peak filter (|s| > PEAK_THRESHOLD)', () => {
  it('PEAK_THRESHOLD is exactly 0.7', () => {
    expect(PEAK_THRESHOLD).toBe(0.7);
  });

  it('filters samples above and below 0.7 by absolute value', () => {
    const samples = [0.8, 0.5, -0.75, 0.7, -0.9];
    const peaks = samples.filter((s) => Math.abs(s) > PEAK_THRESHOLD);
    expect(peaks).toEqual([0.8, -0.75, -0.9]); // 0.5 excluded, 0.7 strict-exclusion
  });
});

describe('Sample amplitude bounds', () => {
  it('samples stay within [-AMP_SCALE, +AMP_SCALE] with default ttsBoost=1.0', () => {
    const w = new SiblingWave();
    w.spike();
    for (let i = 0; i < 200; i++) w.tick();
    // AMP_SCALE = 0.55, ttsBoost=1.0, |sin| <= 1, activity <= 1.
    // Upper bound: 1.0 * 1.0 * 1.0 * 0.55 = 0.55
    expect(w.samples.every((s) => Math.abs(s) <= 0.55)).toBe(true);
  });
});
