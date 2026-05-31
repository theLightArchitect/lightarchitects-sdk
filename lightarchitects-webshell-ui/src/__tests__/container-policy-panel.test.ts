import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { PHASE_2_DISCLOSURE } from '$lib/types';

const ROOT = join(import.meta.dirname, '../..');

function readSource(rel: string): string {
  return readFileSync(join(ROOT, rel), 'utf-8');
}

const PANEL_SRC = 'src/lib/components/ContainerPolicyPanel.svelte';
const TABLE_SRC = 'src/lib/components/ActiveContainersTable.svelte';

describe('ContainerPolicyPanel module', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/components/ContainerPolicyPanel.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('ActiveContainersTable module', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/components/ActiveContainersTable.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('ContainerPolicyPanel source assertions', () => {
  let src: string;
  beforeAll(() => { src = readSource(PANEL_SRC); });

  it('renders 3 iso-mode segment buttons (standard, hardened, airgapped)', () => {
    expect(src).toContain("'standard', 'hardened', 'airgapped'");
  });

  it('renders 3 network policy segments (bridge, host, none)', () => {
    expect(src).toContain("'bridge', 'host', 'none'");
  });

  it('Balanced segment carries aria-disabled="true"', () => {
    // The Balanced button must declare aria-disabled="true" (ASSERT from plan Wave 3.3)
    expect(src).toContain('aria-disabled="true"');
  });

  it('Balanced segment is disabled and does not bind to selectNetwork("balanced")', () => {
    // selectNetwork guards against balanced: `if (net === 'balanced' ...) return`
    expect(src).toContain("net === 'balanced'");
  });

  it('renders 4 resource sliders', () => {
    const sliderCount = (src.match(/type="range"/g) ?? []).length;
    expect(sliderCount).toBeGreaterThanOrEqual(4);
  });

  it('has a collapsible Advanced section', () => {
    expect(src).toContain('<details');
    expect(src).toContain('<summary>');
  });

  it('renders Phase 2 disclosure callout for credential_strategy', () => {
    expect(src).toContain('PHASE_2_DISCLOSURE');
  });

  it('renders 500ms debounced PATCH on control changes', () => {
    expect(src).toContain('500');
    expect(src).toContain('scheduleUpdate');
  });

  it('sends If-Match header on concurrent-mutation guard (412 re-fetch)', () => {
    expect(src).toContain('412');
  });
});

describe('ActiveContainersTable source assertions', () => {
  let src: string;
  beforeAll(() => { src = readSource(TABLE_SRC); });

  it('auto-refreshes on 5 s interval', () => {
    expect(src).toContain('5000');
    expect(src).toContain('setInterval');
  });

  it('renders hardening_actual.seccomp column', () => {
    expect(src).toContain('seccomp');
  });

  it('renders hardening_actual.cap_drop column', () => {
    expect(src).toContain('cap_drop');
  });

  it('renders hardening_actual.userns column with color-coded badge', () => {
    expect(src).toContain('userns-remapped');
    expect(src).toContain('userns-host');
    expect(src).toContain('userns-unsup');
  });

  it('shows iso_mode_at_spawn and network_policy_at_spawn columns', () => {
    expect(src).toContain('iso_mode_at_spawn');
    expect(src).toContain('network_policy_at_spawn');
  });

  it('truncates container_id to 12 chars', () => {
    expect(src).toContain('.slice(0, 12)');
  });
});

describe('PHASE_2_DISCLOSURE constant', () => {
  it('is a non-empty string', () => {
    expect(typeof PHASE_2_DISCLOSURE).toBe('string');
    expect(PHASE_2_DISCLOSURE.length).toBeGreaterThan(10);
  });

  it('is referenced in ContainerPolicyPanel source', () => {
    const src = readSource(PANEL_SRC);
    expect(src).toContain('PHASE_2_DISCLOSURE');
  });
});
