import { describe, it, expect } from 'vitest';
import {
  MOSAIC_PANEL_ROLES,
  ALL_MOSAIC_PANEL_ROLES,
  type MosaicPanelRole,
} from '$lib/panels/panelRoles';

describe('mosaic panel role registry', () => {
  it('contains exactly the two helix panel roles (exhaustiveness)', () => {
    expect(ALL_MOSAIC_PANEL_ROLES).toHaveLength(2);
  });

  it('includes retrieval-metrics', () => {
    expect(ALL_MOSAIC_PANEL_ROLES).toContain('retrieval-metrics' satisfies MosaicPanelRole);
  });

  it('includes cache-stats', () => {
    expect(ALL_MOSAIC_PANEL_ROLES).toContain('cache-stats' satisfies MosaicPanelRole);
  });

  it('every role has a non-empty description', () => {
    for (const role of ALL_MOSAIC_PANEL_ROLES) {
      expect(MOSAIC_PANEL_ROLES[role]).toBeTruthy();
    }
  });

  it('retrieval-metrics description references P3 adaptive retrieval', () => {
    expect(MOSAIC_PANEL_ROLES['retrieval-metrics']).toMatch(/KW.*BALANCED.*GRAPH|P3/);
  });

  it('cache-stats description references P5 knowledge coverage', () => {
    expect(MOSAIC_PANEL_ROLES['cache-stats']).toMatch(/P5|entry count/);
  });

  it('ALL_MOSAIC_PANEL_ROLES and MOSAIC_PANEL_ROLES keys are in sync', () => {
    const recordKeys = Object.keys(MOSAIC_PANEL_ROLES) as MosaicPanelRole[];
    expect(ALL_MOSAIC_PANEL_ROLES.sort()).toEqual(recordKeys.sort());
  });
});
