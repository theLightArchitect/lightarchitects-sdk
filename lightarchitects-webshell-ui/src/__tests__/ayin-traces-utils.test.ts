/**
 * Unit tests for the Phase 3 (copilot-ayin-instrumentation) changes:
 *   - TraceSpan.parent_id field (renamed from parent_span_id)
 *   - buildSequenceDiagram: ACTIVATE/DEACTIVATE for parent-child spans
 *   - buildFlowDiagram: parent_id edge graph instead of positional adjacency
 *   - ayinStatus store observable (G9 — button conditionality contract)
 */
import { describe, it, expect } from 'vitest';
import { get } from 'svelte/store';
import { buildSequenceDiagram, buildFlowDiagram } from '$lib/ayin-traces-utils';
import type { TraceSpan } from '$lib/ayin-traces-utils';
import { ayinStatus } from '$lib/stores';

function span(overrides: Partial<TraceSpan> = {}): TraceSpan {
  return {
    trace_id: 't1',
    span_id: 's1',
    actor: 'webshell',
    action: 'copilot.session.started',
    timestamp: '2026-05-25T00:00:00Z',
    duration_ms: 50,
    outcome: 'Continue',
    ...overrides,
  };
}

// ── G1: buildSequenceDiagram emits activate/deactivate for a parent span ──────

describe('buildSequenceDiagram — parent-child ACTIVATE/DEACTIVATE', () => {
  it('G1: parent span with a child emits activate + deactivate', () => {
    const root = span({ span_id: 's0', action: 'copilot.session.started' });
    const child = span({ span_id: 's1', parent_id: 's0', action: 'copilot.turn.started' });
    const dsl = buildSequenceDiagram([root, child]);
    expect(dsl).toContain('activate webshell');
    expect(dsl).toContain('deactivate webshell');
  });

  it('G2: root span without children has no activate/deactivate', () => {
    const root = span({ span_id: 's0', action: 'copilot.session.started' });
    const dsl = buildSequenceDiagram([root]);
    expect(dsl).not.toContain('activate');
    expect(dsl).not.toContain('deactivate');
  });

  it('child note appears between activate/deactivate lines', () => {
    const root = span({ span_id: 's0', action: 'session' });
    const child = span({ span_id: 's1', parent_id: 's0', action: 'turn' });
    const dsl = buildSequenceDiagram([root, child]);
    const activateIdx = dsl.indexOf('activate');
    const childNoteIdx = dsl.indexOf('Note over webshell: turn');
    const deactivateIdx = dsl.indexOf('deactivate');
    expect(activateIdx).toBeGreaterThan(-1);
    expect(childNoteIdx).toBeGreaterThan(activateIdx);
    expect(deactivateIdx).toBeGreaterThan(childNoteIdx);
  });

  it('orphaned child (parent not in view) is treated as root — no activate', () => {
    const orphan = span({ span_id: 's1', parent_id: 'unknown-parent', action: 'turn' });
    const dsl = buildSequenceDiagram([orphan]);
    expect(dsl).not.toContain('activate');
    expect(dsl).toContain('Note over webshell: turn');
  });
});

// ── G3–G8: buildFlowDiagram parent_id edge graph ──────────────────────────────

describe('buildFlowDiagram — parent_id edges', () => {
  it('G3: child with parent_id in view emits --> edge', () => {
    const root = span({ span_id: 's0', action: 'session', outcome: 'Continue' });
    const child = span({ span_id: 's1', parent_id: 's0', action: 'turn', outcome: 'Continue' });
    const dsl = buildFlowDiagram([root, child]);
    expect(dsl).toContain('N0');
    expect(dsl).toContain('N1');
    // match either --> or -.-> (solid or dashed — depends on outcome)
    expect(dsl).toMatch(/N0\s*-[-.]*>/);
  });

  it('G4: orphaned child (parent not in array) renders as root node', () => {
    const orphan = span({ span_id: 's1', parent_id: 'missing', action: 'turn' });
    const dsl = buildFlowDiagram([orphan]);
    expect(dsl).toContain('(["');
    expect(dsl).not.toMatch(/N\d+\s*-+>/);
  });

  it('G5: Finish outcome uses solid --> edge with duration label', () => {
    const root = span({ span_id: 's0', action: 'session', outcome: 'Continue' });
    const child = span({
      span_id: 's1', parent_id: 's0',
      action: 'turn', outcome: 'Finish', duration_ms: 120,
    });
    const dsl = buildFlowDiagram([root, child]);
    expect(dsl).toContain('-->|120ms|');
    expect(dsl).not.toContain('-.-');
  });

  it('G6: non-Finish outcome uses dashed -.-> edge', () => {
    const root = span({ span_id: 's0', action: 'session', outcome: 'Continue' });
    const child = span({
      span_id: 's1', parent_id: 's0',
      action: 'turn', outcome: 'Continue',
    });
    const dsl = buildFlowDiagram([root, child]);
    expect(dsl).toContain('-.->');
  });

  it('G7: TraceSpan.parent_id field is optional — compiles with null/undefined', () => {
    const noParent: TraceSpan = span({ parent_id: undefined });
    const withNull: TraceSpan = span({ parent_id: null });
    expect(noParent.parent_id).toBeUndefined();
    expect(withNull.parent_id).toBeNull();
  });

  it('G8: single root span renders as stadium node (["label"])', () => {
    const root = span({ span_id: 's0', action: 'session' });
    const dsl = buildFlowDiagram([root]);
    expect(dsl).toContain('(["');
    expect(dsl).not.toMatch(/N\d+\s*-+>/);
  });

  it('two-pass ordering: child processed before parent still produces correct edge', () => {
    const child = span({ span_id: 's1', parent_id: 's0', action: 'turn', outcome: 'Finish', duration_ms: 10 });
    const root = span({ span_id: 's0', action: 'session', outcome: 'Continue' });
    const dsl = buildFlowDiagram([child, root]);
    expect(dsl).toContain('-->|10ms|');
    expect(dsl).not.toContain('evil');
  });
});

// ── G9: ayinStatus store observable (button conditionality contract) ──────────

describe('G9: ayinStatus store — AYIN button conditionality', () => {
  it('initial status is reconnecting', () => {
    expect(get(ayinStatus)).toBe('reconnecting');
  });

  it('can be set to connected', () => {
    ayinStatus.set('connected');
    expect(get(ayinStatus)).toBe('connected');
    ayinStatus.set('reconnecting'); // restore
  });

  it('can be set to offline', () => {
    ayinStatus.set('offline');
    expect(get(ayinStatus)).toBe('offline');
    ayinStatus.set('reconnecting'); // restore
  });
});
