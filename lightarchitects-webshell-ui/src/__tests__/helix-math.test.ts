import { describe, it, expect } from 'vitest';
import * as THREE from 'three';
import {
  tMin, tMax, entities, R_bundle,
  getFade, getPrimaryFrame, getEntityCenter,
  getMiniAnchorFrame, getSubStrandPos, getSoulCenter, seededRandom,
} from '$lib/helix/helix-math';

// Tolerance for floating-point assertions
const EPS = 1e-5;

describe('constants', () => {
  it('tMin < tMax', () => {
    expect(tMin).toBeLessThan(tMax);
  });

  it('entities array is non-empty with required fields', () => {
    expect(entities.length).toBeGreaterThan(0);
    for (const e of entities) {
      expect(typeof e.id).toBe('string');
      expect(typeof e.rail).toBe('number');
      expect(e.rail === 0 || e.rail === 1).toBe(true);
    }
  });
});

describe('getFade', () => {
  it('returns 0 well below tMin', () => {
    expect(getFade(tMin - 20)).toBe(0);
  });

  it('returns 1 well above tMin', () => {
    expect(getFade(tMin + 20)).toBe(1);
  });

  it('returns value in [0, 1]', () => {
    for (let y = tMin - 5; y <= tMax + 5; y += 5) {
      const f = getFade(y);
      expect(f).toBeGreaterThanOrEqual(0);
      expect(f).toBeLessThanOrEqual(1);
    }
  });
});

describe('getPrimaryFrame', () => {
  it('returns C, N, B Vector3s', () => {
    const { C, N, B } = getPrimaryFrame(0, 0);
    expect(C).toBeInstanceOf(THREE.Vector3);
    expect(N).toBeInstanceOf(THREE.Vector3);
    expect(B).toBeInstanceOf(THREE.Vector3);
  });

  it('rail 0 and rail 1 produce different centers', () => {
    const { C: c0 } = getPrimaryFrame(0, 0);
    const { C: c1 } = getPrimaryFrame(1, 0);
    expect(c0.distanceTo(c1)).toBeGreaterThan(EPS);
  });

  it('center y matches input y', () => {
    const { C } = getPrimaryFrame(0, 5);
    expect(C.y).toBeCloseTo(5, 4);
  });
});

describe('getEntityCenter', () => {
  it('returns E, N, B for each entity', () => {
    for (let i = 0; i < entities.length; i++) {
      const { E, N, B } = getEntityCenter(i, 0);
      expect(E).toBeInstanceOf(THREE.Vector3);
      expect(N).toBeInstanceOf(THREE.Vector3);
      expect(B).toBeInstanceOf(THREE.Vector3);
    }
  });

  it('different entities have different E centers', () => {
    const { E: a } = getEntityCenter(0, 0);
    const { E: b } = getEntityCenter(1, 0);
    expect(a.distanceTo(b)).toBeGreaterThan(EPS);
  });
});

describe('getMiniAnchorFrame', () => {
  it('returns M, N, B Vector3s', () => {
    const { M, N, B } = getMiniAnchorFrame(0, 0, 0);
    expect(M).toBeInstanceOf(THREE.Vector3);
    expect(N).toBeInstanceOf(THREE.Vector3);
    expect(B).toBeInstanceOf(THREE.Vector3);
  });

  it('N is unit length (normalized)', () => {
    const { N } = getMiniAnchorFrame(0, 0, 0);
    expect(Math.abs(N.length() - 1)).toBeLessThan(EPS);
  });
});

describe('getSubStrandPos', () => {
  it('returns a Vector3', () => {
    const v = getSubStrandPos(0, 0, 0, 4, 0);
    expect(v).toBeInstanceOf(THREE.Vector3);
  });

  it('different strand indices produce different positions', () => {
    const a = getSubStrandPos(0, 0, 0, 4, 0);
    const b = getSubStrandPos(0, 0, 1, 4, 0);
    expect(a.distanceTo(b)).toBeGreaterThan(EPS);
  });
});

describe('getSoulCenter', () => {
  it('returns a Vector3', () => {
    const v = getSoulCenter(0);
    expect(v).toBeInstanceOf(THREE.Vector3);
  });

  it('center shifts with y', () => {
    const a = getSoulCenter(-5);
    const b = getSoulCenter(5);
    expect(a.distanceTo(b)).toBeGreaterThan(EPS);
  });
});

describe('seededRandom', () => {
  it('generator returns values in [0, 1)', () => {
    const rng = seededRandom(1);
    for (let i = 0; i < 20; i++) {
      const v = rng();
      expect(v).toBeGreaterThanOrEqual(0);
      expect(v).toBeLessThan(1);
    }
  });

  it('same seed produces same sequence (deterministic)', () => {
    const a = seededRandom(42);
    const b = seededRandom(42);
    for (let i = 0; i < 5; i++) {
      expect(a()).toBeCloseTo(b(), 10);
    }
  });

  it('different seeds produce different first values', () => {
    const vals = new Set(Array.from({ length: 10 }, (_, i) => seededRandom(i + 1)()));
    expect(vals.size).toBeGreaterThan(5);
  });
});

describe('R_bundle constant', () => {
  it('is a positive number', () => {
    expect(R_bundle).toBeGreaterThan(0);
  });
});
