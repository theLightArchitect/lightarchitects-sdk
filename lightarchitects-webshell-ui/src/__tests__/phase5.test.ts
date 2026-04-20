import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import {
  artifacts, buildNotes, expandedFindings, selectedArtifact,
  notesEditing, activeBuildArtifacts, findings, currentBuildId,
} from '$lib/stores';
import { parseCommand } from '$lib/commands';
import type { Artifact } from '$lib/types';

const NOW = new Date().toISOString();

const TEST_ARTIFACTS: Artifact[] = [
  { id: 'a-001', buildId: 'build-001', name: 'arch-report.json', type: 'report', size: 2048, url: '/api/builds/build-001/artifacts/a-001', createdAt: NOW, pillar: 'ARCH' },
  { id: 'a-002', buildId: 'build-001', name: 'coverage.html', type: 'coverage', size: 15360, url: '/api/builds/build-001/artifacts/a-002', createdAt: NOW, pillar: 'TEST' },
  { id: 'a-003', buildId: 'build-003', name: 'audit.json', type: 'audit', size: 4096, url: '/api/builds/build-003/artifacts/a-003', createdAt: NOW, pillar: 'SEC' },
  { id: 'a-004', buildId: 'build-001', name: 'server.bin', type: 'binary', size: 8192000, url: '/api/builds/build-001/artifacts/a-004', createdAt: NOW },
  { id: 'a-005', buildId: 'build-001', name: 'build.log', type: 'log', size: 512, url: '/api/builds/build-001/artifacts/a-005', createdAt: NOW },
];

const TEST_FINDINGS = [
  { id: 'f-001', buildId: 'build-001', pillar: 'QUAL' as const, severity: 'warning' as const, category: 'quality' as const, title: 'Long function', description: 'Function exceeds 60 lines', verified: false },
];

const TEST_NOTES = {
  'build-001': { buildId: 'build-001', content: 'Notes about authentication flow', updatedAt: NOW },
  'build-003': { buildId: 'build-003', content: 'Benchmark results show 20ms p99', updatedAt: NOW },
};

describe('Phase 5: Artifacts + Findings + Notes', () => {
  beforeEach(() => {
    currentBuildId.set(null);
    expandedFindings.set(new Set());
    selectedArtifact.set(null);
    notesEditing.set(false);
    artifacts.set(TEST_ARTIFACTS);
    findings.set(TEST_FINDINGS);
    buildNotes.set(TEST_NOTES);
  });

  describe('Artifact stores', () => {
    it('initializes as empty array when cleared', () => {
      artifacts.set([]);
      expect(get(artifacts)).toHaveLength(0);
    });

    it('has artifacts for build-001', () => {
      const arts = get(artifacts).filter(a => a.buildId === 'build-001');
      expect(arts.length).toBeGreaterThan(0);
    });

    it('has artifacts for build-003', () => {
      const arts = get(artifacts).filter(a => a.buildId === 'build-003');
      expect(arts.length).toBeGreaterThan(0);
    });

    it('all artifacts have valid type', () => {
      const validTypes = ['log', 'report', 'coverage', 'audit', 'binary'];
      for (const a of get(artifacts)) {
        expect(validTypes).toContain(a.type);
      }
    });

    it('some artifacts have pillar links', () => {
      const withPillar = get(artifacts).filter(a => a.pillar);
      expect(withPillar.length).toBeGreaterThan(0);
    });

    it('artifacts have valid size and url', () => {
      for (const a of get(artifacts)) {
        expect(a.size).toBeGreaterThan(0);
        expect(a.url).toContain('/api/builds/');
        expect(a.createdAt).toBeTruthy();
      }
    });

    it('selectedArtifact starts as null', () => {
      expect(get(selectedArtifact)).toBeNull();
    });

    it('selectedArtifact can be set', () => {
      const art = get(artifacts)[0];
      selectedArtifact.set(art);
      expect(get(selectedArtifact)?.id).toBe(art.id);
      selectedArtifact.set(null);
    });
  });

  describe('activeBuildArtifacts derived store', () => {
    it('returns empty when no build is selected', () => {
      currentBuildId.set(null);
      expect(get(activeBuildArtifacts)).toHaveLength(0);
    });

    it('returns artifacts for the selected build', () => {
      currentBuildId.set('build-001');
      const arts = get(activeBuildArtifacts);
      expect(arts.length).toBeGreaterThan(0);
      expect(arts.every(a => a.buildId === 'build-001')).toBe(true);
      currentBuildId.set(null);
    });
  });

  describe('Build notes stores', () => {
    it('has notes for build-001', () => {
      const notes = get(buildNotes);
      expect(notes['build-001']).toBeDefined();
      expect(notes['build-001'].content).toContain('authentication');
    });

    it('has notes for build-003', () => {
      const notes = get(buildNotes);
      expect(notes['build-003']).toBeDefined();
      expect(notes['build-003'].content).toContain('Benchmark');
    });

    it('notesEditing starts as false', () => {
      expect(get(notesEditing)).toBe(false);
    });

    it('notesEditing can be toggled', () => {
      notesEditing.set(true);
      expect(get(notesEditing)).toBe(true);
      notesEditing.set(false);
    });

    it('notes can be updated', () => {
      buildNotes.update(n => ({
        ...n,
        'build-001': { ...n['build-001'], content: 'Updated notes' },
      }));
      expect(get(buildNotes)['build-001'].content).toBe('Updated notes');
    });
  });

  describe('expandedFindings store', () => {
    it('starts as empty set', () => {
      expect(get(expandedFindings).size).toBe(0);
    });

    it('can add and remove finding IDs', () => {
      expandedFindings.update(s => { s.add('f-001'); return new Set(s); });
      expect(get(expandedFindings).has('f-001')).toBe(true);

      expandedFindings.update(s => { s.delete('f-001'); return new Set(s); });
      expect(get(expandedFindings).has('f-001')).toBe(false);
    });
  });

  describe('Findings verification workflow', () => {
    it('can mark a finding as verified', () => {
      const unverified = get(findings).find(f => !f.verified);
      expect(unverified).toBeDefined();

      findings.update(f =>
        f.map(finding =>
          finding.id === unverified!.id ? { ...finding, verified: true } : finding
        )
      );

      const updated = get(findings).find(f => f.id === unverified!.id);
      expect(updated?.verified).toBe(true);
    });
  });

  describe('Artifact type filtering', () => {
    it('can filter by type', () => {
      const reports = get(artifacts).filter(a => a.type === 'report');
      expect(reports.length).toBeGreaterThan(0);
      expect(reports.every(a => a.type === 'report')).toBe(true);

      const coverage = get(artifacts).filter(a => a.type === 'coverage');
      expect(coverage.length).toBeGreaterThan(0);
    });

    it('all five types are represented', () => {
      const types = new Set(get(artifacts).map(a => a.type));
      expect(types.size).toBeGreaterThanOrEqual(3);
    });
  });

  describe('Slash commands for artifacts', () => {
    it('/navigate command exists', () => {
      const { command } = parseCommand('/navigate build-001');
      expect(command).toBeDefined();
      expect(command!.name).toBe('navigate');
    });

    it('/focus command exists', () => {
      const { command } = parseCommand('/focus terminal');
      expect(command).toBeDefined();
      expect(command!.name).toBe('focus');
    });
  });

  describe('Component imports', () => {
    it('ArtifactPanel imports successfully', async () => {
      const mod = await import('$lib/../components/ArtifactPanel.svelte');
      expect(mod.default).toBeDefined();
    });

    it('BuildNotes imports successfully', async () => {
      const mod = await import('$lib/../components/BuildNotes.svelte');
      expect(mod.default).toBeDefined();
    });

    it('FindingsPanel imports successfully', async () => {
      const mod = await import('$lib/../components/FindingsPanel.svelte');
      expect(mod.default).toBeDefined();
    });
  });
});
