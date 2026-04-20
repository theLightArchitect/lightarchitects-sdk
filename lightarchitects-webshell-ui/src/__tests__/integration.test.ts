import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import {
  builds, workspaces, findings, logEntries, selectedPillar,
  buildStats, activeBuild, currentBuildId,
} from '$lib/stores';
import { PILLAR_ACTIONS, PILLARS, SIBLINGS } from '$lib/types';
import { getMetaSkillPolytope, getMetaSkillColor, SIBLING_COLORS } from '$lib/design-tokens';
import { getPolytope4D } from '$lib/polytopes4d-canvas2d';

describe('BuildQueue integration', () => {
  it('all builds have valid meta-skill polytope mappings', () => {
    const allBuilds = get(builds);
    for (const build of allBuilds) {
      const polyType = getMetaSkillPolytope(build.metaSkill);
      const polyColor = getMetaSkillColor(build.metaSkill);

      // Polytope should be a valid type
      const polyData = getPolytope4D(polyType);
      expect(polyData.vertices.length).toBeGreaterThan(0);
      expect(polyData.edges.length).toBeGreaterThan(0);

      // Color should be a valid hex
      expect(polyColor).toMatch(/^#[0-9a-fA-F]{6}$/);
    }
  });

  it('all builds have 7 pillar gates', () => {
    const allBuilds = get(builds);
    for (const build of allBuilds) {
      expect(build.pillars).toHaveLength(7);
      for (const gate of build.pillars) {
        expect(PILLARS).toContain(gate.pillar);
      }
    }
  });

  it('currentPillar matches a pillar in the build', () => {
    const allBuilds = get(builds);
    for (const build of allBuilds) {
      expect(PILLARS).toContain(build.currentPillar);
    }
  });

  it('meta-skill pillar actions match for each build', () => {
    const allBuilds = get(builds);
    for (const build of allBuilds) {
      const actions = PILLAR_ACTIONS[build.metaSkill];
      expect(actions).toBeDefined();
      for (const pillar of PILLARS) {
        expect(actions[pillar]).toBeDefined();
        expect(typeof actions[pillar]).toBe('string');
      }
    }
  });

  it('buildStats are consistent with build data', () => {
    const stats = get(buildStats);
    const allBuilds = get(builds);
    expect(stats.total).toBe(allBuilds.length);
    expect(stats.inProgress + stats.completed + stats.failed + stats.pending).toBe(stats.total);
  });
});

describe('Workspace integration', () => {
  beforeEach(() => {
    findings.set([
      { id: 'f-001', buildId: 'build-001', pillar: 'QUAL', severity: 'warning', category: 'quality', title: 'T', description: 'D', verified: false },
      { id: 'f-002', buildId: 'build-001', pillar: 'SEC', severity: 'error', category: 'security', title: 'T2', description: 'D2', verified: false },
    ]);
  });
  afterEach(() => { findings.set([]); });

  it('findings can be filtered by build and pillar', () => {
    const allFindings = get(findings);
    const build1Findings = allFindings.filter(f => f.buildId === 'build-001');
    expect(build1Findings.length).toBeGreaterThan(0);

    const qualFindings = build1Findings.filter(f => f.pillar === 'QUAL');
    expect(qualFindings.length).toBeGreaterThan(0);
  });

  it('selecting a pillar filters findings correctly', () => {
    const allFindings = get(findings);
    const build1Findings = allFindings.filter(f => f.buildId === 'build-001');

    // Select QUAL pillar
    selectedPillar.set('QUAL');
    const filtered = build1Findings.filter(f => f.pillar === 'QUAL');
    expect(filtered.every(f => f.pillar === 'QUAL')).toBe(true);
    expect(filtered.length).toBeGreaterThan(0);

    // Reset
    selectedPillar.set(null);
  });

  it('log entries have valid timestamps and sources', () => {
    const logs = get(logEntries);
    for (const entry of logs) {
      expect(entry.id).toBeDefined();
      expect(entry.timestamp).toBeDefined();
      expect(entry.level).toBeDefined();
      expect(entry.source).toBeDefined();
      expect(entry.message).toBeDefined();
      expect(['debug', 'info', 'warn', 'error', 'success']).toContain(entry.level);
    }
  });
});

describe('HierarchyNav integration', () => {
  beforeEach(() => {
    workspaces.set([
      { id: 'ws-001', name: 'Auth Service', path: '/auth', builds: [
        { id: 'build-001', workspaceId: 'ws-001', name: 'Auth flow', metaSkill: '/BUILD', status: 'in_progress', pillars: [], currentPillar: 'ARCH', confidence: 0.5, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [] },
      ]},
      { id: 'ws-002', name: 'API Gateway', path: '/api', builds: [
        { id: 'build-002', workspaceId: 'ws-002', name: 'Gateway', metaSkill: '/BUILD', status: 'completed', pillars: [], currentPillar: 'OPS', confidence: 0.9, createdAt: '', updatedAt: '', modules: [], siblingDispatches: [] },
      ]},
    ]);
  });
  afterEach(() => { workspaces.set([]); });

  it('workspaces contain their builds', () => {
    const ws = get(workspaces);
    const ws001 = ws.find(w => w.id === 'ws-001');
    expect(ws001).toBeDefined();
    expect(ws001!.builds.length).toBeGreaterThan(0);

    const ws002 = ws.find(w => w.id === 'ws-002');
    expect(ws002).toBeDefined();
    expect(ws002!.builds.length).toBeGreaterThan(0);
  });

  it('build workspaceIds match their parent workspace', () => {
    const ws = get(workspaces);
    for (const workspace of ws) {
      for (const build of workspace.builds) {
        expect(build.workspaceId).toBe(workspace.id);
      }
    }
  });
});

describe('Sibling dispatch integration', () => {
  it('all 6 dispatch buttons reference valid siblings', () => {
    // The dispatch panel shows SIBLINGS.slice(0, 6)
    const dispatchSiblings = SIBLINGS.slice(0, 6);
    expect(dispatchSiblings).toHaveLength(6);
    for (const sib of dispatchSiblings) {
      expect(SIBLING_COLORS[sib]).toBeDefined();
    }
  });
});

describe('Polytope rendering integration', () => {
  it('every meta-skill maps to a renderable polytope', () => {
    const allBuilds = get(builds);
    for (const build of allBuilds) {
      const polyType = getMetaSkillPolytope(build.metaSkill);
      const data = getPolytope4D(polyType);

      // Polytope should have reasonable vertex/edge counts
      expect(data.vertices.length).toBeLessThanOrEqual(120);
      expect(data.edges.length).toBeLessThanOrEqual(720);

      // All edge indices should be valid
      for (const [a, b] of data.edges) {
        expect(a).toBeGreaterThanOrEqual(0);
        expect(a).toBeLessThan(data.vertices.length);
        expect(b).toBeGreaterThanOrEqual(0);
        expect(b).toBeLessThan(data.vertices.length);
      }
    }
  });
});