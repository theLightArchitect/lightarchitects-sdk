import { describe, it, expect } from 'vitest';
import { matchRoute, type ScreenKey } from '$lib/routes';

describe('routes', () => {
  describe('matchRoute() — top-level routes', () => {
    it('/ → Builds', () => {
      expect(matchRoute('/').screen).toBe('Builds');
    });

    it('empty string → Builds', () => {
      expect(matchRoute('').screen).toBe('Builds');
    });

    it('/builds → Builds', () => {
      expect(matchRoute('/builds').screen).toBe('Builds');
    });

    it('/ops → Ops', () => {
      expect(matchRoute('/ops').screen).toBe('Ops');
    });

    it('/ops#activity → Ops (hash fragment passes through)', () => {
      expect(matchRoute('/ops#activity').screen).toBe('Ops');
    });

    it('/dispatch → Dispatch', () => {
      expect(matchRoute('/dispatch').screen).toBe('Dispatch');
    });

    it('/helix → Helix', () => {
      expect(matchRoute('/helix').screen).toBe('Helix');
    });
  });

  describe('matchRoute() — BuildDetail patterns', () => {
    it('/builds/:buildId → BuildDetail with buildId param', () => {
      const r = matchRoute('/builds/abc-123');
      expect(r.screen).toBe('BuildDetail');
      expect(r.params.buildId).toBe('abc-123');
    });

    it('/builds/:buildId/phase/:phaseId → BuildDetail', () => {
      const r = matchRoute('/builds/b1/phase/p2');
      expect(r.screen).toBe('BuildDetail');
      expect(r.params.buildId).toBe('b1');
      expect(r.params.phaseId).toBe('p2');
    });

    it('/builds/:buildId/phase/:phaseId/wave/:waveId → BuildDetail', () => {
      const r = matchRoute('/builds/b1/phase/p2/wave/w3');
      expect(r.screen).toBe('BuildDetail');
      expect(r.params.waveId).toBe('w3');
    });

    it('/builds/:buildId/phase/:phaseId/wave/:waveId/agent/:agentKey → BuildDetail', () => {
      const r = matchRoute('/builds/b1/phase/p2/wave/w3/agent/engineer');
      expect(r.screen).toBe('BuildDetail');
      expect(r.params.agentKey).toBe('engineer');
    });
  });

  describe('matchRoute() — Dispatch orphan run patterns', () => {
    it('/dispatch/run/:runId → Dispatch', () => {
      const r = matchRoute('/dispatch/run/run-42');
      expect(r.screen).toBe('Dispatch');
      expect(r.params.runId).toBe('run-42');
    });

    it('/dispatch/run/:runId/agent/:agentKey → Dispatch', () => {
      const r = matchRoute('/dispatch/run/run-42/agent/security');
      expect(r.screen).toBe('Dispatch');
      expect(r.params.runId).toBe('run-42');
      expect(r.params.agentKey).toBe('security');
    });
  });

  describe('matchRoute() — Helix drilldown patterns', () => {
    it('/helix/strand/:siblingKey → Helix', () => {
      const r = matchRoute('/helix/strand/soul');
      expect(r.screen).toBe('Helix');
      expect(r.params.siblingKey).toBe('soul');
    });

    it('/helix/entry/:entryId → Helix', () => {
      const r = matchRoute('/helix/entry/entry-99');
      expect(r.screen).toBe('Helix');
      expect(r.params.entryId).toBe('entry-99');
    });
  });

  describe('matchRoute() — legacy workspace aliases', () => {
    it('/workspace → BuildDetail', () => {
      expect(matchRoute('/workspace').screen).toBe('BuildDetail');
    });

    it('/workspace/:buildId → BuildDetail with buildId', () => {
      const r = matchRoute('/workspace/proj-7');
      expect(r.screen).toBe('BuildDetail');
      expect(r.params.buildId).toBe('proj-7');
    });
  });

  describe('matchRoute() — ProjectDetail', () => {
    it('/project/:projectId → ProjectDetail', () => {
      const r = matchRoute('/project/proj-alpha');
      expect(r.screen).toBe('ProjectDetail');
      expect(r.params.projectId).toBe('proj-alpha');
    });
  });

  describe('matchRoute() — # prefix stripping', () => {
    it('strips leading # before matching', () => {
      expect(matchRoute('#/builds').screen).toBe('Builds');
      expect(matchRoute('#/ops').screen).toBe('Ops');
    });
  });

  describe('matchRoute() — query string stripping', () => {
    it('strips ?view= query before matching', () => {
      const r = matchRoute('/builds/abc?view=kanban');
      expect(r.screen).toBe('BuildDetail');
      expect(r.params.buildId).toBe('abc');
    });
  });

  describe('matchRoute() — unknown paths fall back to Builds', () => {
    it('/unknown → Builds fallback', () => {
      expect(matchRoute('/unknown').screen).toBe('Builds');
    });

    it('/builds/b/phase/p/wave/w/agent/a/extra → Builds fallback (too deep)', () => {
      const r = matchRoute('/builds/b/phase/p/wave/w/agent/a/extra');
      expect(r.screen).toBe('Builds');
    });
  });

  describe('route specificity — longer patterns match before shorter', () => {
    it('/builds/b/phase/p/wave/w/agent/a does NOT match /builds/:buildId', () => {
      const r = matchRoute('/builds/b/phase/p/wave/w/agent/a');
      expect(r.screen).toBe('BuildDetail');
      expect(r.params.agentKey).toBe('a');
      expect(r.params.buildId).toBe('b');
    });
  });
});
