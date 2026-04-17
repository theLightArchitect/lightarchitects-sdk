// ============================================================================
// File: web-figma/src/engineering/tests/authorization.test.ts
// Territory: ENGINEERING — not Figma Make synced
// Suite: AUTHORIZATION — trust boundary: who/what is admitted through the
//        CommandPalette sanitizers (SERAPH §27)
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

// ── Authorization: only SIBLINGS members are admitted ───────────────────────

describe('sanitizeSibling — authorization boundary', () => {
  it('authorizes every member of the canonical SIBLINGS list', () => {
    for (const s of SIBLINGS as readonly string[]) {
      expect(sanitizeSibling(s)).toBe(s);
    }
  });

  it('returns normalized (lowercase) canonical form on success', () => {
    for (const s of SIBLINGS as readonly string[]) {
      const upper = s.toUpperCase();
      expect(sanitizeSibling(upper)).toBe(s); // canonical is lowercase
    }
  });

  it('admits sibling with leading/trailing whitespace (normalization before check)', () => {
    for (const s of SIBLINGS as readonly string[]) {
      expect(sanitizeSibling(`  ${s}  `)).toBe(s);
    }
  });

  it('denies a sibling name with an inserted space (e.g., "s o u l")', () => {
    expect(sanitizeSibling('s o u l')).toBeNull();
  });

  it('denies a sibling name with a suffix (e.g., "soul2")', () => {
    expect(sanitizeSibling('soul2')).toBeNull();
  });

  it('denies a sibling name with a prefix (e.g., "xsoul")', () => {
    expect(sanitizeSibling('xsoul')).toBeNull();
  });

  it('denial is total: function returns null, not false or undefined', () => {
    const result = sanitizeSibling('notasibling');
    expect(result).toBeNull();
    expect(result === null).toBe(true);
  });

  it('SIBLINGS list itself is the complete authorization allowlist (no hidden members)', () => {
    const knownMembers = new Set(['soul', 'eva', 'corso', 'quantum', 'seraph', 'ayin', 'larc']);
    for (const s of SIBLINGS as readonly string[]) {
      expect(knownMembers.has(s), `unexpected SIBLINGS member: ${s}`).toBe(true);
    }
  });
});

// ── sanitizeQuery — authorized character set ─────────────────────────────────

describe('sanitizeQuery — character set authorization', () => {
  it('admits alphanumeric characters', () => {
    expect(sanitizeQuery('abc123')).toBe('abc123');
  });

  it('admits space (word separator)', () => {
    expect(sanitizeQuery('hello world')).toBe('hello world');
  });

  it('admits dot (file/version separator)', () => {
    expect(sanitizeQuery('v1.2.3')).toBe('v1.2.3');
  });

  it('admits underscore (identifier separator)', () => {
    expect(sanitizeQuery('helix_query')).toBe('helix_query');
  });

  it('admits hyphen (kebab-case names)', () => {
    expect(sanitizeQuery('soul-helix')).toBe('soul-helix');
  });

  it('denies every character outside the authorized set individually', () => {
    const unauthorized = ['<', '>', '{', '}', '[', ']', '(', ')', '!', '@', '#',
      '$', '%', '^', '&', '*', '/', '\\', '|', ';', ':', '"', "'", '`', '~', '+', '='];
    for (const ch of unauthorized) {
      // Insert between two safe chars — if the char is stripped, we get 'ab'
      const result = sanitizeQuery(`a${ch}b`);
      expect(result).toBe('ab');
    }
  });

  it('enforces 200-character maximum length (input truncated, not rejected)', () => {
    const input = 'a'.repeat(300);
    const result = sanitizeQuery(input);
    expect(result).not.toBeNull();
    expect(result!.length).toBe(200);
  });

  it('rejects whitespace-only input (not authorized as a query)', () => {
    expect(sanitizeQuery('     ')).toBeNull();
    expect(sanitizeQuery('\t\n\r')).toBeNull();
  });
});
