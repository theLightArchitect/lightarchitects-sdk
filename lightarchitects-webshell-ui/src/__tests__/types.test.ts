import { describe, it, expect } from 'vitest';
import {
  PILLARS, PILLAR_ACTIONS, META_SKILLS, SIBLINGS,
  SiblingWave, BUF_LEN, DECAY,
  type Pillar, type MetaSkill, type PillarStatus,
} from '$lib/types';

describe('types', () => {
  describe('PILLARS', () => {
    it('has exactly 7 pillars in CORSO order', () => {
      expect(PILLARS).toHaveLength(7);
      expect(PILLARS).toEqual(['ARCH', 'SEC', 'QUAL', 'PERF', 'TEST', 'DOC', 'OPS']);
    });
  });

  describe('PILLAR_ACTIONS', () => {
    it('has actions for every meta-skill', () => {
      for (const skill of META_SKILLS) {
        expect(PILLAR_ACTIONS[skill]).toBeDefined();
        for (const pillar of PILLARS) {
          expect(PILLAR_ACTIONS[skill][pillar]).toBeDefined();
          expect(typeof PILLAR_ACTIONS[skill][pillar]).toBe('string');
          expect(PILLAR_ACTIONS[skill][pillar].length).toBeGreaterThan(0);
        }
      }
    });

    it('uses SCOUT for /BUILD ARCH phase', () => {
      expect(PILLAR_ACTIONS['/BUILD'].ARCH).toBe('SCOUT');
    });

    it('uses RECON for /SECURE ARCH phase', () => {
      expect(PILLAR_ACTIONS['/SECURE'].ARCH).toBe('RECON');
    });

    it('uses SCAN for /RESEARCH ARCH phase', () => {
      expect(PILLAR_ACTIONS['/RESEARCH'].ARCH).toBe('SCAN');
    });

    it('OPS always ends with SCRUM or CLOSE', () => {
      for (const skill of META_SKILLS) {
        const ops = PILLAR_ACTIONS[skill].OPS;
        expect(['SCRUM', 'CLOSE']).toContain(ops);
      }
    });
  });

  describe('META_SKILLS', () => {
    it('has exactly 12 canonical meta-skills', () => {
      // 12 canonical SQUAD meta-skills. PILLAR_ACTIONS additionally contains
      // a `/USING-SKILLS` fallback map that isn't in the MetaSkill type union.
      expect(META_SKILLS).toHaveLength(12);
    });

    it('all meta-skills start with /', () => {
      for (const skill of META_SKILLS) {
        expect(skill).toMatch(/^\//);
      }
    });
  });

  describe('SIBLINGS', () => {
    it('has exactly 7 siblings', () => {
      expect(SIBLINGS).toHaveLength(7);
    });
  });

  describe('SiblingWave', () => {
    it('initializes with zeroed samples', () => {
      const wave = new SiblingWave();
      expect(wave.samples).toHaveLength(BUF_LEN);
      expect(wave.samples.every(s => s === 0)).toBe(true);
      expect(wave.activity).toBe(0);
    });

    it('spikes activity to 1.0', () => {
      const wave = new SiblingWave();
      wave.spike();
      expect(wave.activity).toBe(1.0);
    });

    it('decays activity on tick', () => {
      const wave = new SiblingWave();
      wave.spike();
      const before = wave.activity;
      wave.tick();
      expect(wave.activity).toBeLessThan(before);
      expect(wave.activity).toBeCloseTo(before * DECAY, 5);
    });

    it('reports not active when activity is near zero', () => {
      const wave = new SiblingWave();
      expect(wave.isActive()).toBe(false);
    });

    it('reports active after spike', () => {
      const wave = new SiblingWave();
      wave.spike();
      expect(wave.isActive()).toBe(true);
    });

    it('pushes new sample and shifts on tick', () => {
      const wave = new SiblingWave();
      wave.spike();
      wave.tick();
      // After spike + tick, the last sample should be non-zero
      expect(wave.samples[BUF_LEN - 1]).not.toBe(0);
    });
  });

  describe('Type validation', () => {
    it('Pillar type accepts all 7 pillars', () => {
      const pillars: Pillar[] = ['ARCH', 'SEC', 'QUAL', 'PERF', 'TEST', 'DOC', 'OPS'];
      expect(pillars).toHaveLength(7);
    });

    it('PillarStatus type accepts all status values', () => {
      const statuses: PillarStatus[] = ['pending', 'in_progress', 'passed', 'failed', 'blocked'];
      expect(statuses).toHaveLength(5);
    });

    it('MetaSkill type covers all defined skills', () => {
      for (const skill of META_SKILLS) {
        const typed: MetaSkill = skill;
        expect(typed).toBeDefined();
      }
    });
  });
});