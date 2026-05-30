import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { ALL_COCKPIT_CARD_ROLES, COCKPIT_CARD_ROLES, type CockpitCardRole } from '$lib/cockpit/cardRoles';

const ROOT = join(import.meta.dirname, '../..');

function readSource(rel: string): string {
  return readFileSync(join(ROOT, rel), 'utf-8');
}

const ANNOTATED_SOURCES: Record<CockpitCardRole, string[]> = {
  'preset-chips':       ['src/components/Cockpit/PresetChips.svelte'],
  'target-breadcrumb':  ['src/components/Cockpit/TargetBreadcrumb.svelte'],
  'quick-pick-palette': ['src/components/Cockpit/QuickPickPalette.svelte'],
  'build-health':       ['src/screens/Cockpit.svelte'],
  'hitl-escalations':   ['src/screens/Cockpit.svelte'],
  'worker-fleet':       ['src/screens/Cockpit.svelte'],
  'decision-feed':      ['src/screens/Cockpit.svelte'],
  'git-state':          ['src/screens/Cockpit.svelte'],
  'builds-rail':        ['src/screens/Cockpit.svelte'],
  'hitl-inbox':         ['src/screens/Cockpit.svelte'],
  'pr-detail-panel':    ['src/screens/Cockpit.svelte'],
  'engineer-zones':     ['src/screens/Cockpit.svelte'],
  'copilot-drawer':     ['src/components/CopilotDrawer.svelte'],
  'strategy-catalogue': ['src/screens/Cockpit.svelte'],
};

describe('Cockpit card-role registry', () => {
  it('COCKPIT_CARD_ROLES has an entry for every CockpitCardRole', () => {
    for (const role of ALL_COCKPIT_CARD_ROLES) {
      expect(COCKPIT_CARD_ROLES).toHaveProperty(role);
      expect(COCKPIT_CARD_ROLES[role].length).toBeGreaterThan(0);
    }
  });

  it('every registry entry has a corresponding data-card-role attribute in source', () => {
    for (const role of ALL_COCKPIT_CARD_ROLES) {
      const files = ANNOTATED_SOURCES[role];
      const attr = `data-card-role="${role}"`;
      const found = files.some(f => readSource(f).includes(attr));
      expect(found, `Missing ${attr} in ${files.join(', ')}`).toBe(true);
    }
  });

  it('every data-card-role attribute in Cockpit.svelte is registered', () => {
    const cockpit = readSource('src/screens/Cockpit.svelte');
    const matches = [...cockpit.matchAll(/data-card-role="([^"]+)"/g)];
    for (const [, role] of matches) {
      expect(
        ALL_COCKPIT_CARD_ROLES,
        `data-card-role="${role}" found in Cockpit.svelte but not in registry`,
      ).toContain(role);
    }
  });

  it('every data-card-role attribute in PresetChips.svelte is registered', () => {
    const src = readSource('src/components/Cockpit/PresetChips.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES).toContain(role);
    }
  });

  it('every data-card-role attribute in TargetBreadcrumb.svelte is registered', () => {
    const src = readSource('src/components/Cockpit/TargetBreadcrumb.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES).toContain(role);
    }
  });

  it('every data-card-role attribute in QuickPickPalette.svelte is registered', () => {
    const src = readSource('src/components/Cockpit/QuickPickPalette.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES).toContain(role);
    }
  });

  it('every data-card-role attribute in CopilotDrawer.svelte is registered', () => {
    const src = readSource('src/components/CopilotDrawer.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES).toContain(role);
    }
  });

  it('registry has exactly 14 roles — adding a card requires updating this test', () => {
    expect(ALL_COCKPIT_CARD_ROLES).toHaveLength(14);
  });
});
