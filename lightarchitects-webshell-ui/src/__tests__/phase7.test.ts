import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import {
  intakeForm, META_SKILL_CARDS, builds,
} from '$lib/stores';
import { META_SKILLS, PILLAR_ACTIONS, PILLARS, SIBLINGS } from '$lib/types';
import { getMetaSkillColor, getMetaSkillPolytope, SIBLING_COLORS, META_SKILL_TO_SIBLING } from '$lib/design-tokens';
import type { MetaSkill, IntakeSource, Priority } from '$lib/types';

describe('Phase 7: Intake + Meta-Skill Selection', () => {
  beforeEach(() => {
    intakeForm.set({
      metaSkill: '/BUILD',
      source: 'manual',
      priority: 'medium',
      repoPath: '',
      description: '',
    });
  });

  describe('intakeForm store', () => {
    it('initializes with defaults', () => {
      const form = get(intakeForm);
      expect(form.metaSkill).toBe('/BUILD');
      expect(form.source).toBe('manual');
      expect(form.priority).toBe('medium');
      expect(form.repoPath).toBe('');
    });

    it('can update metaSkill', () => {
      intakeForm.update(f => ({ ...f, metaSkill: '/SECURE' }));
      expect(get(intakeForm).metaSkill).toBe('/SECURE');
    });

    it('can update source', () => {
      intakeForm.update(f => ({ ...f, source: 'github' }));
      expect(get(intakeForm).source).toBe('github');
    });

    it('can update priority', () => {
      intakeForm.update(f => ({ ...f, priority: 'high' }));
      expect(get(intakeForm).priority).toBe('high');
    });

    it('can update repoPath', () => {
      intakeForm.update(f => ({ ...f, repoPath: 'TheLightArchitects/corso' }));
      expect(get(intakeForm).repoPath).toBe('TheLightArchitects/corso');
    });

    it('can update description', () => {
      intakeForm.update(f => ({ ...f, description: 'Fix auth middleware' }));
      expect(get(intakeForm).description).toBe('Fix auth middleware');
    });

    it('valid source types', () => {
      const validSources: IntakeSource[] = ['manual', 'github', 'audit', 'discovery'];
      for (const src of validSources) {
        intakeForm.update(f => ({ ...f, source: src }));
        expect(get(intakeForm).source).toBe(src);
      }
    });

    it('valid priority levels', () => {
      const validPriorities: Priority[] = ['high', 'medium', 'low'];
      for (const p of validPriorities) {
        intakeForm.update(f => ({ ...f, priority: p }));
        expect(get(intakeForm).priority).toBe(p);
      }
    });
  });

  describe('META_SKILL_CARDS', () => {
    it('has a card for every meta-skill', () => {
      expect(META_SKILL_CARDS.length).toBe(META_SKILLS.length);
      for (const skill of META_SKILLS) {
        const card = META_SKILL_CARDS.find(c => c.skill === skill);
        expect(card).toBeDefined();
      }
    });

    it('each card has required fields', () => {
      for (const card of META_SKILL_CARDS) {
        expect(card.skill).toBeTruthy();
        expect(card.label).toBeTruthy();
        expect(card.description).toBeTruthy();
        expect(card.sibling).toBeTruthy();
        expect(SIBLINGS).toContain(card.sibling);
      }
    });

    it('each card has 7 pillar actions', () => {
      for (const card of META_SKILL_CARDS) {
        expect(Object.keys(card.pillarActions)).toHaveLength(7);
        for (const pillar of PILLARS) {
          expect(card.pillarActions[pillar]).toBeDefined();
        }
      }
    });

    it('card labels match skill names without slash', () => {
      for (const card of META_SKILL_CARDS) {
        expect(card.label).toBe(card.skill.replace('/', ''));
      }
    });

    it('each card maps to a valid sibling color', () => {
      for (const card of META_SKILL_CARDS) {
        const color = SIBLING_COLORS[card.sibling];
        expect(color).toBeTruthy();
        expect(color).toMatch(/^#[0-9a-fA-F]{6}$/);
      }
    });

    it('card siblings match META_SKILL_TO_SIBLING mapping', () => {
      for (const card of META_SKILL_CARDS) {
        const expected = META_SKILL_TO_SIBLING[card.skill];
        expect(card.sibling).toBe(expected);
      }
    });
  });

  describe('Meta-skill to polytope mapping', () => {
    it('every meta-skill has a valid polytope', () => {
      for (const skill of META_SKILLS) {
        const polyType = getMetaSkillPolytope(skill);
        const polyColor = getMetaSkillColor(skill);
        expect(polyType).toBeTruthy();
        expect(polyColor).toMatch(/^#[0-9a-fA-F]{6}$/);
      }
    });
  });

  describe('Build creation flow', () => {
    it('can add a build to the builds store', () => {
      const initialCount = get(builds).length;
      builds.update(b => [...b, {
        id: 'build-test',
        workspaceId: 'ws-001',
        name: 'Test Build',
        metaSkill: '/BUILD',
        status: 'queued',
        pillars: [],
        currentPillar: 'ARCH',
        confidence: 0,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        modules: [],
        siblingDispatches: [],
      }]);
      expect(get(builds).length).toBe(initialCount + 1);
      // Clean up
      builds.update(b => b.filter(x => x.id !== 'build-test'));
    });
  });

  describe('Component imports', () => {
    it('Intake screen imports successfully', async () => {
      const mod = await import('$lib/../screens/Intake.svelte');
      expect(mod.default).toBeDefined();
    });
  });

  describe('PILLAR_ACTIONS consistency', () => {
    it('every meta-skill has pillar actions for all 7 pillars', () => {
      for (const skill of META_SKILLS) {
        const actions = PILLAR_ACTIONS[skill];
        expect(actions).toBeDefined();
        for (const pillar of PILLARS) {
          expect(actions[pillar]).toBeDefined();
          expect(typeof actions[pillar]).toBe('string');
        }
      }
    });
  });
});