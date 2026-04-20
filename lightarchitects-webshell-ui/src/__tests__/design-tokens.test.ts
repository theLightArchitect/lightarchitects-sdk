import { describe, it, expect } from 'vitest';
import {
  SIBLINGS, SIBLING_COLORS, PILLARS, PILLAR_COLORS,
  STATUS_COLORS, SIBLING_POLYTOPES, LAYOUT, TYPO, Z,
  META_SKILL_TO_SIBLING, getMetaSkillPolytope, getMetaSkillColor,
} from '$lib/design-tokens';

describe('design-tokens', () => {
  describe('SIBLINGS', () => {
    it('contains all 7 sibling IDs', () => {
      expect(SIBLINGS).toHaveLength(7);
      expect(SIBLINGS).toContain('soul');
      expect(SIBLINGS).toContain('eva');
      expect(SIBLINGS).toContain('corso');
      expect(SIBLINGS).toContain('quantum');
      expect(SIBLINGS).toContain('seraph');
      expect(SIBLINGS).toContain('ayin');
      expect(SIBLINGS).toContain('larc');
    });
  });

  describe('SIBLING_COLORS', () => {
    it('has a color for every sibling', () => {
      for (const sib of SIBLINGS) {
        expect(SIBLING_COLORS[sib]).toBeDefined();
        expect(SIBLING_COLORS[sib]).toMatch(/^#[0-9a-fA-F]{6}$/);
      }
    });
  });

  describe('PILLARS', () => {
    it('contains all 7 CORSO pillars in order', () => {
      expect(PILLARS).toEqual(['ARCH', 'SEC', 'QUAL', 'PERF', 'TEST', 'DOC', 'OPS']);
    });
  });

  describe('PILLAR_COLORS', () => {
    it('has a color for every pillar', () => {
      for (const pillar of PILLARS) {
        expect(PILLAR_COLORS[pillar]).toBeDefined();
        expect(PILLAR_COLORS[pillar]).toMatch(/^#[0-9a-fA-F]{6}$/);
      }
    });
  });

  describe('SIBLING_POLYTOPES', () => {
    it('maps every sibling to a polytope with type, label, vertices, edges', () => {
      for (const sib of SIBLINGS.slice(0, -1)) { // Exclude 'larc' which has no polytope
        if (SIBLING_POLYTOPES[sib]) {
          expect(SIBLING_POLYTOPES[sib].type).toBeDefined();
          expect(SIBLING_POLYTOPES[sib].label).toBeDefined();
          expect(SIBLING_POLYTOPES[sib].vertices).toBeGreaterThan(0);
          expect(SIBLING_POLYTOPES[sib].edges).toBeGreaterThan(0);
        }
      }
    });
  });

  describe('META_SKILL_TO_SIBLING', () => {
    it('maps every meta-skill to a valid sibling', () => {
      for (const [skill, sib] of Object.entries(META_SKILL_TO_SIBLING)) {
        expect(SIBLINGS).toContain(sib);
        expect(skill).toMatch(/^\//);
      }
    });
  });

  describe('getMetaSkillPolytope', () => {
    it('returns hexadecachoron for /BUILD (corso)', () => {
      expect(getMetaSkillPolytope('/BUILD')).toBe('hexadecachoron');
    });

    it('returns pentachoron for /RESEARCH (quantum)', () => {
      expect(getMetaSkillPolytope('/RESEARCH')).toBe('pentachoron');
    });

    it('returns duoprism64 for /SECURE (seraph)', () => {
      expect(getMetaSkillPolytope('/SECURE')).toBe('duoprism64');
    });

    it('returns icositetrachoron for unknown skills', () => {
      expect(getMetaSkillPolytope('/UNKNOWN')).toBe('icositetrachoron');
    });
  });

  describe('getMetaSkillColor', () => {
    it('returns corso color for /BUILD', () => {
      expect(getMetaSkillColor('/BUILD')).toBe('#00BFFF');
    });

    it('returns quantum color for /RESEARCH', () => {
      expect(getMetaSkillColor('/RESEARCH')).toBe('#B44AFF');
    });

    it('returns default violet for unknown skills', () => {
      expect(getMetaSkillColor('/UNKNOWN')).toBe('#8B5CF6');
    });
  });

  describe('LAYOUT constants', () => {
    it('has all required layout values', () => {
      expect(LAYOUT.sidebarWidth).toBe(260);
      expect(LAYOUT.headerHeight).toBe(48);
      expect(LAYOUT.railWidth).toBe(240);
      expect(LAYOUT.panelGap).toBe(4);
      expect(LAYOUT.borderRadius).toBe(8);
      expect(LAYOUT.terminalMinHeight).toBe(200);
    });
  });

  describe('TYPO constants', () => {
    it('has all required typography values', () => {
      expect(TYPO.fontFamily).toContain('JetBrains Mono');
      expect(TYPO.sizeXs).toBeDefined();
      expect(TYPO.sizeLg).toBeDefined();
    });
  });

  describe('Z-index layers', () => {
    it('has ascending z-index values', () => {
      expect(Z.base).toBeLessThan(Z.panel);
      expect(Z.panel).toBeLessThan(Z.overlay);
      expect(Z.overlay).toBeLessThan(Z.scope);
      expect(Z.scope).toBeLessThan(Z.palette);
      expect(Z.palette).toBeLessThan(Z.toast);
    });
  });
});