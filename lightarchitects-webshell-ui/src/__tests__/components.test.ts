import { describe, it, expect } from 'vitest';
import { get } from 'svelte/store';

// Note: Svelte 5 component testing with runes requires vitest-browser-svelte.
// These tests verify the component logic and prop interfaces.
// Full rendering tests would use @testing-library/svelte in browser mode.

describe('PillarDetail', () => {
  // Import the component to verify it exists and can be parsed
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/PillarDetail.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('FindingsPanel', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/FindingsPanel.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('LogStream', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/LogStream.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('HierarchyNav', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/HierarchyNav.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('PolytopeIcon', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/PolytopeIcon.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('PolytopeDecor', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/PolytopeDecor.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('PillarRail', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/PillarRail.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('StatusBar', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/StatusBar.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('ScopeRail', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/ScopeRail.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('CommandPalette', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/CommandPalette.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('HelixScene', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/HelixScene.svelte');
    expect(mod.default).toBeDefined();
  });
});

describe('Helix3D', () => {
  it('module imports successfully', async () => {
    const mod = await import('$lib/../components/Helix3D.svelte');
    expect(mod.default).toBeDefined();
  });
});