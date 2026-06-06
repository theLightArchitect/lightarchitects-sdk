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
  // d1 cards — live in CockpitProject
  'build-health':        ['src/screens/CockpitProject.svelte', 'src/screens/CockpitBuild.svelte'],
  'hitl-escalations':    ['src/screens/CockpitProject.svelte', 'src/screens/CockpitBuild.svelte'],
  'builds-rail':         ['src/screens/CockpitProject.svelte'],
  // d2 cards — live in CockpitBuild
  'worker-fleet':        ['src/screens/CockpitBuild.svelte'],
  'decision-feed':       ['src/screens/CockpitBuild.svelte'],
  'git-state':           ['src/screens/CockpitBuild.svelte'],
  'engineer-zones':      ['src/screens/CockpitBuild.svelte'],
  // multi-scope cards — d0 + d1
  'hitl-inbox':          ['src/screens/CockpitPlatform.svelte', 'src/screens/CockpitProject.svelte'],
  'strategy-catalogue':  ['src/screens/CockpitPlatform.svelte', 'src/screens/CockpitProject.svelte'],
  // multi-scope — d1 + d2
  'pr-detail-panel':     ['src/screens/CockpitProject.svelte', 'src/screens/CockpitBuild.svelte'],
  // shared infrastructure
  'copilot-drawer':      ['src/components/CopilotDrawer.svelte'],
  'wave-composer':       ['src/components/Cockpit/WaveComposer.svelte', 'src/screens/CockpitBuild.svelte'],
  // d0-only aggregator cards — self-annotated in component + wrapped in CockpitPlatform
  'northstar-pulse':     ['src/components/Cockpit/NorthstarPulseCard.svelte', 'src/screens/CockpitPlatform.svelte'],
  'strand-mosaic':       ['src/components/Cockpit/StrandMosaicCard.svelte',   'src/screens/CockpitPlatform.svelte', 'src/screens/CockpitProject.svelte'],
  'smart-dispatch':      ['src/components/Cockpit/SmartDispatchCard.svelte',  'src/screens/CockpitPlatform.svelte'],
  'squad-constellation': ['src/components/Cockpit/SquadConstellationCard.svelte', 'src/screens/CockpitPlatform.svelte'],
  // right drawer — shell + inner router
  'focus-drawer':        ['src/lib/cockpit/shell/RightDrawer.svelte'],
  'focus-router':        ['src/lib/cockpit/focus/FocusRouter.svelte'],
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

  it('every data-card-role attribute in CockpitProject.svelte is registered', () => {
    const src = readSource('src/screens/CockpitProject.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES, `data-card-role="${role}" in CockpitProject.svelte but not registered`).toContain(role);
    }
  });

  it('every data-card-role attribute in CockpitBuild.svelte is registered', () => {
    const src = readSource('src/screens/CockpitBuild.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES, `data-card-role="${role}" in CockpitBuild.svelte but not registered`).toContain(role);
    }
  });

  it('every data-card-role attribute in RightDrawer.svelte is registered', () => {
    const src = readSource('src/lib/cockpit/shell/RightDrawer.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES, `data-card-role="${role}" in RightDrawer.svelte but not registered`).toContain(role);
    }
  });

  it('every data-card-role attribute in FocusRouter.svelte is registered', () => {
    const src = readSource('src/lib/cockpit/focus/FocusRouter.svelte');
    for (const [, role] of src.matchAll(/data-card-role="([^"]+)"/g)) {
      expect(ALL_COCKPIT_CARD_ROLES, `data-card-role="${role}" in FocusRouter.svelte but not registered`).toContain(role);
    }
  });

  // Scope-leak: d0-only cards must not appear in d1/d2 screens
  it('d0-only cards (northstar-pulse, smart-dispatch, squad-constellation) do not leak into d1/d2 screens', () => {
    const d0Only = ['northstar-pulse', 'smart-dispatch', 'squad-constellation'];
    for (const screen of ['src/screens/CockpitProject.svelte', 'src/screens/CockpitBuild.svelte']) {
      const src = readSource(screen);
      for (const role of d0Only) {
        expect(src, `d0-only card "${role}" leaked into ${screen}`).not.toContain(`data-card-role="${role}"`);
      }
    }
  });

  // Scope-leak: d2-only cards must not appear in d0 screen
  it('d2-only cards (worker-fleet, decision-feed, git-state, engineer-zones) do not leak into CockpitPlatform', () => {
    const d2Only = ['worker-fleet', 'decision-feed', 'git-state', 'engineer-zones'];
    const src = readSource('src/screens/CockpitPlatform.svelte');
    for (const role of d2Only) {
      expect(src, `d2-only card "${role}" found in CockpitPlatform.svelte`).not.toContain(`data-card-role="${role}"`);
    }
  });

  it('registry has exactly 21 roles — adding a card requires updating this test', () => {
    expect(ALL_COCKPIT_CARD_ROLES).toHaveLength(21);
  });
});
