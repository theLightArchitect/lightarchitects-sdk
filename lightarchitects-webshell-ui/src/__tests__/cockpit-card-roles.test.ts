import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import { ALL_COCKPIT_CARD_ROLES, COCKPIT_CARD_ROLES, type CockpitCardRole } from '$lib/cockpit/cardRoles';

const ROOT = join(import.meta.dirname, '../..');

function readSource(rel: string): string {
  return readFileSync(join(ROOT, rel), 'utf-8');
}

const ANNOTATED_SOURCES: Record<CockpitCardRole, string[]> = {
  'preset-chips':        ['src/components/Cockpit/PresetChips.svelte'],
  'target-breadcrumb':   ['src/components/Cockpit/TargetBreadcrumb.svelte'],
  'quick-pick-palette':  ['src/components/Cockpit/QuickPickPalette.svelte'],
  'build-health':        ['src/screens/CockpitPlatform.svelte'],
  'hitl-escalations':    ['src/screens/CockpitPlatform.svelte'],
  'worker-fleet':        ['src/screens/CockpitPlatform.svelte'],
  'decision-feed':       ['src/screens/CockpitPlatform.svelte'],
  'git-state':           ['src/screens/CockpitPlatform.svelte'],
  'builds-rail':         ['src/screens/CockpitPlatform.svelte'],
  'hitl-inbox':          ['src/screens/CockpitPlatform.svelte'],
  'pr-detail-panel':     ['src/screens/CockpitPlatform.svelte'],
  'engineer-zones':      ['src/screens/CockpitPlatform.svelte'],
  'copilot-drawer':      ['src/components/CopilotDrawer.svelte'],
  'strategy-catalogue':  ['src/screens/CockpitPlatform.svelte'],
  'wave-composer':       ['src/components/Cockpit/WaveComposer.svelte'],
  'northstar-pulse':     ['src/components/Cockpit/NorthstarPulseCard.svelte'],
  'strand-mosaic':       ['src/components/Cockpit/StrandMosaicCard.svelte'],
  'smart-dispatch':      ['src/components/Cockpit/SmartDispatchCard.svelte'],
  'squad-constellation': ['src/components/Cockpit/SquadConstellationCard.svelte'],
};

describe('Cockpit card-role registry', () => {
  it('COCKPIT_CARD_ROLES has an entry for every CockpitCardRole', () => {
    for (const role of ALL_COCKPIT_CARD_ROLES) {
      expect(COCKPIT_CARD_ROLES).toHaveProperty(role);
      expect(COCKPIT_CARD_ROLES[role].description.length).toBeGreaterThan(0);
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

  it('every data-card-role attribute in CockpitPlatform.svelte is registered', () => {
    const cockpit = readSource('src/screens/CockpitPlatform.svelte');
    const matches = [...cockpit.matchAll(/data-card-role="([^"]+)"/g)];
    for (const [, role] of matches) {
      expect(
        ALL_COCKPIT_CARD_ROLES,
        `data-card-role="${role}" found in CockpitPlatform.svelte but not in registry`,
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

  it('every data-card-role attribute in NorthstarPulseCard.svelte is registered', () => {
    const src = readSource('src/components/Cockpit/NorthstarPulseCard.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES).toContain(role);
    }
  });

  it('every data-card-role attribute in StrandMosaicCard.svelte is registered', () => {
    const src = readSource('src/components/Cockpit/StrandMosaicCard.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES).toContain(role);
    }
  });

  it('every data-card-role attribute in SmartDispatchCard.svelte is registered', () => {
    const src = readSource('src/components/Cockpit/SmartDispatchCard.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES).toContain(role);
    }
  });

  it('every data-card-role attribute in SquadConstellationCard.svelte is registered', () => {
    const src = readSource('src/components/Cockpit/SquadConstellationCard.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES).toContain(role);
    }
  });

  it('registry has exactly 19 roles — adding a card requires updating this test', () => {
    expect(ALL_COCKPIT_CARD_ROLES).toHaveLength(19);
  });
});
