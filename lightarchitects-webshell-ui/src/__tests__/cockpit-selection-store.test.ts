/**
 * cockpit-selection-store.test.ts
 *
 * Unit coverage for the polymorphic selection store (Phase 5):
 *   - select() sets the store value
 *   - select() enforces scope guards (d0 rejects 'crate', d3 rejects 'pr')
 *   - clearOnScopeChange() resets to { kind: 'none' }
 *   - clearSelection() resets to { kind: 'none' }
 *   - Multiple successive selects update correctly
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import {
  selection,
  select,
  clearSelection,
  clearOnScopeChange,
  type Selection,
} from '$lib/cockpit/stores/selection';
import type { RouteScope } from '$lib/cockpit/stores/scope';

const d0Scope: RouteScope = { kind: 'platform', depth: 0 };
const d1Scope: RouteScope = { kind: 'project',  depth: 1, project_id: 'test-project' };
const d2Scope: RouteScope = { kind: 'build',    depth: 2, codename: 'test-build' };
const d3Scope: RouteScope = { kind: 'file',     depth: 3, codename: 'test-build', file_path: 'src/lib/foo.ts' };

function currentSelection(): Selection { return get(selection); }

describe('cockpit selection store', () => {
  beforeEach(() => { clearSelection(); });

  describe('initial state', () => {
    it('starts as { kind: "none" }', () => {
      expect(currentSelection().kind).toBe('none');
    });
  });

  describe('select()', () => {
    it('sets build selection at d0 scope', () => {
      select({ kind: 'build', codename: 'scope-e2e' }, d0Scope);
      const s = currentSelection();
      expect(s.kind).toBe('build');
      if (s.kind === 'build') expect(s.codename).toBe('scope-e2e');
    });

    it('sets build selection at d1 scope', () => {
      select({ kind: 'build', codename: 'my-build' }, d1Scope);
      expect(currentSelection().kind).toBe('build');
    });

    it('sets worker selection at d2 scope', () => {
      select({ kind: 'worker', worker_id: 'w-1', build_codename: 'my-build' }, d2Scope);
      const s = currentSelection();
      expect(s.kind).toBe('worker');
      if (s.kind === 'worker') {
        expect(s.worker_id).toBe('w-1');
        expect(s.build_codename).toBe('my-build');
      }
    });

    it('sets decision selection', () => {
      select({ kind: 'decision', decision_id: '42', build_codename: 'my-build' }, d2Scope);
      const s = currentSelection();
      expect(s.kind).toBe('decision');
      if (s.kind === 'decision') expect(s.decision_id).toBe('42');
    });

    it('sets pr selection at d1 scope', () => {
      select({ kind: 'pr', owner: 'lightarchitects', repo: 'sdk', number: 7 }, d1Scope);
      const s = currentSelection();
      expect(s.kind).toBe('pr');
      if (s.kind === 'pr') expect(s.number).toBe(7);
    });

    it('sets crate selection at d2 scope', () => {
      select({ kind: 'crate', name: 'tokio' }, d2Scope);
      expect(currentSelection().kind).toBe('crate');
    });

    it('successive selects overwrite each other', () => {
      select({ kind: 'build', codename: 'a' }, d1Scope);
      select({ kind: 'build', codename: 'b' }, d1Scope);
      const s = currentSelection();
      if (s.kind === 'build') expect(s.codename).toBe('b');
    });
  });

  describe('scope guards', () => {
    it('rejects "pr" selection at d3 (file) scope', () => {
      select({ kind: 'pr', owner: 'o', repo: 'r', number: 1 }, d3Scope);
      expect(currentSelection().kind).toBe('none');
    });

    it('rejects "crate" selection at d0 (platform) scope', () => {
      select({ kind: 'crate', name: 'serde' }, d0Scope);
      expect(currentSelection().kind).toBe('none');
    });

    it('allows "pr" at d0, d1, d2 scopes', () => {
      for (const scope of [d0Scope, d1Scope, d2Scope]) {
        clearSelection();
        select({ kind: 'pr', owner: 'o', repo: 'r', number: 1 }, scope);
        expect(currentSelection().kind).toBe('pr');
      }
    });

    it('allows "crate" at d1, d2, d3 scopes', () => {
      for (const scope of [d1Scope, d2Scope, d3Scope]) {
        clearSelection();
        select({ kind: 'crate', name: 'tokio' }, scope);
        expect(currentSelection().kind).toBe('crate');
      }
    });

    it('scope guard is a no-op — prior selection is preserved on guard fail', () => {
      select({ kind: 'build', codename: 'existing' }, d2Scope);
      // attempt invalid selection — should be silently rejected
      select({ kind: 'crate', name: 'serde' }, d0Scope);
      // still the previous selection
      const s = currentSelection();
      expect(s.kind).toBe('build');
    });
  });

  describe('clearSelection()', () => {
    it('resets to { kind: "none" } from any state', () => {
      select({ kind: 'build', codename: 'x' }, d1Scope);
      clearSelection();
      expect(currentSelection().kind).toBe('none');
    });
  });

  describe('clearOnScopeChange()', () => {
    it('resets to { kind: "none" }', () => {
      select({ kind: 'worker', worker_id: 'w-2', build_codename: 'y' }, d2Scope);
      clearOnScopeChange();
      expect(currentSelection().kind).toBe('none');
    });

    it('is idempotent — safe to call when already none', () => {
      clearOnScopeChange();
      clearOnScopeChange();
      expect(currentSelection().kind).toBe('none');
    });
  });

  describe('FocusRouter card-role contract', () => {
    it('FocusRouter.svelte carries data-card-role="focus-router"', async () => {
      const { readFileSync } = await import('fs');
      const { join } = await import('path');
      const src = readFileSync(
        join(import.meta.dirname, '../lib/cockpit/focus/FocusRouter.svelte'),
        'utf-8',
      );
      expect(src).toContain('data-card-role="focus-router"');
    });

    it('all 8 focus panel components are imported by FocusRouter', async () => {
      const { readFileSync } = await import('fs');
      const { join } = await import('path');
      const src = readFileSync(
        join(import.meta.dirname, '../lib/cockpit/focus/FocusRouter.svelte'),
        'utf-8',
      );
      const PANELS = ['BuildFocusPanel', 'WorkerFocusPanel', 'EscalationFocusPanel',
        'SpanFocusPanel', 'GateFocusPanel', 'DecisionFocusPanel', 'PrFocusPanel', 'CrateFocusPanel'];
      for (const panel of PANELS) {
        expect(src, `FocusRouter missing import of ${panel}`).toContain(panel);
      }
    });
  });
});
