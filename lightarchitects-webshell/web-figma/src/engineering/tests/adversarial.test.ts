// ============================================================================
// File: web-figma/src/engineering/tests/adversarial.test.ts
// Territory: ENGINEERING — not Figma Make synced
// Suite: ADVERSARIAL — CommandPalette sanitizer rejection gate (SERAPH §30)
// ============================================================================

import { describe, it, expect } from 'vitest';
import { SIBLINGS } from '../store/sceneState';

// ── Replicate sanitizers (same logic as CommandPalette.tsx) ──────────────────

function sanitizeSibling(raw: string): string | null {
  const s = raw.toLowerCase().trim();
  return (SIBLINGS as readonly string[]).includes(s) ? s : null;
}

function sanitizeQuery(raw: string): string | null {
  const s = raw.replace(/[^a-zA-Z0-9 ._\-]/g, '').slice(0, 200).trim();
  return s.length > 0 ? s : null;
}

// ── sanitizeSibling — accept only exact SIBLINGS members ────────────────────

describe('sanitizeSibling — accepts', () => {
  it.each(SIBLINGS as unknown as string[])('accepts "%s"', (s) => {
    expect(sanitizeSibling(s)).toBe(s);
  });

  it('is case-insensitive', () => {
    expect(sanitizeSibling('SOUL')).toBe('soul');
    expect(sanitizeSibling('Eva')).toBe('eva');
  });

  it('trims whitespace', () => {
    expect(sanitizeSibling('  corso  ')).toBe('corso');
  });
});

describe('sanitizeSibling — rejects adversarial inputs', () => {
  const injections = [
    // Script injection
    '<script>alert(1)</script>',
    'javascript:alert(1)',
    '"><img src=x onerror=alert(1)>',
    // Shell / eval
    '$(whoami)',
    '`id`',
    '; rm -rf /',
    '| cat /etc/passwd',
    // Path traversal
    '../../etc/passwd',
    '../../../root',
    // Unicode homoglyphs
    'soｕl',
    'evа', // Cyrillic а
    // Null bytes
    'soul\x00',
    // Overflow
    'soul'.repeat(300),
    // Unknown names
    'admin',
    'root',
    'god',
    '__proto__',
    'constructor',
    'prototype',
    // Empty / whitespace only
    '',
    '   ',
    '\t\n',
  ];

  it.each(injections)('rejects: %j', (input) => {
    expect(sanitizeSibling(input)).toBeNull();
  });
});

// ── sanitizeQuery — strip and clamp ─────────────────────────────────────────

describe('sanitizeQuery — accepts safe queries', () => {
  it('accepts plain alphanumeric query', () => {
    expect(sanitizeQuery('test helix query')).toBe('test helix query');
  });

  it('accepts dots underscores hyphens', () => {
    expect(sanitizeQuery('soul.helix_v1-test')).toBe('soul.helix_v1-test');
  });

  it('clamps to 200 chars', () => {
    const long = 'a'.repeat(300);
    expect(sanitizeQuery(long)!.length).toBe(200);
  });
});

describe('sanitizeQuery — rejects / strips adversarial inputs', () => {
  it('strips angle brackets (XSS vector)', () => {
    expect(sanitizeQuery('<script>alert(1)</script>')).toBe('scriptalert1script');
  });

  it('strips shell metacharacters', () => {
    expect(sanitizeQuery('$(id)')).toBe('id');
    expect(sanitizeQuery('`whoami`')).toBe('whoami');
  });

  it('strips semicolons and pipes', () => {
    expect(sanitizeQuery('foo;bar|baz')).toBe('foobarbaz');
  });

  it('strips null bytes', () => {
    expect(sanitizeQuery('query\x00inject')).toBe('queryinject');
  });

  it('strips backslashes', () => {
    expect(sanitizeQuery('C:\\Windows\\System32')).toBe('CWindowsSystem32');
  });

  it('strips percent-encoding', () => {
    expect(sanitizeQuery('%3Cscript%3E')).toBe('3Cscript3E');
  });

  it('strips quotes', () => {
    expect(sanitizeQuery('"quote" \'injection\'')).toBe('quote injection');
  });

  it('strips curly braces (template injection)', () => {
    expect(sanitizeQuery('{{7*7}}')).toBe('77');
  });

  it('returns null for all-special-char input', () => {
    expect(sanitizeQuery('<>{}[]()!@#$%^&*')).toBeNull();
  });

  it('returns null for empty string', () => {
    expect(sanitizeQuery('')).toBeNull();
  });

  it('returns null for whitespace-only', () => {
    expect(sanitizeQuery('   ')).toBeNull();
  });
});
