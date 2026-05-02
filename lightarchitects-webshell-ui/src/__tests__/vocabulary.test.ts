import { describe, it, expect } from 'vitest';
import { TERMS, NAV_LABELS, TOOLTIPS, t, tip } from '$lib/vocabulary';

describe('vocabulary', () => {
  describe('TERMS', () => {
    it('maps Pillar variants to Quality Gate', () => {
      expect(TERMS['Pillar']).toBe('Quality Gate');
      expect(TERMS['Pillars']).toBe('Quality Gates');
      expect(TERMS['PILLAR']).toBe('QUALITY GATE');
      expect(TERMS['PILLARS']).toBe('QUALITY GATES');
      expect(TERMS['pillar']).toBe('quality gate');
      expect(TERMS['pillars']).toBe('quality gates');
    });

    it('maps Sibling variants to Agent', () => {
      expect(TERMS['Sibling']).toBe('Agent');
      expect(TERMS['Siblings']).toBe('Agents');
      expect(TERMS['sibling']).toBe('agent');
      expect(TERMS['siblings']).toBe('agents');
    });
  });

  describe('NAV_LABELS', () => {
    it('has all four navigation tabs in uppercase', () => {
      expect(NAV_LABELS.ops).toBe('OPS');
      expect(NAV_LABELS.dispatch).toBe('DISPATCH');
      expect(NAV_LABELS.builds).toBe('BUILDS');
      expect(NAV_LABELS.helix).toBe('HELIX');
    });
  });

  describe('TOOLTIPS', () => {
    it('has tooltip text for all defined terms', () => {
      const expectedTerms = ['MCP', 'Skill', 'Helix', 'Wave', 'Phase', 'Rail', 'LASDLC', 'Arena'];
      for (const term of expectedTerms) {
        expect(TOOLTIPS[term]).toBeDefined();
        expect(TOOLTIPS[term].length).toBeGreaterThan(0);
      }
    });
  });

  describe('t()', () => {
    it('returns mapped value for known terms', () => {
      expect(t('Pillar')).toBe('Quality Gate');
      expect(t('sibling')).toBe('agent');
    });

    it('passes through unknown keys unchanged', () => {
      expect(t('Unknown')).toBe('Unknown');
      expect(t('foobar')).toBe('foobar');
      expect(t('')).toBe('');
    });
  });

  describe('tip()', () => {
    it('returns tooltip string for known terms', () => {
      const result = tip('MCP');
      expect(result).toBeDefined();
      expect(typeof result).toBe('string');
      expect(result!.length).toBeGreaterThan(0);
    });

    it('returns undefined for unknown terms', () => {
      expect(tip('UnknownTerm')).toBeUndefined();
      expect(tip('')).toBeUndefined();
    });
  });
});
