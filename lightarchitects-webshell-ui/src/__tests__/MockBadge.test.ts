import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

// Convention (per components.test.ts): Svelte 5 + runes does not have a stable
// in-process renderer in our vitest setup. We use module-import smoke tests +
// source-level a11y assertions for invariants that would break the contract.

describe('MockBadge component', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/MockBadge.svelte');
    expect(mod.default).toBeDefined();
  });

  describe('source-level a11y + design invariants', () => {
    const src = readFileSync(
      resolve(__dirname, '../components/MockBadge.svelte'),
      'utf-8',
    );

    it('FE-3: declares role="note" (NOT role="status") — no SR live-region noise', () => {
      expect(src).toContain('role="note"');
      expect(src).not.toMatch(/role=["']status["']/);
    });

    it('FE-1: uses var(--la-font-mono) — JetBrains Mono via design system token', () => {
      expect(src).toContain('var(--la-font-mono)');
      expect(src).not.toMatch(/font-family:\s*ui-monospace/);
    });

    it('FE-2: uses --la-warn-mock-* tokens — no raw hex orange', () => {
      expect(src).toContain('var(--la-warn-mock-fg)');
      expect(src).toContain('var(--la-warn-mock-edge)');
      // Reject hardcoded amber/orange hex sneaking back in
      expect(src).not.toMatch(/#fb923c/);
      expect(src).not.toMatch(/#f97316/);
    });

    it('FE-6: applies max-width 22ch + 360px media query for narrow viewports', () => {
      expect(src).toContain('max-width: 22ch');
      expect(src).toMatch(/@media\s*\(\s*max-width:\s*360px/);
    });

    it('declares aria-label that includes detail when present', () => {
      expect(src).toContain('aria-label');
      expect(src).toMatch(/aria-label=\{detail/);
    });

    it('declares pointer-events: none (badge does not block clicks)', () => {
      expect(src).toContain('pointer-events: none');
    });
  });
});

describe('MockWrapper component', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/MockWrapper.svelte');
    expect(mod.default).toBeDefined();
  });

  describe('source-level invariants', () => {
    const src = readFileSync(
      resolve(__dirname, '../components/MockWrapper.svelte'),
      'utf-8',
    );

    it('FE-4: applies inert attribute when inertChildren is true', () => {
      expect(src).toContain('inert');
      expect(src).toMatch(/inertChildren\s*\?\s*true\s*:\s*undefined/);
    });

    it('FE-8: declares mock-fade-in keyframes (entry animation)', () => {
      expect(src).toContain('@keyframes mock-fade-in');
      expect(src).toContain('animation: mock-fade-in');
    });

    it('uses filter: grayscale + opacity for desaturated look', () => {
      expect(src).toMatch(/filter:\s*grayscale/);
      expect(src).toContain('opacity(');
    });

    it('imports MockBadge for the corner stamp', () => {
      expect(src).toContain('MockBadge');
    });
  });
});
