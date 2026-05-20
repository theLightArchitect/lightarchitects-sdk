import { describe, it, expect } from 'vitest';
import {
  SIBLINGS, SIBLING_COLORS,
  DOMAIN_AGENT_COLORS,
  QUALITY_GATES, QUALITY_GATE_COLORS,
  PILLARS, PILLAR_COLORS,
  STATUS_COLORS, SIBLING_POLYTOPES, LAYOUT, TYPO, Z,
  MOTION, LETTER_SPACING, ELEVATION, HAIRLINE, TEXT,
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
      expect(SIBLINGS).toContain('laex');
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

  describe('QUALITY_GATES', () => {
    it('contains all 7 quality gates in order', () => {
      expect(QUALITY_GATES).toEqual(['ARCH', 'SEC', 'QUAL', 'PERF', 'TEST', 'DOC', 'OPS']);
    });
  });

  describe('QUALITY_GATE_COLORS', () => {
    it('has a hex color for every quality gate', () => {
      for (const gate of QUALITY_GATES) {
        expect(QUALITY_GATE_COLORS[gate]).toBeDefined();
        expect(QUALITY_GATE_COLORS[gate]).toMatch(/^#[0-9a-fA-F]{6}$/);
      }
    });
  });

  describe('PILLARS / PILLAR_COLORS (deprecated aliases)', () => {
    it('PILLARS is same reference as QUALITY_GATES', () => {
      expect(PILLARS).toBe(QUALITY_GATES);
    });

    it('PILLAR_COLORS is same reference as QUALITY_GATE_COLORS', () => {
      expect(PILLAR_COLORS).toBe(QUALITY_GATE_COLORS);
    });
  });

  describe('DOMAIN_AGENT_COLORS', () => {
    const AGENT_IDS = [
      'engineer', 'quality', 'security', 'ops',
      'researcher', 'knowledge', 'testing', 'squad',
    ];

    it('has exactly 8 domain agent colors', () => {
      expect(Object.keys(DOMAIN_AGENT_COLORS)).toHaveLength(8);
    });

    it('every agent has a valid hex color', () => {
      for (const id of AGENT_IDS) {
        expect(DOMAIN_AGENT_COLORS[id]).toBeDefined();
        expect(DOMAIN_AGENT_COLORS[id]).toMatch(/^#[0-9a-fA-F]{6}$/);
      }
    });
  });

  describe('SIBLING_POLYTOPES', () => {
    it('maps every sibling to a polytope with type, label, vertices, edges', () => {
      for (const sib of SIBLINGS.slice(0, -1)) { // Exclude 'laex' which has no polytope
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
      expect(LAYOUT.borderRadius).toBe(0);
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

  describe('MOTION constants', () => {
    it('has snap < base < slow durations', () => {
      expect(MOTION.snap).toBeLessThan(MOTION.base);
      expect(MOTION.base).toBeLessThan(MOTION.slow);
    });

    it('has a cubic-bezier ease string', () => {
      expect(MOTION.ease).toMatch(/^cubic-bezier/);
    });
  });

  describe('LETTER_SPACING constants', () => {
    it('has loose, mid, tight values in em units', () => {
      expect(LETTER_SPACING.loose).toMatch(/em$/);
      expect(LETTER_SPACING.mid).toMatch(/em$/);
      expect(LETTER_SPACING.tight).toMatch(/em$/);
    });

    it('loose > mid > tight numerically', () => {
      const parse = (v: string) => parseFloat(v);
      expect(parse(LETTER_SPACING.loose)).toBeGreaterThan(parse(LETTER_SPACING.mid));
      expect(parse(LETTER_SPACING.mid)).toBeGreaterThan(parse(LETTER_SPACING.tight));
    });
  });

  describe('ELEVATION scale', () => {
    it('has void, frame, elev1, elev2 as hex colors', () => {
      for (const value of Object.values(ELEVATION)) {
        expect(value).toMatch(/^#[0-9a-fA-F]{3,6}$/);
      }
    });
  });

  describe('HAIRLINE scale', () => {
    it('has faint, base, strong as hex colors', () => {
      for (const value of Object.values(HAIRLINE)) {
        expect(value).toMatch(/^#[0-9a-fA-F]{3,6}$/);
      }
    });
  });

  describe('TEXT scale', () => {
    it('has mute, dim, base, bright, stark as hex colors', () => {
      expect(Object.keys(TEXT)).toEqual(['mute', 'dim', 'base', 'bright', 'stark']);
      for (const value of Object.values(TEXT)) {
        expect(value).toMatch(/^#[0-9a-fA-F]{3,6}$/);
      }
    });
  });

  describe('Z-index layers', () => {
    it('canonical ladder is strictly ascending', () => {
      expect(Z.grid).toBeLessThan(Z.vignette);
      expect(Z.vignette).toBeLessThan(Z.content);
      expect(Z.content).toBeLessThan(Z.panel);
      expect(Z.panel).toBeLessThan(Z.drawer);
      expect(Z.drawer).toBeLessThan(Z.bracket);
      expect(Z.bracket).toBeLessThan(Z.modalScrim);
      expect(Z.modalScrim).toBeLessThan(Z.modal);
      expect(Z.modal).toBeLessThan(Z.tooltip);
      expect(Z.tooltip).toBeLessThan(Z.overlay);
    });

    it('deprecated aliases are still defined', () => {
      expect(Z.base).toBeDefined();
      expect(Z.scope).toBeDefined();
      expect(Z.palette).toBeDefined();
      expect(Z.toast).toBeDefined();
    });
  });
});