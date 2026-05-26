import { describe, it, expect } from 'vitest';
import DOMPurify from 'dompurify';
import {
  sanitize,
  coerceDuration,
  buildSequenceDiagram,
  buildFlowDiagram,
} from '$lib/ayin-traces-utils';
import type { TraceSpan } from '$lib/ayin-traces-utils';

function span(overrides: Partial<TraceSpan> = {}): TraceSpan {
  return {
    trace_id: 't1', span_id: 's1',
    actor: 'CORSO', action: 'build',
    timestamp: '2026-05-14T00:00:00Z',
    duration_ms: 100, outcome: 'Continue',
    ...overrides,
  };
}

// ── Unit: duration_ms coercion ─────────────────────────────────────────────

describe('coerceDuration', () => {
  it('returns 0 for null', () => expect(coerceDuration(null)).toBe(0));
  it('returns 0 for undefined', () => expect(coerceDuration(undefined)).toBe(0));
  it('returns 0 for NaN', () => expect(coerceDuration(NaN)).toBe(0));
  it('returns 0 for plain object', () => expect(coerceDuration({})).toBe(0));
  it('returns 0 for non-numeric string', () => expect(coerceDuration('abc')).toBe(0));
  it('returns 0 for DSL injection attempt', () =>
    expect(coerceDuration('100ms| N0["<evil>"]')).toBe(0));
  it('converts numeric string to integer', () => expect(coerceDuration('42')).toBe(42));
  it('truncates positive float', () => expect(coerceDuration(42.9)).toBe(42));
  it('truncates negative float toward zero', () => expect(coerceDuration(-5.7)).toBe(-5));
  it('handles integer passthrough', () => expect(coerceDuration(250)).toBe(250));
});

// ── Property: sanitize strips all HTML-dangerous chars ─────────────────────

describe('sanitize', () => {
  it('strips < and >', () => {
    expect(sanitize('<script>alert(1)</script>')).not.toMatch(/[<>]/);
  });
  it('strips double quotes', () => {
    expect(sanitize('foo"bar')).not.toContain('"');
  });
  it('strips = sign', () => {
    expect(sanitize('onerror=alert(1)')).not.toContain('=');
  });
  it('strips parentheses', () => {
    expect(sanitize('alert(1)')).not.toMatch(/[()]/);
  });
  it('strips semicolons', () => {
    expect(sanitize('a;b')).not.toContain(';');
  });
  it('preserves alphanumeric and whitelisted chars', () => {
    const safe = 'CORSO.build_trace-1:2/3 x';
    expect(sanitize(safe)).toBe(safe);
  });
  it('truncates output to 40 chars', () => {
    expect(sanitize('a'.repeat(100))).toHaveLength(40);
  });
  it('full XSS actor string has no HTML-special chars remaining', () => {
    const result = sanitize('<img src=x onerror=alert(1)>');
    expect(result).not.toMatch(/[<>"=()]/);
  });
});

// ── Regression: XSS payload in span fields cannot survive DSL generation ──

describe('DSL generation — XSS regression', () => {
  const xssActor = '<img src=x onerror=alert(1)>';
  const xssAction = '<script>evil()</script>';

  it('buildSequenceDiagram: no HTML special chars in output for XSS actor/action', () => {
    const dsl = buildSequenceDiagram([span({ actor: xssActor, action: xssAction })]);
    expect(dsl).not.toMatch(/[<>"]/);
  });

  it('buildFlowDiagram: no HTML injection chars in output for XSS actor/action', () => {
    const a = span({ actor: xssActor, action: xssAction, outcome: 'Finish', duration_ms: 50 });
    const b = span({ span_id: 's2', actor: 'EVA', action: 'deploy' });
    const dsl = buildFlowDiagram([a, b]);
    // " appears in Mermaid ["label"] syntax; --> uses > legitimately — check < (open-tag) only
    expect(dsl).not.toContain('<');
    expect(dsl).not.toContain('onerror');
    expect(dsl).not.toContain('alert');
  });

  it('buildFlowDiagram: duration_ms DSL injection is neutralized to 0', () => {
    const root = span({ span_id: 's0', outcome: 'Continue' });
    const injected = span({
      span_id: 's1',
      parent_id: 's0',
      outcome: 'Finish',
      duration_ms: '0ms| N3["<evil>"]' as unknown as number,
    });
    const dsl = buildFlowDiagram([root, injected]);
    expect(dsl).not.toContain('evil');
    expect(dsl).toContain('-->|0ms|');
  });

  it('buildSequenceDiagram: duration_ms DSL injection is neutralized to 0', () => {
    const injected = span({
      duration_ms: 'INJECTED\n  Note over X: pwned' as unknown as number,
    });
    const dsl = buildSequenceDiagram([injected]);
    expect(dsl).not.toContain('pwned');
    expect(dsl).not.toContain('INJECTED');
  });
});

// ── Integration: DOMPurify SVG profile strips injected content ─────────────

describe('DOMPurify SVG profile', () => {
  const opts = { USE_PROFILES: { svg: true, svgFilters: true } } as const;

  it('strips <script> from SVG output', () => {
    const dirty = '<svg><script>alert(1)</script><rect width="10" height="10"/></svg>';
    const clean = DOMPurify.sanitize(dirty, opts);
    expect(clean).not.toContain('<script>');
    expect(clean).toContain('<rect');
  });

  it('strips onerror event handler', () => {
    const dirty = '<svg><image href="x" onerror="alert(1)"/></svg>';
    const clean = DOMPurify.sanitize(dirty, opts);
    expect(clean).not.toContain('onerror');
  });

  it('strips javascript: href', () => {
    const dirty = '<svg><a href="javascript:alert(1)"><circle r="40"/></a></svg>';
    const clean = DOMPurify.sanitize(dirty, opts);
    expect(clean).not.toContain('javascript:');
  });

  it('strips onclick attribute', () => {
    const dirty = '<svg><g onclick="xssAttack()"><rect/></g></svg>';
    const clean = DOMPurify.sanitize(dirty, opts);
    expect(clean).not.toContain('onclick');
  });

  it('preserves feGaussianBlur needed for Mermaid dark theme', () => {
    const dirty = '<svg><defs><filter id="f"><feGaussianBlur stdDeviation="2"/></filter></defs></svg>';
    const clean = DOMPurify.sanitize(dirty, opts);
    expect(clean).toContain('feGaussianBlur');
  });

  it('preserves structural SVG elements needed for Mermaid output', () => {
    const dirty = '<svg><g id="layer"><rect x="0" y="0" width="100" height="50"/><circle r="10"/></g></svg>';
    const clean = DOMPurify.sanitize(dirty, opts);
    expect(clean).toContain('<g');
    expect(clean).toContain('<rect');
  });
});
