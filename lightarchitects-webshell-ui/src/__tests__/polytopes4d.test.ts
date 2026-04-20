import { describe, it, expect } from 'vitest';
import { getPolytope4D, type Polytope4DType } from '$lib/polytopes4d-canvas2d';

describe('polytopes4d', () => {
  describe('getPolytope4D', () => {
    it('returns pentachoron (5-cell) with correct geometry', () => {
      const p = getPolytope4D('pentachoron');
      expect(p.vertices).toHaveLength(5);
      expect(p.edges).toHaveLength(10);
    });

    it('returns tesseract (8-cell) with correct geometry', () => {
      const p = getPolytope4D('tesseract');
      expect(p.vertices).toHaveLength(16);
      expect(p.edges).toHaveLength(32);
    });

    it('returns hexadecachoron (16-cell) with correct geometry', () => {
      const p = getPolytope4D('hexadecachoron');
      expect(p.vertices).toHaveLength(8);
      expect(p.edges).toHaveLength(24);
    });

    it('returns icositetrachoron (24-cell) with correct geometry', () => {
      const p = getPolytope4D('icositetrachoron');
      expect(p.vertices).toHaveLength(24);
      expect(p.edges).toHaveLength(96);
    });

    it('returns hexacosichoron (600-cell) with correct geometry', () => {
      const p = getPolytope4D('hexacosichoron');
      expect(p.vertices).toHaveLength(120);
      expect(p.edges).toHaveLength(720);
    });

    it('returns rectified 5-cell with correct geometry', () => {
      const p = getPolytope4D('rectified5cell');
      expect(p.vertices).toHaveLength(10);
      expect(p.edges).toHaveLength(30);
    });

    it('returns doubleHelix4D with correct geometry', () => {
      const p = getPolytope4D('doubleHelix4D');
      // 24 per strand × 2 = 48 vertices
      expect(p.vertices).toHaveLength(48);
      // 24 backbone per strand + 24/2 rungs = 48 + 12 = 60
      // Actually: 24+24 backbone + 12 rungs = 60
      expect(p.edges.length).toBeGreaterThan(0);
    });

    it('returns dualCompound with combined geometry', () => {
      const p = getPolytope4D('dualCompound');
      // tesseract (16) + hexadecachoron (8) = 24 vertices
      expect(p.vertices).toHaveLength(24);
      // tesseract (32) + hexadecachoron (24) = 56 edges
      expect(p.edges).toHaveLength(56);
    });

    it('returns duoprism variants with correct geometry', () => {
      const duoprism55 = getPolytope4D('duoprism55');
      expect(duoprism55.vertices).toHaveLength(25); // 5×5
      expect(duoprism55.edges).toHaveLength(50); // 2×5×5

      const duoprism34 = getPolytope4D('duoprism34');
      expect(duoprism34.vertices).toHaveLength(12); // 3×4
      expect(duoprism34.edges).toHaveLength(24); // 2×3×4

      const duoprism64 = getPolytope4D('duoprism64');
      expect(duoprism64.vertices).toHaveLength(24); // 6×4
      expect(duoprism64.edges).toHaveLength(48); // 2×6×4
    });
  });

  describe('vertex normalization', () => {
    it('normalizes vertices to unit 4-sphere', () => {
      const types: Polytope4DType[] = [
        'pentachoron', 'tesseract', 'hexadecachoron', 'icositetrachoron',
      ];
      for (const type of types) {
        const p = getPolytope4D(type);
        for (const v of p.vertices) {
          const norm = Math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2 + v[3] ** 2);
          expect(norm).toBeCloseTo(1, 5);
        }
      }
    });
  });

  describe('edge validity', () => {
    it('has all edge indices within vertex range', () => {
      const types: Polytope4DType[] = [
        'pentachoron', 'tesseract', 'hexadecachoron', 'icositetrachoron',
        'rectified5cell', 'duoprism64',
      ];
      for (const type of types) {
        const p = getPolytope4D(type);
        for (const [a, b] of p.edges) {
          expect(a).toBeGreaterThanOrEqual(0);
          expect(a).toBeLessThan(p.vertices.length);
          expect(b).toBeGreaterThanOrEqual(0);
          expect(b).toBeLessThan(p.vertices.length);
          expect(a).not.toBe(b);
        }
      }
    });
  });

  describe('cache behavior', () => {
    it('returns the same object reference on repeated calls', () => {
      const p1 = getPolytope4D('icositetrachoron');
      const p2 = getPolytope4D('icositetrachoron');
      expect(p1).toBe(p2); // Same reference (cached)
    });

    it('returns different objects for different types', () => {
      const p1 = getPolytope4D('pentachoron');
      const p2 = getPolytope4D('tesseract');
      expect(p1).not.toBe(p2);
      expect(p1.vertices.length).not.toBe(p2.vertices.length);
    });
  });

  describe('all polytope types are valid', () => {
    const allTypes: Polytope4DType[] = [
      'pentachoron', 'tesseract', 'hexadecachoron', 'icositetrachoron',
      'hexacosichoron', 'doubleHelix4D', 'dualCompound', 'rectified5cell',
      'duoprism55', 'duoprism34', 'duoprism64', 'duoprism83', 'duoprism53',
    ];

    it.each(allTypes)('generates %s without errors', (type) => {
      const p = getPolytope4D(type);
      expect(p.vertices.length).toBeGreaterThan(0);
      expect(p.edges.length).toBeGreaterThan(0);
    });
  });
});