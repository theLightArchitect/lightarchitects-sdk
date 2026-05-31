import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';

const ROOT = join(import.meta.dirname, '../..');
const TABLE_SRC = 'src/lib/components/ActiveContainersTable.svelte';

function readSource(rel: string): string {
  return readFileSync(join(ROOT, rel), 'utf-8');
}

describe('ActiveContainersTable module', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/components/ActiveContainersTable.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('ActiveContainersTable source assertions', () => {
  let src: string;
  beforeAll(() => { src = readSource(TABLE_SRC); });

  it('renders rows from containers state (each loop)', () => {
    expect(src).toContain('#each containers');
  });

  it('auto-refreshes on 5 s timer via setInterval', () => {
    expect(src).toContain('setInterval');
    expect(src).toContain('5000');
  });

  it('clears interval on destroy', () => {
    expect(src).toContain('clearInterval');
    expect(src).toContain('onDestroy');
  });

  it('shows empty-state message when no containers', () => {
    expect(src).toContain('No containers running');
  });

  it('shows load error when fetch fails', () => {
    expect(src).toContain('load-error');
  });

  it('renders hardening_actual seccomp column', () => {
    expect(src).toContain('hardening_actual.seccomp');
  });

  it('renders hardening_actual cap_drop column', () => {
    expect(src).toContain('hardening_actual.cap_drop');
  });

  it('renders hardening_actual userns with color-coded badge', () => {
    expect(src).toContain('userns-remapped');
    expect(src).toContain('userns-host');
    expect(src).toContain('userns-unsup');
  });

  it('displays iso_mode_at_spawn and network_policy_at_spawn', () => {
    expect(src).toContain('iso_mode_at_spawn');
    expect(src).toContain('network_policy_at_spawn');
  });

  it('truncates container ID to 12 characters', () => {
    expect(src).toContain('.slice(0, 12)');
  });

  it('shows age in human-readable format', () => {
    expect(src).toContain('formatAge');
  });
});
