import { describe, it, expect } from 'vitest';
import { matchRoute, type ScreenKey } from '$lib/routes';

describe('routes', () => {
  describe('matchRoute() — top-level routes', () => {
    it('/ → Ops', () => {
      expect(matchRoute('/').screen).toBe('Ops');
    });

    it('empty string → Ops', () => {
      expect(matchRoute('').screen).toBe('Ops');
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

  describe('legacy workspace aliases — now handled via REDIRECTS, not match', () => {
    // Wave 1 (2026-05-02) moved /workspace from ROUTES into REDIRECTS so that
    // a hard-coded redirect rewrites the URL to /builds before matching runs.
    // matchRoute() on a bare /workspace path therefore falls through to the
    // default Ops screen — by design. The user-visible deep link is still
    // preserved because applyRedirects() (run on every hashchange in app.svelte)
    // rewrites #/workspace/:id to #/builds/:id before this matcher is called.
    it('/workspace falls through to Ops default (redirect handles user-visible navigation)', () => {
      expect(matchRoute('/workspace').screen).toBe('Ops');
    });

    it('/workspace/:buildId falls through, but redirect rewrites it to /builds/:buildId before matchRoute is called', () => {
      // Direct match (no redirect): falls through to default Ops.
      expect(matchRoute('/workspace/proj-7').screen).toBe('Ops');
      // After applyRedirects(), the URL becomes /builds/proj-7 and resolves correctly.
      // (applyRedirects has DOM side-effects so it's tested in the e2e suite, not here.)
      expect(matchRoute('/builds/proj-7').screen).toBe('BuildDetail');
      expect(matchRoute('/builds/proj-7').params.buildId).toBe('proj-7');
    });
  });

  describe('matchRoute() — view-encoded BuildDetail (Wave 1)', () => {
    const VIEW_MODES = ['kanban', 'list', 'operator', 'manifest', 'plan', 'comms', 'pipeline', 'autonomous', 'decisions'] as const;

    for (const view of VIEW_MODES) {
      it(`/builds/:buildId/${view} → BuildDetail with buildId + view params`, () => {
        const r = matchRoute(`/builds/proj-7/${view}`);
        expect(r.screen).toBe('BuildDetail');
        expect(r.params.buildId).toBe('proj-7');
        expect(r.params.view).toBe(view);
      });
    }

    it('rejects unknown view names (falls back to /builds/:buildId pattern)', () => {
      // /builds/proj-7/bogus does NOT match the view-enum regex; falls through
      // to the next route which is /builds/:buildId. That regex has [^/]+ so it
      // would match "proj-7/bogus" if greedy — but [^/]+ stops at /, so the
      // pattern won't match a 2-segment tail and falls through to default Ops.
      const r = matchRoute('/builds/proj-7/bogus');
      expect(r.screen).toBe('Ops'); // default fallthrough
    });

    it('phase routes still take precedence over view-encoded route', () => {
      // /builds/:buildId/phase/:phaseId is more-specific and listed earlier
      const r = matchRoute('/builds/proj-7/phase/p2');
      expect(r.screen).toBe('BuildDetail');
      expect(r.params.phaseId).toBe('p2');
      expect(r.params.view).toBeUndefined();
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

  describe('matchRoute() — unknown paths fall back to Ops', () => {
    it('/unknown → Ops fallback', () => {
      expect(matchRoute('/unknown').screen).toBe('Ops');
    });

    it('/builds/b/phase/p/wave/w/agent/a/extra → Ops fallback (too deep)', () => {
      const r = matchRoute('/builds/b/phase/p/wave/w/agent/a/extra');
      expect(r.screen).toBe('Ops');
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
